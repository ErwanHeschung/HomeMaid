//! PC agent: lets the phone drive this PC over a direct WebSocket (`/ws`, test page on `/remote`)
//! and exposes the PC's state to the core's `pc` plugin (`POST /api`).
//!
//! Runs in the user's session: a Windows service (session 0) cannot inject input into the desktop.
//! All desktop access happens on one dedicated thread.

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(windows), path = "fallback.rs")]
mod platform;

#[cfg(not(windows))]
pub use platform::recorded;

use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    net::SocketAddr,
    path::PathBuf,
    process::Command,
    sync::{Arc, RwLock, Weak, mpsc},
    thread,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{
        ConnectInfo, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Notify, oneshot, watch};

use crate::{
    api::{authorized, hello_response, pair_response},
    config::{self, AgentConfig, Shortcut},
    pairing::{self, hostname},
};

const MAX_TEXT: usize = 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Relative pointer move, in pixels.
    Move { dx: f64, dy: f64 },
    /// In wheel notches; fractions accumulate.
    Scroll { dx: f64, dy: f64 },
    Click { button: Button },
    Text { text: String },
    Key { key: Key },
    Media { action: Media },
    SetVolume { level: u8 },
    SetMute { muted: bool },
    Launch { app: String },
    /// Opens a `[[agent.shortcut]]`, by name.
    Open { shortcut: String },
    /// One of a shortcut's playback actions: its key combo, sent to the focused window.
    Control { shortcut: String, action: String },
    Power { action: Power },
    State,
    /// The apps found in the Start menu (`discover_apps`), by name.
    ListApps,
    /// From the phone, written to the config file (the agent then restarts to read it): a site
    /// (a URL only: the phone never adds a program), created or replaced by name.
    SaveShortcut { shortcut: Shortcut },
    DeleteShortcut { name: String },
    /// An app found in the Start menu, on the remote's strip (`[agent.apps]`), or off it.
    PinApp { name: String },
    UnpinApp { name: String },
}

impl Request {
    /// What its answer is called: `{"type": "apps", "apps": [...]}`, `{"type": "saved", ...}`.
    fn answer(&self) -> &'static str {
        match self {
            Request::ListApps => "apps",
            Request::SaveShortcut { .. } | Request::DeleteShortcut { .. } | Request::PinApp { .. } | Request::UnpinApp { .. } => "saved",
            _ => "state",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    Lock,
    Sleep,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Enter,
    Backspace,
    Delete,
    Escape,
    Tab,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Mute,
    /// 5 s back or forward: through the player's media session when it can seek, else the arrows.
    Back,
    Forward,
}

/// What is in front on the PC, to find the launcher the remote is on.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Foreground {
    pub title: Option<String>,
    /// The program's file name, lowercase (`vlc.exe`).
    pub exe: Option<String>,
    /// Its AppUserModelID (`Chrome`, `SpotifyAB.SpotifyMusic_…!Spotify`), what the Start menu
    /// and the media sessions name apps by.
    pub aumid: Option<String>,
    /// The window is the default browser's; `url` is its active tab's address when it could be
    /// read (not in fullscreen: the bar is hidden).
    pub browser: bool,
    pub url: Option<String>,
}

/// What a media session (Windows' media flyout) says is playing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

/// Answer to [`Request::State`], read by the `pc` plugin.
#[derive(Debug, Serialize)]
struct PcState {
    hostname: String,
    volume: Option<u8>,
    muted: Option<bool>,
    /// Percent.
    cpu: f32,
    /// Percent.
    memory: f32,
    active_window: Option<String>,
    /// The remote's launcher (`[[agent.shortcut]]` or `[agent.apps]`) in front; none is the
    /// generic keys.
    launcher: Option<String>,
    playing: Option<NowPlaying>,
    /// Names of `[agent.apps]` and `[[agent.shortcut]]`: what `launch` and `open` accept.
    apps: Vec<String>,
    shortcuts: Vec<String>,
}

type Job = (Request, oneshot::Sender<Result<Value, String>>);

/// Apps found on this PC, name to launch target (an AppID of the Start menu).
type Discovered = Arc<RwLock<BTreeMap<String, String>>>;

/// How often the Start menu is read again (apps installed meanwhile).
const DISCOVERY_PERIOD: Duration = Duration::from_secs(600);

/// An app's AppID, and where its icon (PNG) goes; none when it has none.
type IconJob = (String, oneshot::Sender<Option<Arc<Vec<u8>>>>);

/// What lets the phone change the agent's own config: the file, and how to restart the agent so
/// it reads it again. Without it (tests), those requests are refused.
#[derive(Clone)]
pub struct Manage {
    pub config_path: PathBuf,
    pub restart: Arc<Notify>,
}

#[derive(Clone)]
struct Agent {
    jobs: mpsc::Sender<Job>,
    manage: Option<Manage>,
    discovered: Discovered,
    /// `[agent.apps]`: a pinned app (`shell:AppsFolder\<AppID>`) has its icon too.
    apps: Arc<BTreeMap<String, PathBuf>>,
    icons: mpsc::Sender<IconJob>,
    token: Arc<str>,
    hello: Arc<str>,
    pair_page: Arc<str>,
    /// Changes (sender dropped) when the agent stops: live remote sessions end with it, or a
    /// phone would keep the PC after "PC control" is switched off.
    stopped: watch::Receiver<()>,
}

impl Agent {
    async fn run(&self, request: Request) -> Result<Value, String> {
        if request.answer() == "saved" {
            return self.change(request).await;
        }
        let (reply, answer) = oneshot::channel();
        self.jobs.send((request, reply)).map_err(|_| "desktop thread stopped")?;
        answer.await.map_err(|_| "desktop thread stopped")?
    }
}

impl Agent {
    /// Writes the phone's change to the config file, then restarts the agent to read it, a moment
    /// later so this answer goes out first; the phone reconnects at once.
    async fn change(&self, request: Request) -> Result<Value, String> {
        let manage = self.manage.clone().ok_or("this agent has no config file to change")?;
        let discovered = self.discovered.clone();
        let path = manage.config_path.clone();
        tokio::task::spawn_blocking(move || match request {
            Request::SaveShortcut { shortcut } => {
                check_shortcut(&shortcut)?;
                config::save_shortcut(&path, &shortcut)
            }
            Request::DeleteShortcut { name } => config::delete_shortcut(&path, &name),
            Request::PinApp { name } => {
                let id = discovered.read().unwrap().get(&name).cloned().ok_or(format!("`{name}` is not an app of this PC's Start menu"))?;
                config::set_app(&path, &name, Some(&format!("shell:AppsFolder\\{id}")))
            }
            Request::UnpinApp { name } => config::set_app(&path, &name, None),
            _ => unreachable!("only changes come here"),
        })
        .await
        .map_err(|e| e.to_string())??;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            manage.restart.notify_one();
        });
        Ok(json!(true))
    }
}

/// A site from the phone: a name, a web address (never a program), a logo slug, valid keys.
fn check_shortcut(shortcut: &Shortcut) -> Result<(), String> {
    if shortcut.name.trim().is_empty() {
        return Err("a site needs a name".into());
    }
    if !(shortcut.open.starts_with("https://") || shortcut.open.starts_with("http://")) {
        return Err("only web addresses (http:// or https://) can be added from the phone".into());
    }
    if shortcut.icon.as_ref().is_some_and(|i| i.is_empty() || !i.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())) {
        return Err("a logo is a name like `netflix`".into());
    }
    shortcut_keys(std::slice::from_ref(shortcut)).map(drop)
}

/// Starts the desktop thread and returns the agent's routes. Remote sessions end when `stopping`
/// is dropped; with `manage`, the phone can change the agent's sites and apps.
pub fn router(config: &AgentConfig, stopping: &watch::Sender<()>, manage: Option<Manage>) -> Result<Router, String> {
    let (jobs, rx) = mpsc::channel();
    let apps = config.apps.clone();
    let shortcuts = shortcut_keys(&config.shortcuts)?;
    let opens = config.shortcuts.iter().map(|s| (s.name.clone(), s.open.clone())).collect();
    let discovered = Discovered::default();
    if config.discover_apps {
        discover(Arc::downgrade(&discovered))?;
    }
    let to_desktop = discovered.clone();
    let icons = icon_thread()?;
    thread::Builder::new()
        .name("desktop".into())
        .spawn(move || desktop(rx, apps, opens, shortcuts, to_desktop))
        .map_err(|e| e.to_string())?;
    let hello = json!({
        "type": "hello",
        "hostname": hostname(),
        "apps": config.apps.keys().collect::<Vec<_>>(),
        "discover": config.discover_apps,
        "shortcuts": config.shortcuts.iter().map(|s| json!({
            "name": s.name,
            "icon": s.icon,
            "open": s.open,
            "keys": s.keys,
            "actions": ACTIONS.iter().filter(|a| s.keys.contains_key(**a) || default_keys(a).is_some()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    let agent = Agent {
        jobs,
        manage,
        discovered,
        apps: Arc::new(config.apps.clone()),
        icons,
        token: config.token.as_str().into(),
        hello: hello.to_string().into(),
        pair_page: pairing::page("agent", config.bind, &config.token).into(),
        stopped: stopping.subscribe(),
    };
    Ok(Router::new()
        .route("/remote", get(|| async { Html(include_str!("remote.html")) }))
        .route("/pair", get(pair))
        .route("/hello", get(prove))
        .route("/icon", get(icon))
        .route("/ws", get(ws))
        .route("/api", post(api))
        .with_state(agent))
}

/// Serves until `shutdown`, or until `manage.restart` (then returns true: start it again, it
/// reads its config anew).
pub async fn run(config: AgentConfig, manage: Option<Manage>, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<bool, String> {
    let (stopping, _) = watch::channel(());
    let restart = manage.as_ref().map(|m| m.restart.clone());
    let app = router(&config, &stopping, manage)?;
    let listener = tokio::net::TcpListener::bind(config.bind).await.map_err(|e| format!("{}: {e}", config.bind))?;
    let port = config.bind.port();
    tracing::info!("agent listening on {}; to pair the app, open http://localhost:{port}/pair on this PC", config.bind);
    let _announcement = pairing::announce("agent", port);
    let restarting = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = restarting.clone();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            let restarted = async {
                match restart {
                    Some(restart) => restart.notified().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = shutdown => {}
                () = restarted => flag.store(true, std::sync::atomic::Ordering::Relaxed),
            }
            drop(stopping);
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(restarting.load(std::sync::atomic::Ordering::Relaxed))
}

async fn pair(State(agent): State<Agent>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Response {
    pair_response(peer, &headers, &agent.pair_page)
}

async fn prove(State(agent): State<Agent>, Query(query): Query<HashMap<String, String>>) -> Response {
    hello_response(&query, &agent.token)
}

async fn ws(
    State(agent): State<Agent>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !authorized(&headers, &query, &agent.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade.on_upgrade(move |socket| remote(socket, agent))
}

/// Fire-and-forget: only state answers and errors are sent back. Ends when the agent stops.
async fn remote(mut socket: WebSocket, mut agent: Agent) {
    if socket.send(Message::Text(agent.hello.as_ref().into())).await.is_err() {
        return;
    }
    loop {
        let message = tokio::select! {
            message = socket.recv() => message,
            _ = agent.stopped.changed() => break,
        };
        let Some(Ok(message)) = message else { break };
        let Message::Text(text) = message else { continue };
        let reply = match serde_json::from_str::<Request>(text.as_str()) {
            Ok(request) => {
                // An answer says what it answers: `{"type": "apps", "apps": [...]}`, else the state.
                let kind = request.answer();
                match agent.run(request).await {
                    Ok(Value::Null) => continue,
                    Ok(value) => json!({ "type": kind, kind: value }),
                    Err(e) => json!({ "type": "error", "message": e }),
                }
            }
            Err(e) => json!({ "type": "error", "message": e.to_string() }),
        };
        if socket.send(Message::Text(reply.to_string().into())).await.is_err() {
            break;
        }
    }
}

/// The thread that draws app icons, set up as the Windows shell wants it; each icon is drawn
/// once. It ends with the agent (its last sender).
fn icon_thread() -> Result<mpsc::Sender<IconJob>, String> {
    let (tx, rx) = mpsc::channel::<IconJob>();
    thread::Builder::new()
        .name("app-icons".into())
        .spawn(move || {
            platform::init_icon_thread();
            let mut drawn: HashMap<String, Option<Arc<Vec<u8>>>> = HashMap::new();
            for (app_id, reply) in rx {
                let icon = drawn.entry(app_id).or_insert_with_key(|id| platform::app_icon(id).map(Arc::new)).clone();
                let _ = reply.send(icon);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(tx)
}

/// `GET /icon?app=<name>`: the Start menu's icon of an app the agent knows (found there, or
/// pinned from it), as PNG. Never an arbitrary file: anything else is 404.
async fn icon(State(agent): State<Agent>, headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
    if !authorized(&headers, &HashMap::new(), &agent.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let name = query.get("app").map(String::as_str).unwrap_or_default();
    let pinned = agent.apps.get(name).and_then(|target| target.to_str()?.strip_prefix("shell:AppsFolder\\").map(String::from));
    let Some(app_id) = agent.discovered.read().unwrap().get(name).cloned().or(pinned) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (reply, answer) = oneshot::channel();
    if agent.icons.send((app_id, reply)).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match answer.await {
        Ok(Some(png)) => (
            [(axum::http::header::CONTENT_TYPE, "image/png"), (axum::http::header::CACHE_CONTROL, "max-age=86400")],
            png.as_ref().clone(),
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Fills `discovered` from the Start menu now, then every [`DISCOVERY_PERIOD`], for as long as the
/// agent that holds it runs.
fn discover(discovered: Weak<RwLock<BTreeMap<String, String>>>) -> Result<(), String> {
    thread::Builder::new()
        .name("app-discovery".into())
        .spawn(move || {
            loop {
                let apps = platform::start_apps();
                let Some(discovered) = discovered.upgrade() else { return };
                tracing::info!("{} apps found in the Start menu", apps.len());
                *discovered.write().unwrap() = apps;
                drop(discovered);
                thread::sleep(DISCOVERY_PERIOD);
            }
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

/// `Get-StartApps | ConvertTo-Json` (an object for one app, an array otherwise) as name to AppID,
/// without what is not an app to open from a sofa: uninstallers, read-mes, help, web links.
pub fn parse_start_apps(json: &str) -> BTreeMap<String, String> {
    const NOT_APPS: [&str; 12] =
        ["uninstall", "désinstall", "desinstall", "readme", "read me", "lisez-moi", "help", "aide", "documentation", "manual", "manuel", "website"];
    let parsed: Value = serde_json::from_str(json.trim()).unwrap_or(Value::Null);
    let entries = match parsed {
        Value::Array(entries) => entries,
        Value::Null => vec![],
        one => vec![one],
    };
    let mut apps = BTreeMap::new();
    for entry in entries {
        let (Some(name), Some(id)) = (entry["Name"].as_str(), entry["AppID"].as_str()) else { continue };
        // A web link in the Start menu ("Support Center") is not an app.
        if id.starts_with("http://") || id.starts_with("https://") {
            continue;
        }
        let words = name.to_lowercase();
        let junk = NOT_APPS.iter().any(|w| words.split(|c: char| !c.is_alphanumeric() && c != '-').any(|word| word.starts_with(w)) || words.contains(w));
        if !junk && !name.trim().is_empty() {
            apps.entry(name.trim().to_string()).or_insert_with(|| id.to_string());
        }
    }
    apps
}

/// What `launch` starts for `app`: a written app (a program, or an app of the Start menu once
/// pinned: `shell:AppsFolder\<AppID>`), else one found in the Start menu.
fn launch_target(app: &str, apps: &BTreeMap<String, PathBuf>, discovered: &BTreeMap<String, String>) -> Option<String> {
    apps.get(app)
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(|| discovered.get(app).map(|id| format!("shell:AppsFolder\\{id}")))
}

/// An app of the Start menu opens through the shell, as from the Start menu; a program runs.
fn launch(target: &str) -> Result<(), String> {
    let mut command = if target.starts_with("shell:AppsFolder\\") {
        let mut c = Command::new("explorer.exe");
        c.arg(target);
        c
    } else {
        Command::new(target)
    };
    command.spawn().map(drop).map_err(|e| format!("cannot launch {target}: {e}"))
}

async fn api(State(agent): State<Agent>, headers: HeaderMap, Json(request): Json<Request>) -> Response {
    if !authorized(&headers, &HashMap::new(), &agent.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match agent.run(request).await {
        Ok(value) => Json(value).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

/// Playback actions a shortcut can bind; the first three work in most web players by default.
const ACTIONS: [&str; 6] = ["play_pause", "back", "forward", "restart", "next_episode", "skip_intro"];

fn default_keys(action: &str) -> Option<&'static str> {
    match action {
        "play_pause" => Some("space"),
        "back" => Some("left"),
        "forward" => Some("right"),
        _ => None,
    }
}

/// Every shortcut's combos, parsed up front so a typo fails at startup, not on the sofa.
fn shortcut_keys(shortcuts: &[Shortcut]) -> Result<HashMap<(String, String), Combo>, String> {
    let mut out = HashMap::new();
    for s in shortcuts {
        for (action, keys) in &s.keys {
            if !ACTIONS.contains(&action.as_str()) {
                return Err(format!("shortcut `{}`: unknown action `{action}` (one of {})", s.name, ACTIONS.join(", ")));
            }
            if keys.is_empty() {
                continue;
            }
            let combo = Combo::parse(keys).map_err(|e| format!("shortcut `{}`, {action}: {e}", s.name))?;
            out.insert((s.name.clone(), action.clone()), combo);
        }
        for action in ACTIONS {
            if let (None, Some(keys)) = (s.keys.get(action), default_keys(action)) {
                out.insert((s.name.clone(), action.into()), Combo::parse(keys)?);
            }
        }
    }
    Ok(out)
}

/// Modifiers held around one key, e.g. `shift+n`.
#[derive(Debug, Clone, PartialEq)]
pub struct Combo {
    pub modifiers: Vec<Modifier>,
    pub key: ComboKey,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComboKey {
    Named(Key),
    Char(char),
}

impl Combo {
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<String> = text.split('+').map(|p| p.trim().to_lowercase()).collect();
        let (key, mods) = parts.split_last().ok_or("empty key combo")?;
        let modifiers = mods
            .iter()
            .map(|m| match m.as_str() {
                "shift" => Ok(Modifier::Shift),
                "ctrl" | "control" => Ok(Modifier::Ctrl),
                "alt" => Ok(Modifier::Alt),
                "meta" | "win" | "cmd" => Ok(Modifier::Meta),
                other => Err(format!("unknown modifier `{other}` in `{text}`")),
            })
            .collect::<Result<_, _>>()?;
        let mut chars = key.chars();
        let key = match (chars.next(), chars.next()) {
            (Some(c), None) => ComboKey::Char(c),
            _ => ComboKey::Named(
                serde_json::from_value(Value::String(if key == "esc" { "escape".into() } else { key.clone() }))
                    .map_err(|_| format!("unknown key `{key}` in `{text}`"))?,
            ),
        };
        Ok(Self { modifiers, key })
    }
}

/// A URL goes to the default browser; anything else is started as a program.
fn open(target: &str) -> Result<(), String> {
    let mut command = if target.starts_with("http://") || target.starts_with("https://") {
        let mut c = Command::new(if cfg!(windows) { "explorer" } else { "xdg-open" });
        c.arg(target);
        c
    } else {
        Command::new(target)
    };
    command.spawn().map(drop).map_err(|e| format!("cannot open {target}: {e}"))
}

fn desktop(
    jobs: mpsc::Receiver<Job>,
    apps: BTreeMap<String, PathBuf>,
    opens: HashMap<String, String>,
    shortcuts: HashMap<(String, String), Combo>,
    discovered: Discovered,
) {
    let mut desktop = match platform::Desktop::new() {
        Ok(desktop) => desktop,
        Err(e) => {
            tracing::error!("agent cannot access the desktop: {e}");
            return;
        }
    };
    let mut system = sysinfo::System::new();
    // CPU usage is a delta between refreshes: take the baseline now, or the first reading is bogus.
    system.refresh_cpu_usage();
    for (request, reply) in jobs {
        let result = match request {
            Request::State => {
                system.refresh_cpu_usage();
                system.refresh_memory();
                let volume = desktop.volume();
                let front = desktop.foreground();
                let state = PcState {
                    hostname: hostname(),
                    volume: volume.map(|(level, _)| level),
                    muted: volume.map(|(_, muted)| muted),
                    cpu: system.global_cpu_usage(),
                    memory: (system.used_memory() as f64 * 100.0 / system.total_memory().max(1) as f64) as f32,
                    launcher: launcher_in(&front, &opens, &apps),
                    playing: desktop.now_playing(&front),
                    active_window: front.title,
                    // The automation editor offers them all.
                    apps: apps.keys().chain(discovered.read().unwrap().keys()).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect(),
                    shortcuts: {
                        let mut names: Vec<String> = opens.keys().cloned().collect();
                        names.sort();
                        names
                    },
                };
                Ok(json!(state))
            }
            Request::Launch { app } => match launch_target(&app, &apps, &discovered.read().unwrap()) {
                Some(target) => launch(&target).map(|()| Value::Null),
                None => Err(format!("app `{app}` is not in [agent.apps]")),
            },
            Request::ListApps => Ok(json!(discovered.read().unwrap().keys().collect::<Vec<_>>())),
            Request::Open { shortcut } => match opens.get(&shortcut) {
                Some(target) => open(target).map(|()| Value::Null),
                None => Err(format!("no shortcut `{shortcut}` in [[agent.shortcut]]")),
            },
            Request::Control { shortcut, action } => match shortcuts.get(&(shortcut.clone(), action.clone())) {
                Some(combo) => desktop.press(combo).map(|()| Value::Null),
                None => Err(format!("`{shortcut}` has no key for {action}")),
            },
            request => validate(&request).and_then(|()| desktop.handle(&request)).map(|()| Value::Null),
        };
        let _ = reply.send(result);
    }
}

/// The launcher in front: the site of the browser's address (its host, or a subdomain of it; the
/// longest wins, so music.youtube.com beats youtube.com), else the app of the program (its
/// AppID or file name), else, last resort, the longest launcher name in the window title.
fn launcher_in(front: &Foreground, opens: &HashMap<String, String>, apps: &BTreeMap<String, PathBuf>) -> Option<String> {
    let site = front.url.as_deref().and_then(|url| host(url, false)).and_then(|at| {
        opens
            .iter()
            .filter_map(|(name, open)| Some((name, host(open, true)?)))
            .filter(|(_, h)| at == *h || at.ends_with(&format!(".{h}")))
            .max_by_key(|(_, h)| h.len())
            .map(|(name, _)| name.clone())
    });
    // A shortcut that opens a program is an app too.
    let programs = || apps.iter().map(|(n, p)| (n, p.to_string_lossy().into_owned())).chain(opens.iter().filter(|(_, o)| host(o, true).is_none()).map(|(n, o)| (n, o.clone())));
    let app = || programs().find(|(_, target)| same_app(target.strip_prefix("shell:AppsFolder\\").unwrap_or(target), front)).map(|(n, _)| n.clone());
    let titled = || {
        let title = front.title.as_deref()?.to_lowercase();
        opens.keys().chain(apps.keys()).filter(|n| title.contains(&n.to_lowercase())).max_by_key(|n| n.len()).cloned()
    };
    // A browser whose address is unknown: its title names the site better than the browser.
    if front.browser { site.or_else(titled).or_else(app) } else { site.or_else(app).or_else(titled) }
}

/// Whether a media session's source is the program in front: an app id, or one named after the
/// program (Opera's `OperaSoftware.OperaGXWebBrowser.1788635019` for `opera.exe`).
// ponytail: a name inside the id; a short program name could match another app's session, then
// the session of the flyout (`GetCurrentSession`) is the safer pick.
#[cfg_attr(not(windows), allow(dead_code))]
fn plays_in(source: &str, front: &Foreground) -> bool {
    let stem = front.exe.as_deref().map(|e| e.strip_suffix(".exe").unwrap_or(e));
    same_app(source, front) || stem.is_some_and(|stem| stem.len() >= 4 && source.to_lowercase().contains(stem))
}

/// Whether an app id (an AppUserModelID, a program path) is the program in front.
fn same_app(id: &str, front: &Foreground) -> bool {
    let file = id.rsplit(['\\', '/']).next().unwrap_or(id).to_lowercase();
    front.aumid.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(id))
        || front.exe.as_deref().is_some_and(|exe| exe == file || exe.strip_suffix(".exe") == Some(&file))
}

/// The host of a web address, without `www.`: with `scheme`, only of an `http(s)://` one (a
/// shortcut's `open`), otherwise also bare as browsers show them (`crunchyroll.com/watch/…`).
fn host(address: &str, scheme: bool) -> Option<String> {
    let rest = address.strip_prefix("https://").or_else(|| address.strip_prefix("http://"));
    let rest = if scheme { rest? } else { rest.unwrap_or(address) };
    let host = rest.split(['/', '?', '#', ':']).next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (host.contains('.') && !host.contains(|c: char| c.is_whitespace() || c == '\\')).then(|| host.to_string())
}

/// Where a 5 s seek lands, in the media sessions' 100 ns ticks: the position was read at
/// `updated` and has moved on since if playing; kept within 0 and the end, when it has one.
#[cfg_attr(not(windows), allow(dead_code))]
fn seek_to(position: i64, updated: i64, now: i64, playing: bool, delta: i64, end: i64) -> i64 {
    let at = if playing { position + (now - updated).max(0) } else { position };
    let to = (at + delta).max(0);
    if end > 0 { to.min(end) } else { to }
}

/// Values come from the network: reject what the desktop APIs should never see.
fn validate(request: &Request) -> Result<(), String> {
    match request {
        Request::Move { dx, dy } | Request::Scroll { dx, dy } if !(dx.is_finite() && dy.is_finite()) => {
            Err("non-finite delta".into())
        }
        Request::Text { text } if text.chars().count() > MAX_TEXT => Err(format!("text longer than {MAX_TEXT} chars")),
        Request::SetVolume { level } if *level > 100 => Err("volume is 0-100".into()),
        _ => Ok(()),
    }
}

/// Adds fractional deltas to `rest` and takes out the whole steps, so slow moves are not lost.
#[cfg_attr(not(windows), allow(dead_code))]
fn take_whole(rest: &mut (f64, f64), dx: f64, dy: f64) -> (i32, i32) {
    rest.0 += dx;
    rest.1 += dy;
    let (x, y) = (rest.0.trunc(), rest.1.trunc());
    rest.0 -= x;
    rest.1 -= y;
    (x as i32, y as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_combos_parse_and_shortcuts_get_player_defaults() {
        assert_eq!(Combo::parse("shift+n"), Ok(Combo { modifiers: vec![Modifier::Shift], key: ComboKey::Char('n') }));
        assert_eq!(Combo::parse("Space"), Ok(Combo { modifiers: vec![], key: ComboKey::Named(Key::Space) }));
        assert_eq!(Combo::parse("esc").unwrap().key, ComboKey::Named(Key::Escape));
        assert!(Combo::parse("hyper+x").is_err());
        assert!(Combo::parse("f99").is_err());

        let netflix = Shortcut {
            name: "Netflix".into(),
            icon: Some("netflix".into()),
            open: "https://www.netflix.com".into(),
            keys: BTreeMap::from([("skip_intro".into(), "s".into())]),
        };
        let keys = shortcut_keys(std::slice::from_ref(&netflix)).unwrap();
        assert_eq!(keys[&("Netflix".into(), "skip_intro".into())].key, ComboKey::Char('s'));
        assert_eq!(keys[&("Netflix".into(), "play_pause".into())].key, ComboKey::Named(Key::Space), "web player default");
        assert!(!keys.contains_key(&("Netflix".into(), "next_episode".into())));

        let typo = Shortcut { keys: BTreeMap::from([("skip".into(), "s".into())]), ..netflix };
        assert!(shortcut_keys(&[typo]).is_err(), "unknown actions fail at startup");
    }

    #[test]
    fn start_menu_apps_are_parsed_without_the_junk() {
        let json = r#"[{"Name":"Accès vocal","AppID":"{1AC1}\\voiceaccess.exe"},
            {"Name":"Steam","AppID":"Valve.Steam"},{"Name":"Steam","AppID":"duplicate"},
            {"Name":"Uninstall Steam","AppID":"x"},{"Name":"Désinstaller Foo","AppID":"x"},
            {"Name":"Foo Help","AppID":"x"},{"Name":"Aide de Foo","AppID":"x"},{"Name":"Lisez-moi","AppID":"x"},
            {"Name":"Assistance rapide","AppID":"QuickAssist!App"},{"Name":"Apex Legends","AppID":"steam://rungameid/1172470"},
            {"Name":"Steam Support Center","AppID":"http://support.steampowered.com/"}]"#;
        let apps = parse_start_apps(json);
        assert_eq!(apps.keys().collect::<Vec<_>>(), ["Accès vocal", "Apex Legends", "Assistance rapide", "Steam"]);
        assert_eq!(apps["Steam"], "Valve.Steam", "the first of two");
        assert_eq!(parse_start_apps(r#"{"Name":"Only","AppID":"one"}"#).len(), 1, "one app is an object");
        assert!(parse_start_apps("").is_empty());

        let written = BTreeMap::from([("notepad".to_string(), PathBuf::from("notepad.exe"))]);
        assert_eq!(launch_target("notepad", &written, &apps).as_deref(), Some("notepad.exe"));
        assert_eq!(launch_target("Steam", &written, &apps).as_deref(), Some("shell:AppsFolder\\Valve.Steam"));
        assert_eq!(launch_target("rm", &written, &apps), None);
    }

    #[test]
    fn the_launcher_in_front_is_found_by_address_then_program_then_title() {
        let opens = HashMap::from([
            ("YouTube".to_string(), "https://www.youtube.com".to_string()),
            ("YouTube Music".to_string(), "https://music.youtube.com".to_string()),
            ("Crunchyroll".to_string(), "https://www.crunchyroll.com".to_string()),
            ("Kodi".to_string(), "C:\\Program Files\\Kodi\\kodi.exe".to_string()),
        ]);
        let apps = BTreeMap::from([
            ("VLC".to_string(), PathBuf::from("shell:AppsFolder\\{6D809377-6AF0-444B-8957-A3773F02200E}\\VideoLAN\\VLC\\vlc.exe")),
            ("Spotify".to_string(), PathBuf::from("shell:AppsFolder\\SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify")),
            ("notepad".to_string(), PathBuf::from("notepad.exe")),
        ]);
        let at = |front: Foreground| launcher_in(&front, &opens, &apps);
        let browser = |url: &str| Foreground { exe: Some("chrome.exe".into()), aumid: Some("Chrome".into()), url: Some(url.into()), title: Some("Frieren".into()), browser: true };
        assert_eq!(at(browser("crunchyroll.com/fr/watch/G0DUND0K2")).as_deref(), Some("Crunchyroll"), "bare, as Chrome shows it");
        assert_eq!(at(browser("https://www.youtube.com/watch?v=x")).as_deref(), Some("YouTube"));
        assert_eq!(at(browser("music.youtube.com/watch?v=x")).as_deref(), Some("YouTube Music"), "the longest host");
        assert_eq!(at(browser("notyoutube.com")), None, "a host, not a suffix");
        assert_eq!(at(Foreground { exe: Some("vlc.exe".into()), ..Default::default() }).as_deref(), Some("VLC"));
        assert_eq!(at(Foreground { exe: Some("kodi.exe".into()), ..Default::default() }).as_deref(), Some("Kodi"), "a shortcut to a program");
        assert_eq!(at(Foreground { exe: Some("notepad.exe".into()), ..Default::default() }).as_deref(), Some("notepad"));
        let spotify = Foreground { exe: Some("spotify.exe".into()), aumid: Some("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify".into()), title: Some("Daft Punk - One More Time".into()), ..Default::default() };
        assert_eq!(at(spotify).as_deref(), Some("Spotify"), "by AppID: its title is the song");
        assert_eq!(at(Foreground { title: Some("Frieren - Crunchyroll".into()), ..Default::default() }).as_deref(), Some("Crunchyroll"), "title, last");
        assert_eq!(at(Foreground { exe: Some("code.exe".into()), title: Some("main.rs - Visual Studio Code".into()), ..Default::default() }), None);

        let mut apps = apps;
        apps.insert("Opera".into(), PathBuf::from("opera.exe"));
        let fullscreen = Foreground { exe: Some("opera.exe".into()), title: Some("Wolverine - YouTube - Opera".into()), browser: true, ..Default::default() };
        assert_eq!(launcher_in(&fullscreen, &opens, &apps).as_deref(), Some("YouTube"), "the title before the browser");

        let opera = Foreground { exe: Some("opera.exe".into()), ..Default::default() };
        assert!(plays_in("OperaSoftware.OperaGXWebBrowser.1788635019", &opera), "read on the user's PC");
        assert!(plays_in("Spotify.exe", &Foreground { exe: Some("spotify.exe".into()), ..Default::default() }));
        assert!(plays_in("Chrome", &Foreground { exe: Some("chrome.exe".into()), ..Default::default() }));
        assert!(!plays_in("Spotify.exe", &opera));
        assert!(!plays_in("Chrome", &Foreground::default()));
    }

    #[test]
    fn seeking_counts_the_time_played_since_the_position_was_read() {
        const S: i64 = 10_000_000;
        assert_eq!(seek_to(60 * S, 0, 2 * S, true, 5 * S, 0), 67 * S);
        assert_eq!(seek_to(60 * S, 0, 2 * S, false, -5 * S, 0), 55 * S, "paused: where it was");
        assert_eq!(seek_to(2 * S, 0, 0, false, -5 * S, 0), 0);
        assert_eq!(seek_to(98 * S, 0, 0, false, 5 * S, 100 * S), 100 * S);
    }

    #[test]
    fn fractional_moves_accumulate() {
        let mut rest = (0.0, 0.0);
        assert_eq!(take_whole(&mut rest, 0.6, -0.6), (0, 0));
        assert_eq!(take_whole(&mut rest, 0.6, -0.6), (1, -1));
        assert_eq!(take_whole(&mut rest, 2.5, 0.0), (2, 0));
    }

    #[test]
    fn requests_from_the_network_are_validated() {
        assert!(validate(&Request::Move { dx: f64::NAN, dy: 0.0 }).is_err());
        assert!(validate(&Request::SetVolume { level: 101 }).is_err());
        assert!(validate(&Request::Text { text: "a".repeat(MAX_TEXT + 1) }).is_err());
        assert!(validate(&Request::Move { dx: 3.0, dy: -2.0 }).is_ok());
        let control: Request = serde_json::from_str(r#"{"type":"control","shortcut":"Netflix","action":"skip_intro"}"#).unwrap();
        assert_eq!(control, Request::Control { shortcut: "Netflix".into(), action: "skip_intro".into() });
        let power: Request = serde_json::from_str(r#"{"type":"power","action":"shutdown"}"#).unwrap();
        assert_eq!(power, Request::Power { action: Power::Shutdown });
        let parsed: Request = serde_json::from_str(r#"{"type":"media","action":"play_pause"}"#).unwrap();
        assert_eq!(parsed, Request::Media { action: Media::PlayPause });
    }
}

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
    sync::{Arc, mpsc},
    thread,
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
use tokio::sync::{oneshot, watch};

use crate::{
    api::{authorized, hello_response, pair_response},
    config::{AgentConfig, Shortcut},
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
}

type Job = (Request, oneshot::Sender<Result<Value, String>>);

#[derive(Clone)]
struct Agent {
    jobs: mpsc::Sender<Job>,
    token: Arc<str>,
    hello: Arc<str>,
    pair_page: Arc<str>,
    /// Changes (sender dropped) when the agent stops: live remote sessions end with it, or a
    /// phone would keep the PC after "PC control" is switched off.
    stopped: watch::Receiver<()>,
}

impl Agent {
    async fn run(&self, request: Request) -> Result<Value, String> {
        let (reply, answer) = oneshot::channel();
        self.jobs.send((request, reply)).map_err(|_| "desktop thread stopped")?;
        answer.await.map_err(|_| "desktop thread stopped")?
    }
}

/// Starts the desktop thread and returns the agent's routes. Remote sessions end when `stopping`
/// is dropped.
pub fn router(config: &AgentConfig, stopping: &watch::Sender<()>) -> Result<Router, String> {
    let (jobs, rx) = mpsc::channel();
    let apps = config.apps.clone();
    let shortcuts = shortcut_keys(&config.shortcuts)?;
    let opens = config.shortcuts.iter().map(|s| (s.name.clone(), s.open.clone())).collect();
    thread::Builder::new()
        .name("desktop".into())
        .spawn(move || desktop(rx, apps, opens, shortcuts))
        .map_err(|e| e.to_string())?;
    let hello = json!({
        "type": "hello",
        "hostname": hostname(),
        "apps": config.apps.keys().collect::<Vec<_>>(),
        "shortcuts": config.shortcuts.iter().map(|s| json!({
            "name": s.name,
            "icon": s.icon,
            "actions": ACTIONS.iter().filter(|a| s.keys.contains_key(**a) || default_keys(a).is_some()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    let agent = Agent {
        jobs,
        token: config.token.as_str().into(),
        hello: hello.to_string().into(),
        pair_page: pairing::page("agent", config.bind, &config.token).into(),
        stopped: stopping.subscribe(),
    };
    Ok(Router::new()
        .route("/remote", get(|| async { Html(include_str!("remote.html")) }))
        .route("/pair", get(pair))
        .route("/hello", get(prove))
        .route("/ws", get(ws))
        .route("/api", post(api))
        .with_state(agent))
}

pub async fn run(config: AgentConfig, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), String> {
    let (stopping, _) = watch::channel(());
    let app = router(&config, &stopping)?;
    let listener = tokio::net::TcpListener::bind(config.bind).await.map_err(|e| format!("{}: {e}", config.bind))?;
    let port = config.bind.port();
    tracing::info!("agent listening on {}; to pair the app, open http://localhost:{port}/pair on this PC", config.bind);
    let _announcement = pairing::announce("agent", port);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            shutdown.await;
            drop(stopping);
        })
        .await
        .map_err(|e| e.to_string())
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
            Ok(request) => match agent.run(request).await {
                Ok(Value::Null) => continue,
                Ok(state) => json!({ "type": "state", "state": state }),
                Err(e) => json!({ "type": "error", "message": e }),
            },
            Err(e) => json!({ "type": "error", "message": e.to_string() }),
        };
        if socket.send(Message::Text(reply.to_string().into())).await.is_err() {
            break;
        }
    }
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
                let state = PcState {
                    hostname: hostname(),
                    volume: volume.map(|(level, _)| level),
                    muted: volume.map(|(_, muted)| muted),
                    cpu: system.global_cpu_usage(),
                    memory: (system.used_memory() as f64 * 100.0 / system.total_memory().max(1) as f64) as f32,
                    active_window: desktop.active_window(),
                };
                Ok(json!(state))
            }
            Request::Launch { app } => match apps.get(&app) {
                Some(path) => Command::new(path)
                    .spawn()
                    .map(|_| Value::Null)
                    .map_err(|e| format!("cannot launch {}: {e}", path.display())),
                None => Err(format!("app `{app}` is not in [agent.apps]")),
            },
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

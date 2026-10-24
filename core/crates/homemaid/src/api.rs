//! Public API: JSON-RPC 2.0 over WebSocket on `/ws`.
//!
//! Methods: `entities.list`, `entity.set_desired { id, cap, value }`,
//! `entity.update { id, name?, room?, tags?, model? }` (saved to the config file), `rooms.list`,
//! `room.update { name, kind }` (saved too; empty kind clears it), `entity.invoke { id, action,
//! params? }` (a one-shot action), `automations.list`, `automation.dry_run { id }` or
//! `{ automation }` (one not saved yet), `automation.run { id }`, `automation.save { automation }`
//! (created or replaced by id, saved to the config file, in force at once), `automation.delete
//! { id }`, `subscribe` (then `state.changed { id, cap, slot }`, `entity.changed { entity }`,
//! `room.changed { name, kind }`, `automation.ran { id, run }` and `automations.changed`
//! notifications), `home.location` and `home.set_location { latitude, longitude }` (where the
//! home is, for sunrise and sunset; saved to the config file).
//! Plugins: `plugins.list` (installed ones), `registry.list` (what the registry offers; the only
//! methods with `plugin.install` that use the Internet), `plugin.install { id, version, permissions,
//! settings? }` (the version and permissions the user accepted; an update keeps the settings),
//! `plugin.configure { id, settings }` and `plugin.remove { id }`, then `plugins.changed`.
//! Devices added from the app (plugin-api 1.3): `device.add { plugin, settings }` (the plugin
//! checks it first; returns its `id`) and `device.remove { plugin, id }`, then `plugins.changed`.
//! `/pair` serves the pairing QR code to the PC itself; `/hello?nonce=` proves this host knows
//! the token (see [`pairing::proof`]).

use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Instant,
};

use axum::{
    Router,
    extract::{
        ConnectInfo, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::{
    Core, Event,
    automations::{AutomationConfig, Automations, Location},
    config::{self, DeviceConfig, EntityOverride, PluginConfig, RoomConfig},
    model::{CapKind, CapValue},
    pairing, registry, store,
};

#[derive(Clone)]
struct AppState {
    core: Arc<Core>,
    token: Arc<str>,
    config_path: Arc<Path>,
    data_dir: Arc<Path>,
    registry: Arc<str>,
    pair_page: Arc<str>,
}

/// Serve with `into_make_service_with_connect_info::<SocketAddr>()`: `/pair` checks the peer.
pub fn router(core: Arc<Core>, config: &config::CoreConfig, config_path: &Path) -> Router {
    let pair_page = pairing::page("core", config.bind, &config.token);
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ws", get(ws))
        .route("/pair", get(pair))
        .route("/hello", get(prove))
        .with_state(AppState {
            core,
            token: config.token.as_str().into(),
            config_path: config_path.into(),
            data_dir: config.data_dir.as_path().into(),
            registry: config.registry.as_deref().unwrap_or(registry::DEFAULT_URL).into(),
            pair_page: pair_page.into(),
        })
}

/// Also used by the agent.
pub(crate) fn pair_response(peer: SocketAddr, headers: &HeaderMap, page: &str) -> Response {
    if !pairing::is_local_request(peer, headers) {
        return (StatusCode::FORBIDDEN, "open this page on the PC itself: http://localhost:<port>/pair").into_response();
    }
    Html(page.to_string()).into_response()
}

/// Also used by the agent.
pub(crate) fn hello_response(query: &HashMap<String, String>, token: &str) -> Response {
    match query.get("nonce").filter(|n| (16..=128).contains(&n.len())) {
        Some(nonce) => pairing::proof(token, nonce).into_response(),
        None => (StatusCode::BAD_REQUEST, "nonce: 16 to 128 characters").into_response(),
    }
}

async fn pair(State(app): State<AppState>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Response {
    pair_response(peer, &headers, &app.pair_page)
}

async fn prove(State(app): State<AppState>, Query(query): Query<HashMap<String, String>>) -> Response {
    hello_response(&query, &app.token)
}

async fn ws(
    State(app): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !authorized(&headers, &query, &app.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade.on_upgrade(move |socket| session(socket, app))
}

/// Token given as `Authorization: Bearer <token>` or `?token=<token>`.
pub(crate) fn authorized(headers: &HeaderMap, query: &HashMap<String, String>, token: &str) -> bool {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let given = bearer.or(query.get("token").map(String::as_str));
    given.is_some_and(|t| constant_time_eq(t.as_bytes(), token.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Methods that wait for a plugin (an action: ms usually, 70 s at worst) or the registry. They run aside so the
/// notifications keep flowing; the others answer in order with them, so `entities.list` is never
/// overtaken by an older change.
const SLOW: [&str; 8] =
    ["entity.invoke", "automation.run", "registry.list", "plugin.install", "plugin.configure", "plugin.remove", "device.add", "device.remove"];

async fn session(mut socket: WebSocket, app: AppState) {
    let mut changes = app.core.changes.subscribe();
    let mut subscribed = false;
    let (replies, mut replied) = tokio::sync::mpsc::unbounded_channel();
    loop {
        let reply = tokio::select! {
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(text))) => match serde_json::from_str::<Request>(text.as_str()) {
                    Err(e) => error(Value::Null, -32700, e.to_string()),
                    Ok(req) if req.method == "subscribe" => {
                        subscribed = true;
                        json!({ "jsonrpc": "2.0", "id": req.id, "result": true })
                    }
                    Ok(req) if SLOW.contains(&req.method.as_str()) => {
                        let (app, replies) = (app.clone(), replies.clone());
                        tokio::spawn(async move { drop(replies.send(handle(&app, req).await)) });
                        continue;
                    }
                    Ok(req) => handle(&app, req).await,
                },
                Some(Ok(_)) => continue,
                _ => break,
            },
            Some(reply) = replied.recv() => reply,
            change = changes.recv() => match change {
                Ok(Event::Slot(change)) if subscribed => json!({ "jsonrpc": "2.0", "method": "state.changed", "params": change }),
                Ok(Event::Entity(entity)) if subscribed => json!({ "jsonrpc": "2.0", "method": "entity.changed", "params": { "entity": entity } }),
                Ok(Event::Room(room)) if subscribed => json!({ "jsonrpc": "2.0", "method": "room.changed", "params": room }),
                Ok(Event::Automation { id, run }) if subscribed => {
                    json!({ "jsonrpc": "2.0", "method": "automation.ran", "params": { "id": id, "run": run } })
                }
                Ok(Event::Automations) if subscribed => json!({ "jsonrpc": "2.0", "method": "automations.changed", "params": {} }),
                Ok(Event::Plugins) if subscribed => json!({ "jsonrpc": "2.0", "method": "plugins.changed", "params": {} }),
                Ok(_) => continue,
                // Changes were lost: closing makes the client reconnect and read everything again.
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("websocket client lagged, {n} changes dropped: closing it to resync");
                    break;
                }
                Err(RecvError::Closed) => break,
            },
        };
        if socket.send(Message::Text(reply.to_string().into())).await.is_err() {
            break;
        }
    }
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct SetDesired {
    id: Uuid,
    cap: CapKind,
    value: CapValue,
}

#[derive(Deserialize)]
struct Invoke {
    id: Uuid,
    action: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct AutomationId {
    id: String,
}

#[derive(Deserialize)]
struct SetLocation {
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct SaveAutomation {
    automation: AutomationConfig,
}

#[derive(Deserialize)]
struct Install {
    id: String,
    version: String,
    permissions: Vec<String>,
    #[serde(default)]
    settings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Configure {
    id: String,
    settings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct AddDevice {
    plugin: String,
    #[serde(default)]
    settings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct DeviceId {
    plugin: String,
    id: String,
}

#[derive(Deserialize)]
struct PluginId {
    id: String,
}

#[derive(Deserialize)]
struct DryRunParams {
    id: Option<String>,
    automation: Option<AutomationConfig>,
}

async fn handle(app: &AppState, req: Request) -> Value {
    let result = match req.method.as_str() {
        "entities.list" => Ok(app.core.with(|r| json!(r.entities.values().collect::<Vec<_>>()))),
        "entity.set_desired" => serde_json::from_value::<SetDesired>(req.params)
            .map_err(|e| (-32602, e.to_string()))
            .and_then(|p| {
                app.core.with(|r| r.set_desired(p.id, &p.cap, p.value, Instant::now()))
                    .map(|status| json!(status))
                    .map_err(|e| (-32000, e))
            }),
        "entity.update" => match serde_json::from_value::<EntityOverride>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(update) => blocking(app, move |app| update_entity(app, update)).await.map_err(|e| (-32000, e)),
        },
        "rooms.list" => {
            let rooms = app.core.rooms.lock().unwrap();
            Ok(json!(rooms.iter().map(|(name, kind)| RoomConfig { name: name.clone(), kind: Some(kind.clone()) }).collect::<Vec<_>>()))
        }
        "room.update" => match serde_json::from_value::<RoomConfig>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(room) => blocking(app, move |app| update_room(app, room)).await.map_err(|e| (-32000, e)),
        },
        "entity.invoke" => match serde_json::from_value::<Invoke>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => match app.core.plugins.get() {
                Some(plugins) => plugins.invoke(&app.core, p.id, &p.action, p.params).await.map(|()| json!(true)).map_err(|e| (-32000, e)),
                None => Err((-32000, "plugins are not running".into())),
            },
        },
        "automations.list" => Ok(app.core.automations().list(&app.core, &jiff::Zoned::now())),
        "automation.dry_run" => match serde_json::from_value::<DryRunParams>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(DryRunParams { id: Some(id), automation: None }) => {
                app.core.automations().dry_run(&app.core, &id, &jiff::Zoned::now()).map(|d| json!(d)).map_err(|e| (-32000, e))
            }
            // Not saved yet: compiled on its own, as it would be.
            Ok(DryRunParams { id: None, automation: Some(a) }) => Automations::new(std::slice::from_ref(&a), app.core.automations().location)
                .and_then(|one| one.dry_run(&app.core, &a.id, &jiff::Zoned::now()))
                .map(|d| json!(d))
                .map_err(|e| (-32000, e)),
            Ok(_) => Err((-32602, "give either `id` or `automation`".into())),
        },
        "automation.run" => match serde_json::from_value::<AutomationId>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => {
                let automations = app.core.automations();
                automations.run(&app.core, &p.id, "manual".into(), &jiff::Zoned::now()).await.map(|r| json!(r)).map_err(|e| (-32000, e))
            }
        },
        "automation.save" => match serde_json::from_value::<SaveAutomation>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => {
                let id = p.automation.id.clone();
                blocking(app, move |app| save_automation(app, &id, Some(p.automation))).await.map_err(|e| (-32000, e))
            }
        },
        "home.location" => Ok(json!(app.core.automations().location.map(|l| json!({ "latitude": l.latitude, "longitude": l.longitude })))),
        "home.set_location" => match serde_json::from_value::<SetLocation>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => blocking(app, move |app| set_location(app, p)).await.map_err(|e| (-32000, e)),
        },
        "automation.delete" => match serde_json::from_value::<AutomationId>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => blocking(app, move |app| save_automation(app, &p.id, None)).await.map_err(|e| (-32000, e)),
        },
        "plugins.list" => blocking(app, list_plugins).await.map_err(|e| (-32000, e)),
        "registry.list" => blocking(app, list_registry).await.map_err(|e| (-32000, e)),
        "plugin.install" => match serde_json::from_value::<Install>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => blocking(app, move |app| install_plugin(app, p)).await.map_err(|e| (-32000, e)),
        },
        "plugin.configure" => match serde_json::from_value::<Configure>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => blocking(app, move |app| configure_plugin(app, p)).await.map_err(|e| (-32000, e)),
        },
        "plugin.remove" => match serde_json::from_value::<PluginId>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => blocking(app, move |app| remove_plugin(app, &p.id)).await.map_err(|e| (-32000, e)),
        },
        "device.add" => match serde_json::from_value::<AddDevice>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => add_device(app, p).await.map_err(|e| (-32000, e)),
        },
        "device.remove" => match serde_json::from_value::<DeviceId>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => blocking(app, move |app| remove_device(app, p)).await.map_err(|e| (-32000, e)),
        },
        _ => Err((-32601, format!("unknown method `{}`", req.method))),
    };
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": req.id, "result": result }),
        Err((code, message)) => error(req.id, code, message),
    }
}

/// Runs file work (the config file) off the async threads.
async fn blocking(app: &AppState, f: impl FnOnce(&AppState) -> Result<Value, String> + Send + 'static) -> Result<Value, String> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&app)).await.map_err(|e| e.to_string())?
}

/// Saves where the home is, then compiles the automations again with it: sunrise and sunset work
/// at once.
fn set_location(app: &AppState, p: SetLocation) -> Result<Value, String> {
    if sunrise::Coordinates::new(p.latitude, p.longitude).is_none() {
        return Err("latitude is -90 to 90, longitude -180 to 180".into());
    }
    config::set_location(&app.config_path, p.latitude, p.longitude)?;
    let all = config::load(&app.config_path)?.automations;
    let location = Location { latitude: p.latitude, longitude: p.longitude };
    app.core.set_automations(Automations::new(&all, Some(location))?);
    Ok(json!(true))
}

/// Writes the automation to the config file (`None` removes it) once every automation of the
/// file compiles, and puts them in force at once.
fn save_automation(app: &AppState, id: &str, automation: Option<AutomationConfig>) -> Result<Value, String> {
    let location = app.core.automations().location;
    let next = config::save_automation(&app.config_path, id, automation.as_ref(), |all| Automations::new(all, location))?;
    app.core.set_automations(next);
    Ok(json!(true))
}

/// Merges the given fields into the entity's metadata, saves it to the config file, then applies it.
fn update_entity(app: &AppState, update: EntityOverride) -> Result<Value, String> {
    let mut overrides = app.core.overrides.lock().unwrap();
    if !app.core.with(|r| r.entities.contains_key(&update.id)) {
        return Err("unknown entity".into());
    }
    let merged = match overrides.get(&update.id) {
        Some(old) => EntityOverride {
            id: update.id,
            name: update.name.or_else(|| old.name.clone()),
            room: update.room.or_else(|| old.room.clone()),
            tags: update.tags.or_else(|| old.tags.clone()),
            model: update.model.or_else(|| old.model.clone()),
        },
        None => update,
    };
    // File first: nothing changes if it cannot be saved.
    config::save_override(&app.config_path, &merged)?;
    app.core.with(|r| r.edit(merged.id, |e| merged.apply(e)))?;
    overrides.insert(merged.id, merged);
    reschedule(app);
    Ok(json!(true))
}

fn update_room(app: &AppState, mut room: RoomConfig) -> Result<Value, String> {
    if room.name.is_empty() {
        return Err("a room needs a name".into());
    }
    room.kind = room.kind.filter(|k| !k.is_empty());
    let mut rooms = app.core.rooms.lock().unwrap();
    config::save_room(&app.config_path, &room)?;
    match &room.kind {
        Some(kind) => rooms.insert(room.name.clone(), kind.clone()),
        None => rooms.remove(&room.name),
    };
    let _ = app.core.changes.send(Event::Room(room));
    reschedule(app);
    Ok(json!(true))
}

/// Where an installed plugin's files are: `<id>.wasm`, and `<id>.json` (its registry entry, for
/// its name and settings without the Internet).
fn plugin_file(app: &AppState, id: &str, ext: &str) -> std::path::PathBuf {
    app.data_dir.join("plugins").join(format!("{id}.{ext}"))
}

fn installed_entry(app: &AppState, id: &str) -> Option<registry::Entry> {
    serde_json::from_slice(&std::fs::read(plugin_file(app, id, "json")).ok()?).ok()
}

fn secrets_path(app: &AppState) -> std::path::PathBuf {
    app.data_dir.join("secrets.toml")
}

/// The plugins of the config file, with how they are doing. Secret values are never sent back:
/// only which ones are set.
fn list_plugins(app: &AppState) -> Result<Value, String> {
    let states = app.core.plugins.get().map(|p| p.states()).unwrap_or_default();
    let secrets = config::load_secrets(&secrets_path(app))?;
    let list: Vec<Value> = config::load(&app.config_path)?
        .plugins
        .into_iter()
        .map(|cfg| {
            let entry = cfg.key.is_some().then(|| installed_entry(app, &cfg.id)).flatten();
            let state = states.get(&cfg.id).cloned().unwrap_or_default();
            let set: Vec<&String> = secrets.get(&cfg.id).map(|s| s.keys().collect()).unwrap_or_default();
            let devices: Vec<Value> = cfg
                .devices
                .iter()
                .map(|d| {
                    let set: Vec<&String> = secrets.get(&config::device_secrets(&cfg.id, &d.id)).map(|s| s.keys().collect()).unwrap_or_default();
                    json!({ "id": d.id, "settings": d.settings, "secrets": set, "error": state.devices.get(&d.id) })
                })
                .collect();
            json!({
                "id": cfg.id,
                "name": entry.as_ref().map_or(&cfg.id, |e| &e.name),
                "description": entry.as_ref().map(|e| &e.description),
                "source": if cfg.key.is_some() { "registry" } else { "local" },
                "version": cfg.version.or(state.version),
                "error": state.error,
                "permissions": cfg.permissions,
                "settings": cfg.settings,
                "secrets": set,
                "declared": entry.as_ref().map(|e| &e.settings),
                "device_settings": entry.as_ref().map(|e| &e.device_settings),
                "devices": devices,
            })
        })
        .collect();
    Ok(json!(list))
}

/// What the registry offers this core: each plugin with the newest version it can run (none: it
/// needs a newer HomeMaid), and what is installed.
fn list_registry(app: &AppState) -> Result<Value, String> {
    let index = registry::fetch(&app.registry)?;
    let installed: HashMap<String, PluginConfig> = config::load(&app.config_path)?.plugins.into_iter().map(|c| (c.id.clone(), c)).collect();
    let list: Vec<Value> = index
        .plugins
        .iter()
        .map(|e| {
            let version = e.compatible();
            let mine = installed.get(&e.id);
            json!({
                "id": e.id,
                "name": e.name,
                "description": e.description,
                "repo": e.repo,
                "settings": e.settings,
                "device_settings": e.device_settings,
                "version": version.map(|v| &v.version),
                "permissions": version.map(|v| &v.permissions),
                "installed": mine.map(|c| c.version.as_deref().unwrap_or("local")),
                "update": matches!((mine.and_then(|c| c.version.as_deref()), version), (Some(have), Some(v)) if registry::compare(&v.version, have).is_gt()),
            })
        })
        .collect();
    Ok(json!(list))
}

/// Downloads, checks and starts a plugin of the registry, or updates it. Nothing changes unless
/// the file is the one its author signed, at the version and with the permissions the user saw.
fn install_plugin(app: &AppState, p: Install) -> Result<Value, String> {
    let index = registry::fetch(&app.registry)?;
    let mut entry = index.plugins.into_iter().find(|e| e.id == p.id).ok_or(format!("no plugin `{}` in the registry", p.id))?;
    let version = entry.compatible().cloned().ok_or(format!("{} needs a newer HomeMaid", entry.name))?;
    if version.version != p.version {
        return Err(format!("{} is now at version {}: look at it again", entry.name, version.version));
    }
    if let Some(more) = version.permissions.iter().find(|x| !p.permissions.contains(x)) {
        return Err(format!("{} asks for `{more}`, which was not accepted", entry.name));
    }
    let current = config::load(&app.config_path)?.plugins.into_iter().find(|c| c.id == p.id);
    if current.as_ref().is_some_and(|c| c.key.is_none()) {
        return Err(format!("`{}` is already in the config, not from the registry: remove its [[plugin]] first", p.id));
    }
    let old_settings = current.as_ref().map(|c| c.settings.clone()).unwrap_or_default();
    let (settings, secrets) = split_settings(app, &entry.settings, &p.id, p.settings, &old_settings)?;
    let wasm = registry::download(&entry, &version, current.as_ref().and_then(|c| c.key.as_deref()))?;

    // The running instance compiled its own copy: the file can be replaced under it.
    let file = plugin_file(app, &p.id, "wasm");
    std::fs::create_dir_all(file.parent().expect("in the plugins dir")).map_err(|e| e.to_string())?;
    store::write_atomic(&file, &wasm).map_err(|e| format!("{}: {e}", file.display()))?;
    let key = std::mem::take(&mut entry.key);
    entry.versions.clear();
    store::write_atomic(&plugin_file(app, &p.id, "json"), &serde_json::to_vec(&entry).expect("serializable"))
        .map_err(|e| e.to_string())?;
    let cfg = PluginConfig {
        id: p.id,
        path: file,
        permissions: version.permissions,
        settings,
        version: Some(version.version),
        key: Some(key),
        devices: current.map(|c| c.devices).unwrap_or_default(),
    };
    save_and_start(app, cfg, secrets)
}

/// New settings for an installed plugin; empty secrets are kept as they were.
fn configure_plugin(app: &AppState, p: Configure) -> Result<Value, String> {
    let cfg = config::load(&app.config_path)?.plugins.into_iter().find(|c| c.id == p.id).ok_or(format!("no plugin `{}`", p.id))?;
    let entry = installed_entry(app, &p.id).filter(|_| cfg.key.is_some()).ok_or(format!("`{}` is set in homemaid.toml, not from the app", p.id))?;
    let (settings, secrets) = split_settings(app, &entry.settings, &p.id, p.settings, &cfg.settings)?;
    save_and_start(app, PluginConfig { settings, ..cfg }, secrets)
}

type Settings = BTreeMap<String, String>;

/// Sorts what the user typed by the declared settings: secrets (to `secrets.toml`) and the rest
/// (to the config). A setting left out (or an empty secret) keeps its value, from `old` or the
/// `section` of `secrets.toml`; a required one with none is refused.
fn split_settings(
    app: &AppState,
    declared: &[registry::Setting],
    section: &str,
    mut given: BTreeMap<String, String>,
    old: &BTreeMap<String, String>,
) -> Result<(Settings, Settings), String> {
    if let Some(unknown) = given.keys().find(|k| !declared.iter().any(|d| &d.key == *k)) {
        return Err(format!("no setting `{unknown}`"));
    }
    let old_secrets = config::load_secrets(&secrets_path(app))?.remove(section).unwrap_or_default();
    let (mut settings, mut secrets) = (BTreeMap::new(), BTreeMap::new());
    for d in declared {
        let value = given.remove(&d.key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let kept = if d.secret { old_secrets.get(&d.key) } else { old.get(&d.key) };
        if d.required && value.is_none() && kept.is_none() {
            return Err(format!("{} is required", d.label));
        }
        match (value, d.secret) {
            (Some(v), true) => drop(secrets.insert(d.key.clone(), v)),
            (Some(v), false) => drop(settings.insert(d.key.clone(), v)),
            (None, false) => settings.extend(kept.map(|v| (d.key.clone(), v.clone()))),
            (None, true) => secrets.extend(kept.map(|v| (d.key.clone(), v.clone()))),
        }
    }
    Ok((settings, secrets))
}

/// Writes the secrets and the `[[plugin]]`, then (re)starts the plugin with them.
fn save_and_start(app: &AppState, cfg: PluginConfig, secrets: BTreeMap<String, String>) -> Result<Value, String> {
    for (key, value) in &secrets {
        config::set_secret(&secrets_path(app), &cfg.id, key, value)?;
    }
    let dir = app.config_path.parent().unwrap_or(Path::new("."));
    let file = cfg.path.strip_prefix(dir).unwrap_or(&cfg.path).display().to_string();
    config::save_plugin(&app.config_path, &cfg, &file)?;
    restart(app, cfg)
}

/// (Re)starts a plugin with its secrets and its devices' ones, as saved.
fn restart(app: &AppState, mut cfg: PluginConfig) -> Result<Value, String> {
    let plugins = app.core.plugins.get().ok_or("plugins are not running")?;
    let secrets = crate::plugins::take_secrets(&mut cfg, &mut config::load_secrets(&secrets_path(app))?);
    plugins.spawn(cfg, secrets)?;
    let _ = app.core.changes.send(Event::Plugins);
    Ok(json!(true))
}

/// Adds a device to a plugin from its form (plugin-api 1.3). The plugin checks it first (a wrong
/// address or password is said in the form) and only one it accepts is saved. An empty secret
/// takes the plugin's last added device's: a second strip on the same account needs no login.
async fn add_device(app: &AppState, p: AddDevice) -> Result<Value, String> {
    let plugins = app.core.plugins.get().ok_or("plugins are not running")?.clone();
    let (prepare, plugin) = (app.clone(), p.plugin.clone());
    let (device, secrets) = tokio::task::spawn_blocking(move || {
        let cfg = config::load(&prepare.config_path)?.plugins.into_iter().find(|c| c.id == p.plugin).ok_or(format!("no plugin `{}`", p.plugin))?;
        let entry = installed_entry(&prepare, &p.plugin).filter(|_| cfg.key.is_some());
        let declared = entry.map(|e| e.device_settings).filter(|d| !d.is_empty()).ok_or(format!("`{}` has no devices to add", p.plugin))?;
        let last = cfg.devices.last().map(|d| config::device_secrets(&p.plugin, &d.id)).unwrap_or_default();
        let (settings, secrets) = split_settings(&prepare, &declared, &last, p.settings, &BTreeMap::new())?;
        let taken = |id: &str| cfg.devices.iter().any(|d| d.id == id);
        let id = std::iter::repeat_with(|| Uuid::new_v4().simple().to_string()[..8].to_string()).find(|id| !taken(id)).expect("endless");
        Ok::<_, String>((DeviceConfig { id, settings }, secrets))
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut given = device.settings.clone();
    given.extend(secrets.clone());
    plugins.add_device(&plugin, &device.id, given).await?;
    blocking(app, move |app| {
        for (key, value) in &secrets {
            config::set_secret(&secrets_path(app), &config::device_secrets(&plugin, &device.id), key, value)?;
        }
        config::add_device(&app.config_path, &plugin, &device)?;
        let _ = app.core.changes.send(Event::Plugins);
        Ok(json!({ "id": device.id }))
    })
    .await
}

/// Removes a device added from the app, its entities and its secrets; the plugin restarts without it.
fn remove_device(app: &AppState, p: DeviceId) -> Result<Value, String> {
    let cfg = config::load(&app.config_path)?.plugins.into_iter().find(|c| c.id == p.plugin).ok_or(format!("no plugin `{}`", p.plugin))?;
    config::delete_device(&app.config_path, &p.plugin, &p.id)?;
    config::delete_secrets(&secrets_path(app), &config::device_secrets(&p.plugin, &p.id))?;
    // Stopped first: an instance still starting with the old devices would bring its entities back.
    if let Some(plugins) = app.core.plugins.get() {
        plugins.stop(&p.plugin);
    }
    app.core.with(|r| r.entities.retain(|_, e| !(e.plugin == p.plugin && e.added.as_deref() == Some(p.id.as_str()))));
    app.core.dirty.notify_one();
    restart(app, PluginConfig { devices: cfg.devices.into_iter().filter(|d| d.id != p.id).collect(), ..cfg })
}

/// Uninstalls a plugin of the registry: its `[[plugin]]`, files, KV store and entities. Its
/// secrets stay in `secrets.toml` (the user's), ready for a reinstall.
fn remove_plugin(app: &AppState, id: &str) -> Result<Value, String> {
    let cfg = config::load(&app.config_path)?.plugins.into_iter().find(|c| c.id == id).ok_or(format!("no plugin `{id}`"))?;
    if cfg.key.is_none() {
        return Err(format!("`{id}` is set in homemaid.toml, not from the app"));
    }
    config::delete_plugin(&app.config_path, id)?;
    if let Some(plugins) = app.core.plugins.get() {
        plugins.remove(id);
    }
    for ext in ["wasm", "json"] {
        let _ = std::fs::remove_file(plugin_file(app, id, ext));
    }
    let _ = app.core.changes.send(Event::Plugins);
    Ok(json!(true))
}

/// A device moved or renamed may now match other automations: their schedules follow.
fn reschedule(app: &AppState) {
    if let Some(plugins) = app.core.plugins.get() {
        plugins.reschedule();
    }
}

fn error(id: Value, code: i32, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

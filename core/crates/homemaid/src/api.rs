//! Public API: JSON-RPC 2.0 over WebSocket on `/ws`.
//!
//! Methods: `entities.list`, `entity.set_desired { id, cap, value }`,
//! `entity.update { id, name?, room?, tags?, model? }` (saved to the config file), `rooms.list`,
//! `room.update { name, kind }` (saved too; empty kind clears it), `entity.invoke { id, action,
//! params? }` (a one-shot action), `automations.list`, `automation.dry_run { id }`,
//! `automation.run { id }`, `subscribe` (then `state.changed { id, cap, slot }`,
//! `entity.changed { entity }`, `room.changed { name, kind }` and `automation.ran { id, run }`
//! notifications).
//! `/pair` serves the pairing QR code to the PC itself; `/hello?nonce=` proves this host knows
//! the token (see [`pairing::proof`]).

use std::{collections::HashMap, net::SocketAddr, path::Path, sync::Arc, time::Instant};

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
    config::{self, EntityOverride, RoomConfig},
    model::{CapKind, CapValue},
    pairing,
};

#[derive(Clone)]
struct AppState {
    core: Arc<Core>,
    token: Arc<str>,
    config_path: Arc<Path>,
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
        .with_state(AppState { core, token: config.token.as_str().into(), config_path: config_path.into(), pair_page: pair_page.into() })
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

/// Methods that wait for a plugin (an action: ms usually, 70 s at worst). They run aside so the
/// notifications keep flowing; the others answer in order with them, so `entities.list` is never
/// overtaken by an older change.
const SLOW: [&str; 2] = ["entity.invoke", "automation.run"];

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
        "entity.update" => serde_json::from_value::<EntityOverride>(req.params)
            .map_err(|e| (-32602, e.to_string()))
            .and_then(|update| update_entity(app, update).map_err(|e| (-32000, e))),
        "rooms.list" => {
            let rooms = app.core.rooms.lock().unwrap();
            Ok(json!(rooms.iter().map(|(name, kind)| RoomConfig { name: name.clone(), kind: Some(kind.clone()) }).collect::<Vec<_>>()))
        }
        "room.update" => serde_json::from_value::<RoomConfig>(req.params)
            .map_err(|e| (-32602, e.to_string()))
            .and_then(|room| update_room(app, room).map_err(|e| (-32000, e))),
        "entity.invoke" => match serde_json::from_value::<Invoke>(req.params) {
            Err(e) => Err((-32602, e.to_string())),
            Ok(p) => match app.core.plugins.get() {
                Some(plugins) => plugins.invoke(&app.core, p.id, &p.action, p.params).await.map(|()| json!(true)).map_err(|e| (-32000, e)),
                None => Err((-32000, "plugins are not running".into())),
            },
        },
        "automations.list" => match app.core.automations.get() {
            Some(automations) => Ok(automations.list(&app.core, &jiff::Zoned::now())),
            None => Ok(json!([])),
        },
        "automation.dry_run" => match (serde_json::from_value::<AutomationId>(req.params), app.core.automations.get()) {
            (Err(e), _) => Err((-32602, e.to_string())),
            (Ok(_), None) => Err((-32000, "no automations".into())),
            (Ok(p), Some(a)) => a.dry_run(&app.core, &p.id, &jiff::Zoned::now()).map(|d| json!(d)).map_err(|e| (-32000, e)),
        },
        "automation.run" => match (serde_json::from_value::<AutomationId>(req.params), app.core.automations.get()) {
            (Err(e), _) => Err((-32602, e.to_string())),
            (Ok(_), None) => Err((-32000, "no automations".into())),
            (Ok(p), Some(a)) => a.run(&app.core, &p.id, "manual".into(), &jiff::Zoned::now()).await.map(|r| json!(r)).map_err(|e| (-32000, e)),
        },
        _ => Err((-32601, format!("unknown method `{}`", req.method))),
    };
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": req.id, "result": result }),
        Err((code, message)) => error(req.id, code, message),
    }
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
    Ok(json!(true))
}

fn error(id: Value, code: i32, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

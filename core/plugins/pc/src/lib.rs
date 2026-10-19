//! A PC running the HomeMaid agent, as an entity: volume, mute and sensors (CPU, RAM, active window).
//!
//! Settings: `agent` = `ip:port` of the agent (`127.0.0.1:8124` on the same PC, which needs the
//! `loopback` permission besides `local-network`). Secret: `token`
//! (the agent's `agent.token`). One-shot actions (plugin-api 1.1): media keys, lock, sleep,
//! shutdown, opening a configured shortcut or app; they are not states, so they are actions,
//! for automations and the app. The remote itself talks to the agent directly, for latency.

use std::{collections::BTreeMap, net::SocketAddr, sync::Mutex};

use homemaid_plugin::{
    ActionDecl, Actions, CapKind, CapMeta, CapValue, DiscoveredEntity, LOCAL_NETWORK, LogLevel, Manifest, Plugin, http,
    log, report_state, secret, set_available,
};
use serde_json::{Value, json};

const LOCAL_ID: &str = "pc";
const REFRESH_MS: u32 = 2_000;

struct State {
    agent: SocketAddr,
    token: String,
    online: bool,
    /// What `launch` and `open` accept, as the agent said at discovery.
    apps: Vec<String>,
    shortcuts: Vec<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> Result<R, String>) -> Result<R, String> {
    f(STATE.lock().unwrap().as_mut().ok_or("plugin not initialized")?)
}

fn call(state: &State, request: Value) -> Result<Value, String> {
    let auth = format!("Bearer {}", state.token);
    let headers = [("Authorization", auth.as_str()), ("Content-Type", "application/json")];
    let response = http::post(state.agent, "/api", request.to_string().as_bytes(), &headers).map_err(|e| {
        if state.agent.ip().is_loopback() && e.contains("Permission denied") {
            format!("{e}: the agent is on this PC, grant `loopback` to the pc plugin (its permissions in homemaid.toml)")
        } else {
            e
        }
    })?;
    match response.status {
        200 => serde_json::from_slice(&response.body).map_err(|e| format!("agent: invalid JSON: {e}")),
        401 => Err("agent refused the token: `token` in [pc] of secrets.toml must be the agent's agent.token".into()),
        // The agent says why in words (`{"error": "app `x` is not in [agent.apps]"}`): just that.
        status => Err(serde_json::from_slice::<Value>(&response.body)
            .ok()
            .and_then(|body| body["error"].as_str().map(String::from))
            .unwrap_or_else(|| format!("agent: HTTP {status}: {}", String::from_utf8_lossy(&response.body)))),
    }
}

fn sensor(unit: Option<&str>, schema: Value) -> CapMeta {
    CapMeta { unit: unit.map(Into::into), read_only: true, schema: Some(schema), ..Default::default() }
}

fn custom(name: &str) -> CapKind {
    CapKind::Custom(name.into())
}

fn report(pc: &Value) {
    if let Some(level) = pc["volume"].as_u64() {
        report_state(LOCAL_ID, &CapKind::Volume, &CapValue::Volume(level.min(100) as u8));
    }
    if let Some(muted) = pc["muted"].as_bool() {
        report_state(LOCAL_ID, &CapKind::Mute, &CapValue::Mute(muted));
    }
    // Rounded: sensors change constantly, no need to wake subscribers for noise.
    let percent = |v: &Value| json!(v.as_f64().map_or(0.0, f64::round));
    report_state(LOCAL_ID, &custom("cpu"), &CapValue::Custom(percent(&pc["cpu"])));
    report_state(LOCAL_ID, &custom("memory"), &CapValue::Custom(percent(&pc["memory"])));
    report_state(LOCAL_ID, &custom("active_window"), &CapValue::Custom(pc["active_window"].clone()));
}

struct Pc;

impl Plugin for Pc {
    fn manifest() -> Manifest {
        Manifest { id: "pc".into(), version: env!("CARGO_PKG_VERSION").into(), permissions: vec![LOCAL_NETWORK.into()] }
    }

    fn init(settings: BTreeMap<String, String>) -> Result<(), String> {
        let agent = settings.get("agent").ok_or("missing setting `agent` (ip:port of the agent, e.g. 127.0.0.1:8124)")?;
        let agent = agent.parse().map_err(|_| format!("invalid setting `agent` = `{agent}`: expected ip:port"))?;
        let token = secret("token").ok_or("missing secret `token` in [pc] of secrets.toml (the agent's agent.token)")?;
        *STATE.lock().unwrap() = Some(State { agent, token, online: false, apps: vec![], shortcuts: vec![] });
        Ok(())
    }

    fn discover() -> Result<Vec<DiscoveredEntity>, String> {
        with_state(|state| {
            let pc = call(state, json!({ "type": "state" }))?;
            let names = |key: &str| pc[key].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(String::from)).collect();
            state.apps = names("apps");
            state.shortcuts = names("shortcuts");
            let mut caps = vec![
                (custom("cpu"), sensor(Some("%"), json!({ "type": "number" }))),
                (custom("memory"), sensor(Some("%"), json!({ "type": "number" }))),
                (custom("active_window"), sensor(None, json!({ "type": ["string", "null"] }))),
            ];
            // No audio output on the PC: no volume to control.
            if !pc["volume"].is_null() {
                caps.push((CapKind::Volume, CapMeta { min: Some(0.0), max: Some(100.0), unit: Some("%".into()), ..Default::default() }));
                caps.push((CapKind::Mute, CapMeta::default()));
            }
            let name = pc["hostname"].as_str().unwrap_or("PC").to_string();
            Ok(vec![DiscoveredEntity { local_id: LOCAL_ID.into(), name, device: None, caps }])
        })
    }

    fn apply(local_id: &str, kind: CapKind, value: CapValue) -> Result<(), String> {
        if local_id != LOCAL_ID {
            return Err(format!("unknown entity {local_id}"));
        }
        let request = match (&kind, value) {
            (CapKind::Volume, CapValue::Volume(level)) => json!({ "type": "set_volume", "level": level }),
            (CapKind::Mute, CapValue::Mute(muted)) => json!({ "type": "set_mute", "muted": muted }),
            _ => return Err(format!("unsupported capability {kind}")),
        };
        // Confirmed by the refresh the host calls next.
        with_state(|state| call(state, request).map(drop))
    }

    fn refresh() -> u32 {
        let _ = with_state(|state| {
            match call(state, json!({ "type": "state" })) {
                Ok(pc) => {
                    if !state.online {
                        log(LogLevel::Info, &format!("agent {} online", state.agent));
                    }
                    state.online = true;
                    set_available(LOCAL_ID, true);
                    report(&pc);
                }
                Err(e) => {
                    if state.online {
                        log(LogLevel::Warn, &format!("agent {} offline: {e}", state.agent));
                    }
                    state.online = false;
                    set_available(LOCAL_ID, false);
                }
            }
            Ok(())
        });
        REFRESH_MS
    }
}

/// Actions without parameters, and the agent request each one sends.
const SIMPLE: [(&str, &str, &str); 6] = [
    ("play_pause", "media", "play_pause"),
    ("next", "media", "next"),
    ("previous", "media", "previous"),
    ("lock", "power", "lock"),
    ("sleep", "power", "sleep"),
    ("shutdown", "power", "shutdown"),
];

impl Actions for Pc {
    fn actions() -> Vec<ActionDecl> {
        // The choices the agent has (an older one tells none: any name).
        let (apps, shortcuts) = with_state(|s| Ok((s.apps.clone(), s.shortcuts.clone()))).unwrap_or_default();
        let named = |key: &str, choices: Vec<String>| {
            let mut field = json!({ "type": "string" });
            if !choices.is_empty() {
                field["enum"] = json!(choices);
            }
            Some(json!({ "type": "object", "properties": { key: field }, "required": [key] }))
        };
        let simple = SIMPLE.iter().map(|(name, ..)| ActionDecl { local_id: LOCAL_ID.into(), name: (*name).into(), params: None });
        simple
            .chain([
                ActionDecl { local_id: LOCAL_ID.into(), name: "open".into(), params: named("shortcut", shortcuts) },
                ActionDecl { local_id: LOCAL_ID.into(), name: "launch".into(), params: named("app", apps) },
            ])
            .collect()
    }

    fn invoke(local_id: &str, name: &str, params: Value) -> Result<(), String> {
        if local_id != LOCAL_ID {
            return Err(format!("unknown entity {local_id}"));
        }
        let text = |key: &str| params[key].as_str().map(str::to_string).ok_or(format!("`{name}` needs `{key}`"));
        let request = match (name, SIMPLE.iter().find(|(n, ..)| *n == name)) {
            (_, Some((_, kind, action))) => json!({ "type": kind, "action": action }),
            ("open", None) => json!({ "type": "open", "shortcut": text("shortcut")? }),
            ("launch", None) => json!({ "type": "launch", "app": text("app")? }),
            _ => return Err(format!("unknown action `{name}`")),
        };
        with_state(|state| call(state, request).map(drop))
    }
}

homemaid_plugin::export_with_actions!(Pc with_types_in homemaid_plugin::bindings_actions);

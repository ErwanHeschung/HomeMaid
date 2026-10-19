//! TP-Link Tapo P300 power strip, local control over KLAP v2: one `on_off` entity per outlet.
//!
//! Settings: `hosts` = comma-separated IPs (`ip` or `ip:port`). Secrets: `username`, `password`
//! (TP-Link account). Requires "Third-Party Compatibility" in the Tapo app (Me > Third-Party
//! Services), otherwise recent firmwares speak TPAP, which is not supported.
//!
//! Schedules (plugin-api 1.2): the outlets' `on_off` schedules handed over by the core become
//! schedule rules on the strip, which then switches with the hub off. It computes sunrise and
//! sunset from the location and time zone set in the Tapo app. Only the rules this plugin added
//! are ever replaced (ids in the KV store, `schedules:<outlet>`); those made in the app stay.
// ponytail: the rule format comes from the tapo crate, measured on a P110, and per-outlet rules
// through control_child are unverified on a real P300 (examples/probe.rs checks it).

pub mod klap;

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use homemaid_plugin::{
    ActionDecl, Actions, CapKind, http, CapMeta, CapValue, DiscoveredEntity, LOCAL_NETWORK, LogLevel, Manifest, Plugin,
    Schedule, Schedules, TimeOfDay, kv_get, kv_set, log, report_state, secret, set_available,
};
use serde_json::{Value, json};

const AUTH_HINT: &str = "credentials refused by the device: check `username`/`password` of [tplink] in secrets.toml \
    (TP-Link account), and enable Third-Party Compatibility in the Tapo app (Me > Third-Party Services); \
    the newer TPAP protocol is not supported";
const BINARY: (&str, &str) = ("Content-Type", "application/octet-stream");
/// Sessions are renewed this long before the device expires them, like python-kasa.
const SESSION_MARGIN: Duration = Duration::from_secs(20 * 60);
const DEFAULT_SESSION: u64 = 24 * 3600;
const FAST_REFRESH_MS: u32 = 2_000;
const SLOW_REFRESH_MS: u32 = 10_000;
/// Refresh fast for a while after a command, to catch follow-up changes.
const FAST_WINDOW: Duration = Duration::from_secs(10);
/// An offline strip is asked again this often, not at every refresh: each try can block the
/// plugin for seconds (connect and read timeouts), and commands to the other strips wait meanwhile.
// ponytail: still one blocking try per period; concurrent requests need WASIp3 async (plugin-api v2).
const OFFLINE_RETRY: Duration = Duration::from_secs(30);

struct Live {
    klap: klap::Session,
    cookie: String,
    expires: Instant,
}

struct Host {
    addr: SocketAddr,
    session: Option<Live>,
    /// `device_id` of each outlet, as discovered.
    outlets: Vec<String>,
    online: bool,
    /// When an offline strip is tried again.
    retry_at: Instant,
}

struct Outlet {
    id: String,
    name: String,
    on: Option<bool>,
}

struct State {
    auth: [u8; 32],
    terminal_uuid: String,
    hosts: Vec<Host>,
    fast_until: Instant,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> Result<R, String>) -> Result<R, String> {
    f(STATE.lock().unwrap().as_mut().ok_or("plugin not initialized")?)
}

impl Host {
    /// Sends a SMART request and returns its `result`.
    fn call(&mut self, auth: &[u8; 32], terminal_uuid: &str, method: &str, params: Option<Value>) -> Result<Value, String> {
        // A stale session is the only failure a new handshake can fix.
        let had_session = self.session.is_some();
        match self.try_call(auth, terminal_uuid, method, params.clone()) {
            Err(_) if had_session => {
                self.session = None;
                self.try_call(auth, terminal_uuid, method, params)
            }
            result => result,
        }
    }

    fn try_call(&mut self, auth: &[u8; 32], terminal_uuid: &str, method: &str, params: Option<Value>) -> Result<Value, String> {
        if !self.session.as_ref().is_some_and(|live| live.expires > Instant::now()) {
            self.session = Some(handshake(self.addr, auth)?);
        }
        let live = self.session.as_mut().expect("session just established");

        let mut request = json!({ "method": method, "request_time_milis": now_ms(), "terminal_uuid": terminal_uuid });
        if let Some(params) = params {
            request["params"] = params;
        }
        let (body, seq) = live.klap.encrypt(request.to_string().as_bytes());
        let response = http::post(self.addr, &format!("/app/request?seq={seq}"), &body, &[BINARY, ("Cookie", &live.cookie)])?;
        if response.status != 200 {
            return Err(format!("{}: {method}: HTTP {}", self.addr, response.status));
        }
        let mut reply: Value = serde_json::from_slice(&live.klap.decrypt(seq, &response.body)?)
            .map_err(|e| format!("{}: {method}: invalid JSON: {e}", self.addr))?;
        check(&reply, method)?;
        Ok(reply["result"].take())
    }

    fn outlets(&mut self, auth: &[u8; 32], terminal_uuid: &str) -> Result<Vec<Outlet>, String> {
        // ponytail: first page only (10 children), a P300 has 3.
        let result = self.call(auth, terminal_uuid, "get_child_device_list", None)?;
        Ok(parse_outlets(&result))
    }

    /// A request to one outlet, through the strip.
    fn child(&mut self, auth: &[u8; 32], terminal_uuid: &str, outlet: &str, method: &str, params: Option<Value>) -> Result<Value, String> {
        let mut request = json!({ "method": method });
        if let Some(params) = params {
            request["params"] = params;
        }
        let mut result = self.call(auth, terminal_uuid, "control_child", Some(json!({ "device_id": outlet, "requestData": request })))?;
        let mut response = result["responseData"].take();
        check(&response, method)?;
        Ok(response["result"].take())
    }

    /// The ids of an outlet's schedule rules, every page.
    fn rule_ids(&mut self, auth: &[u8; 32], terminal_uuid: &str, outlet: &str) -> Result<Vec<String>, String> {
        let mut ids = vec![];
        loop {
            let page = self.child(auth, terminal_uuid, outlet, "get_schedule_rules", Some(json!({ "start_index": ids.len() })))?;
            let rules = page["rule_list"].as_array().cloned().unwrap_or_default();
            ids.extend(rules.iter().filter_map(|r| r["id"].as_str().map(String::from)));
            if rules.is_empty() || ids.len() as u64 >= page["sum"].as_u64().unwrap_or(0) {
                return Ok(ids);
            }
        }
    }

    /// Puts an outlet's wished rules on the strip, in place of the ones this plugin added before.
    fn sync_schedules(&mut self, auth: &[u8; 32], terminal_uuid: &str, outlet: &str) -> Result<(), String> {
        let mut entry = schedules_entry(outlet);
        if entry["synced"] == true {
            return Ok(());
        }
        let existing = self.rule_ids(auth, terminal_uuid, outlet)?;
        let ours: Vec<Value> = entry["ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|id| id.as_str().is_some_and(|id| existing.iter().any(|e| e == id)))
            .map(|id| json!({ "id": id }))
            .collect();
        if !ours.is_empty() {
            self.child(auth, terminal_uuid, outlet, "remove_schedule_rules", Some(json!({ "remove_all": false, "rule_list": ours })))?;
        }
        // The new ids are read back, not taken from add_schedule_rule's answer (undocumented).
        let before = self.rule_ids(auth, terminal_uuid, outlet)?;
        let added = entry["want"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .try_for_each(|rule| self.child(auth, terminal_uuid, outlet, "add_schedule_rule", Some(rule)).map(drop));
        // Read back after a failure too, so the next try removes what did get in.
        let after = self.rule_ids(auth, terminal_uuid, outlet);
        if let Ok(after) = &after {
            entry["ids"] = json!(after.iter().filter(|id| !before.contains(id)).collect::<Vec<_>>());
        }
        let done = added.and(after.map(drop));
        entry["synced"] = json!(done.is_ok());
        kv_set(&schedules_key(outlet), &entry.to_string());
        done
    }

    /// After a strip comes back: the schedules it missed while unreachable.
    fn retry_schedules(&mut self, auth: &[u8; 32], terminal_uuid: &str) {
        for outlet in self.outlets.clone() {
            if schedules_entry(&outlet)["synced"] == false {
                match self.sync_schedules(auth, terminal_uuid, &outlet) {
                    Ok(()) => log(LogLevel::Info, &format!("{outlet}: its schedules are on the strip now")),
                    Err(e) => log(LogLevel::Warn, &format!("{outlet}: schedules not on the strip: {e}")),
                }
            }
        }
    }

    /// Reports every outlet; on failure the outlets go offline until the device answers again.
    fn report(&mut self, auth: &[u8; 32], terminal_uuid: &str) {
        if !self.online && Instant::now() < self.retry_at {
            return;
        }
        match self.outlets(auth, terminal_uuid) {
            Ok(outlets) => {
                let back = !self.online;
                if back {
                    log(LogLevel::Info, &format!("{} online", self.addr));
                }
                self.online = true;
                for outlet in outlets {
                    set_available(&outlet.id, true);
                    if let Some(on) = outlet.on {
                        report_state(&outlet.id, &CapKind::OnOff, &CapValue::OnOff(on));
                    }
                }
                if back {
                    self.retry_schedules(auth, terminal_uuid);
                }
            }
            Err(e) => {
                if self.online {
                    log(LogLevel::Warn, &format!("{} offline: {e}", self.addr));
                }
                self.online = false;
                self.retry_at = Instant::now() + OFFLINE_RETRY;
                self.session = None;
                for id in &self.outlets {
                    set_available(id, false);
                }
            }
        }
    }
}

fn handshake(addr: SocketAddr, auth: &[u8; 32]) -> Result<Live, String> {
    let mut local = [0; 16];
    getrandom::fill(&mut local).map_err(|e| format!("random seed: {e}"))?;
    let first = http::post(addr, "/app/handshake1", &local, &[BINARY])?;
    if first.status != 200 || first.body.len() != 48 {
        return Err(format!("{addr}: handshake1: HTTP {} with {} bytes", first.status, first.body.len()));
    }
    let (remote, server_hash) = first.body.split_at(16);
    if server_hash != klap::server_hash(&local, remote, auth) {
        return Err(format!("{addr}: {AUTH_HINT}"));
    }
    let cookie_value = |name: &str| first.cookies.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    let cookie = format!("TP_SESSIONID={}", cookie_value("TP_SESSIONID").ok_or(format!("{addr}: no session cookie"))?);
    let timeout = cookie_value("TIMEOUT").and_then(|t| t.parse().ok()).unwrap_or(DEFAULT_SESSION);

    let second = http::post(addr, "/app/handshake2", &klap::client_hash(&local, remote, auth), &[BINARY, ("Cookie", &cookie)])?;
    if second.status != 200 {
        return Err(format!("{addr}: handshake2: HTTP {}", second.status));
    }
    Ok(Live {
        klap: klap::Session::new(&local, remote, auth),
        cookie,
        expires: Instant::now() + Duration::from_secs(timeout).saturating_sub(SESSION_MARGIN),
    })
}

fn schedules_key(outlet: &str) -> String {
    format!("schedules:{outlet}")
}

/// What the core handed over for an outlet (`want`, as strip rules), the ids of the rules this
/// plugin added (`ids`), and whether the strip holds `want` (`synced`). Nothing yet: in sync.
fn schedules_entry(outlet: &str) -> Value {
    kv_get(&schedules_key(outlet))
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "want": [], "ids": [], "synced": true }))
}

/// A schedule as a strip rule, in the shape of the Tapo app's (as the tapo crate sends it).
fn rule(schedule: &Schedule) -> Result<Value, String> {
    let (CapKind::OnOff, CapValue::OnOff(on)) = (&schedule.kind, &schedule.value) else {
        return Err(format!("only on_off runs on the strip, not {}", schedule.kind));
    };
    let (s_type, s_min, time_offset) = match schedule.at {
        TimeOfDay::Clock(minutes) => ("normal", minutes, 0),
        TimeOfDay::Sunrise(offset) => ("sunrise", 0, offset),
        TimeOfDay::Sunset(offset) => ("sunset", 0, offset),
    };
    Ok(json!({
        "enable": true, "mode": "repeat", "week_day": sunday_first(schedule.days),
        "s_type": s_type, "s_min": s_min, "time_offset": time_offset,
        "e_type": "normal", "e_min": 0, "e_action": "none",
        "desired_states": { "on": on },
        // Required, and ignored by the device.
        "year": 1970, "month": 1, "day": 1,
    }))
}

/// Days from Monday = bit 0 (the core's) to Sunday = bit 0 (the strip's).
fn sunday_first(monday_first: u8) -> u8 {
    ((monday_first << 1) | (monday_first >> 6)) & 0x7f
}

fn check(reply: &Value, method: &str) -> Result<(), String> {
    match reply["error_code"].as_i64() {
        Some(0) => Ok(()),
        code => Err(format!("{method}: device error {}", code.map_or("missing".into(), |c| c.to_string()))),
    }
}

fn parse_outlets(result: &Value) -> Vec<Outlet> {
    result["child_device_list"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|child| {
            let id = child["device_id"].as_str()?.to_string();
            let position = child["position"].as_u64().unwrap_or(0);
            let name = child["nickname"]
                .as_str()
                .and_then(|n| BASE64.decode(n).ok())
                .and_then(|n| String::from_utf8(n).ok())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!("P300 prise {position}"));
            Some(Outlet { id, name, on: child["device_on"].as_bool() })
        })
        .collect()
}

fn parse_host(host: &str) -> Result<SocketAddr, String> {
    host.parse::<SocketAddr>()
        .or_else(|_| host.parse::<IpAddr>().map(|ip| SocketAddr::new(ip, 80)))
        .map_err(|_| format!("invalid host `{host}`: expected an IP address, optionally with :port"))
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

fn entity(id: String, name: String, device: String) -> DiscoveredEntity {
    DiscoveredEntity { local_id: id, name, device: Some(device), caps: vec![(CapKind::OnOff, CapMeta::default())] }
}

struct Tplink;

impl Plugin for Tplink {
    fn manifest() -> Manifest {
        Manifest { id: "tplink".into(), version: env!("CARGO_PKG_VERSION").into(), permissions: vec![LOCAL_NETWORK.into()] }
    }

    fn init(settings: BTreeMap<String, String>) -> Result<(), String> {
        let hosts = settings.get("hosts").ok_or("missing setting `hosts` (comma-separated IPs of the P300s)")?;
        let hosts = hosts
            .split(',')
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .map(|h| Ok(Host { addr: parse_host(h)?, session: None, outlets: vec![], online: false, retry_at: Instant::now() }))
            .collect::<Result<Vec<_>, String>>()?;
        let username = secret("username").ok_or("missing secret `username` in [tplink] of secrets.toml")?;
        let password = secret("password").ok_or("missing secret `password` in [tplink] of secrets.toml")?;
        let mut terminal = [0; 16];
        getrandom::fill(&mut terminal).map_err(|e| format!("random terminal id: {e}"))?;
        *STATE.lock().unwrap() = Some(State {
            auth: klap::auth_hash(&username, &password),
            terminal_uuid: BASE64.encode(terminal),
            hosts,
            fast_until: Instant::now(),
        });
        Ok(())
    }

    fn discover() -> Result<Vec<DiscoveredEntity>, String> {
        with_state(|state| {
            let mut entities = Vec::new();
            let mut failed = 0;
            for host in &mut state.hosts {
                let cache_key = format!("children:{}", host.addr);
                let live = host.call(&state.auth, &state.terminal_uuid, "get_device_info", None).and_then(|info| {
                    let device = info["device_id"].as_str().unwrap_or_default().to_string();
                    Ok((device, host.outlets(&state.auth, &state.terminal_uuid)?))
                });
                let (device, outlets): (String, Vec<(String, String)>) = match live {
                    Ok((device, outlets)) => {
                        let outlets: Vec<_> = outlets.into_iter().map(|o| (o.id, o.name)).collect();
                        kv_set(&cache_key, &json!({ "device": device, "outlets": outlets }).to_string());
                        (device, outlets)
                    }
                    // Device off at startup: expose its last known outlets, they go online when it answers.
                    Err(e) => {
                        // One strip unplugged must not hide the others.
                        let Some(cached) = kv_get(&cache_key).and_then(|c| serde_json::from_str::<Value>(&c).ok()) else {
                            log(LogLevel::Error, &format!("{e}, and no cached outlets: they show up at the next start once it answers"));
                            failed += 1;
                            continue;
                        };
                        log(LogLevel::Warn, &format!("{e}; using cached outlets"));
                        let outlets = serde_json::from_value(cached["outlets"].clone()).map_err(|e| e.to_string())?;
                        (cached["device"].as_str().unwrap_or_default().to_string(), outlets)
                    }
                };
                host.outlets = outlets.iter().map(|(id, _)| id.clone()).collect();
                entities.extend(outlets.into_iter().map(|(id, name)| entity(id, name, device.clone())));
            }
            // None answered: fail, so the host restarts the plugin (and retries) with backoff.
            if failed > 0 && failed == state.hosts.len() {
                return Err("no P300 answered, and none has cached outlets".into());
            }
            Ok(entities)
        })
    }

    fn apply(local_id: &str, kind: CapKind, value: CapValue) -> Result<(), String> {
        let (CapKind::OnOff, CapValue::OnOff(on)) = (&kind, value) else {
            return Err(format!("unsupported capability {kind}"));
        };
        with_state(|state| {
            let State { auth, terminal_uuid, hosts, fast_until } = state;
            let host = hosts
                .iter_mut()
                .find(|h| h.outlets.iter().any(|o| o == local_id))
                .ok_or_else(|| format!("unknown outlet {local_id}"))?;
            let params = json!({
                "device_id": local_id,
                "requestData": { "method": "set_device_info", "params": { "device_on": on } },
            });
            let result = host.call(auth, terminal_uuid, "control_child", Some(params))?;
            check(&result["responseData"], "control_child")?;
            // Confirmed by the refresh the host calls next, from the device's own state.
            *fast_until = Instant::now() + FAST_WINDOW;
            Ok(())
        })
    }

    fn refresh() -> u32 {
        with_state(|state| {
            for host in &mut state.hosts {
                host.report(&state.auth, &state.terminal_uuid);
            }
            Ok(if Instant::now() < state.fast_until { FAST_REFRESH_MS } else { SLOW_REFRESH_MS })
        })
        .unwrap_or(SLOW_REFRESH_MS)
    }
}

impl Actions for Tplink {
    fn actions() -> Vec<ActionDecl> {
        vec![]
    }

    fn invoke(_: &str, name: &str, _: Value) -> Result<(), String> {
        Err(format!("no action `{name}`"))
    }
}

impl Schedules for Tplink {
    fn set_schedules(local_id: &str, schedules: Vec<Schedule>) -> Result<(), String> {
        let want = json!(schedules.iter().map(rule).collect::<Result<Vec<_>, _>>()?);
        with_state(|state| {
            let State { auth, terminal_uuid, hosts, .. } = state;
            let host = hosts
                .iter_mut()
                .find(|h| h.outlets.iter().any(|o| o == local_id))
                .ok_or_else(|| format!("unknown outlet {local_id}"))?;
            let mut entry = schedules_entry(local_id);
            if entry["want"] != want {
                entry["want"] = want;
                entry["synced"] = json!(false);
                kv_set(&schedules_key(local_id), &entry.to_string());
            }
            // Ok only once the strip holds them: the app then says they run without the hub.
            host.sync_schedules(auth, terminal_uuid, local_id).map_err(|e| format!("{e} (tried again when the strip answers)"))
        })
    }
}

homemaid_plugin::export_with_schedules!(Tplink with_types_in homemaid_plugin::bindings_schedules);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_p300_children() {
        // Shape of python-kasa's fixture P300(EU)_1.0_1.0.15.json.
        let result = json!({ "child_device_list": [
            { "device_id": "A", "position": 1, "device_on": true, "nickname": "TGFtcGU=" },
            { "device_id": "B", "position": 2, "device_on": false, "nickname": "" },
            { "position": 3 },
        ], "start_index": 0, "sum": 3 });
        let outlets = parse_outlets(&result);
        assert_eq!(outlets.len(), 2, "children without device_id are skipped");
        assert_eq!((outlets[0].id.as_str(), outlets[0].name.as_str(), outlets[0].on), ("A", "Lampe", Some(true)));
        assert_eq!(outlets[1].name, "P300 prise 2");
    }

    #[test]
    fn days_move_from_monday_first_to_sunday_first() {
        assert_eq!(sunday_first(0b000_0001), 0b000_0010, "monday");
        assert_eq!(sunday_first(0b100_0000), 0b000_0001, "sunday");
        assert_eq!(sunday_first(0x7f), 0x7f);
        let dusk = Schedule { id: "dusk".into(), kind: CapKind::OnOff, value: CapValue::OnOff(true), at: TimeOfDay::Sunset(-15), days: 0b001_0001 };
        let rule = rule(&dusk).unwrap();
        assert_eq!((rule["s_type"].as_str(), rule["time_offset"].as_i64(), rule["week_day"].as_u64()), (Some("sunset"), Some(-15), Some(0b010_0010)));
        assert!(super::rule(&Schedule { kind: CapKind::Brightness, value: CapValue::Brightness(5), ..dusk }).is_err());
    }

    #[test]
    fn parses_hosts() {
        assert_eq!(parse_host("192.168.1.50").unwrap(), "192.168.1.50:80".parse().unwrap());
        assert_eq!(parse_host("127.0.0.1:8080").unwrap(), "127.0.0.1:8080".parse().unwrap());
        assert!(parse_host("p300.local").is_err());
    }
}

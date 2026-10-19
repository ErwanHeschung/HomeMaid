//! TP-Link Tapo P300 power strip, local control over KLAP v2: one `on_off` entity per outlet.
//!
//! Settings: `hosts` = comma-separated IPs (`ip` or `ip:port`). Secrets: `username`, `password`
//! (TP-Link account). Requires "Third-Party Compatibility" in the Tapo app (Me > Third-Party
//! Services), otherwise recent firmwares speak TPAP, which is not supported.

pub mod klap;

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use homemaid_plugin::{
    CapKind, http, CapMeta, CapValue, DiscoveredEntity, LOCAL_NETWORK, LogLevel, Manifest, Plugin, kv_get, kv_set, log,
    report_state, secret, set_available,
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

    /// Reports every outlet; on failure the outlets go offline until the device answers again.
    fn report(&mut self, auth: &[u8; 32], terminal_uuid: &str) {
        if !self.online && Instant::now() < self.retry_at {
            return;
        }
        match self.outlets(auth, terminal_uuid) {
            Ok(outlets) => {
                if !self.online {
                    log(LogLevel::Info, &format!("{} online", self.addr));
                }
                self.online = true;
                for outlet in outlets {
                    set_available(&outlet.id, true);
                    if let Some(on) = outlet.on {
                        report_state(&outlet.id, &CapKind::OnOff, &CapValue::OnOff(on));
                    }
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

homemaid_plugin::export!(Tplink with_types_in homemaid_plugin::bindings);

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
    fn parses_hosts() {
        assert_eq!(parse_host("192.168.1.50").unwrap(), "192.168.1.50:80".parse().unwrap());
        assert_eq!(parse_host("127.0.0.1:8080").unwrap(), "127.0.0.1:8080".parse().unwrap());
        assert!(parse_host("p300.local").is_err());
    }
}

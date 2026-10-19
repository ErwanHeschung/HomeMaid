//! Throwaway probe: does a P300 take schedule rules per outlet, and in which shape?
//!
//! ```text
//! HOST=192.168.1.50 USER=you@example.com PASS=... cargo run -p tplink-plugin --example probe
//! HOST=... USER=... PASS=... cargo run -p tplink-plugin --example probe -- add      # one disabled rule on outlet 1
//! HOST=... USER=... PASS=... cargo run -p tplink-plugin --example probe -- remove <rule id>
//! ```
//! Read-only without arguments. `add` leaves the rule in place so it can be checked in the Tapo
//! app (disabled, 23:59, every day, off); `remove` takes it away again.

use std::{env, net::SocketAddr, time::{SystemTime, UNIX_EPOCH}};

use homemaid_plugin::http;
use serde_json::{Value, json};
use tplink_plugin::klap;

const BINARY: (&str, &str) = ("Content-Type", "application/octet-stream");

struct Device {
    addr: SocketAddr,
    klap: klap::Session,
    cookie: String,
}

impl Device {
    fn connect(addr: SocketAddr, auth: &[u8; 32]) -> Result<Self, String> {
        let mut local = [0; 16];
        getrandom::fill(&mut local).map_err(|e| e.to_string())?;
        let first = http::post(addr, "/app/handshake1", &local, &[BINARY])?;
        if first.status != 200 || first.body.len() != 48 {
            return Err(format!("handshake1: HTTP {} with {} bytes", first.status, first.body.len()));
        }
        let (remote, server_hash) = first.body.split_at(16);
        if server_hash != klap::server_hash(&local, remote, auth) {
            return Err("credentials refused (or Third-Party Compatibility is off)".into());
        }
        let session = first.cookies.iter().find(|(k, _)| k == "TP_SESSIONID").ok_or("no session cookie")?;
        let cookie = format!("TP_SESSIONID={}", session.1);
        let second = http::post(addr, "/app/handshake2", &klap::client_hash(&local, remote, auth), &[BINARY, ("Cookie", &cookie)])?;
        if second.status != 200 {
            return Err(format!("handshake2: HTTP {}", second.status));
        }
        Ok(Self { addr, klap: klap::Session::new(&local, remote, auth), cookie })
    }

    /// The whole reply, error_code included: the point is to see what the device says.
    fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let mut request = json!({ "method": method, "request_time_milis": ms, "terminal_uuid": "aG9tZW1haWQtcHJvYmU=" });
        if let Some(params) = params {
            request["params"] = params;
        }
        let (body, seq) = self.klap.encrypt(request.to_string().as_bytes());
        let response = http::post(self.addr, &format!("/app/request?seq={seq}"), &body, &[BINARY, ("Cookie", &self.cookie)])?;
        if response.status != 200 {
            return Err(format!("{method}: HTTP {}", response.status));
        }
        serde_json::from_slice(&self.klap.decrypt(seq, &response.body)?).map_err(|e| e.to_string())
    }

    fn child(&mut self, device_id: &str, method: &str, params: Option<Value>) -> Result<Value, String> {
        let mut inner = json!({ "method": method });
        if let Some(params) = params {
            inner["params"] = params;
        }
        self.request("control_child", Some(json!({ "device_id": device_id, "requestData": inner })))
    }
}

fn show(what: &str, reply: Result<Value, String>) -> Option<Value> {
    match reply {
        Ok(v) => {
            println!("== {what}\n{}\n", serde_json::to_string_pretty(&v).unwrap_or_default());
            Some(v)
        }
        Err(e) => {
            println!("== {what}\nERROR {e}\n");
            None
        }
    }
}

fn main() -> Result<(), String> {
    let var = |name: &str| env::var(name).map_err(|_| format!("set {name}"));
    let host = var("HOST")?;
    let addr: SocketAddr = if host.contains(':') { host.parse() } else { format!("{host}:80").parse() }.map_err(|e| format!("HOST: {e}"))?;
    let mut device = Device::connect(addr, &klap::auth_hash(&var("USER")?, &var("PASS")?))?;

    let info = show("get_device_info (parent)", device.request("get_device_info", None)).unwrap_or_default();
    let info = &info["result"];
    println!(
        "-> model {} fw {} region {} time_diff {} latitude {} longitude {}\n",
        info["model"], info["fw_ver"], info["region"], info["time_diff"], info["latitude"], info["longitude"]
    );
    let children = show("get_child_device_list", device.request("get_child_device_list", None)).unwrap_or_default();
    let outlets: Vec<String> = children["result"]["child_device_list"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["device_id"].as_str().map(String::from))
        .collect();
    let outlet = outlets.first().ok_or("no outlet found")?.clone();
    println!("-> outlet 1 = {outlet}\n");

    show("get_schedule_rules, parent", device.request("get_schedule_rules", Some(json!({ "start_index": 0 }))));
    show("get_max_schedule_rules, parent", device.request("get_max_schedule_rules", None));
    show("get_schedule_rules, outlet 1 via control_child", device.child(&outlet, "get_schedule_rules", Some(json!({ "start_index": 0 }))));
    show("get_max_schedule_rules, outlet 1 via control_child", device.child(&outlet, "get_max_schedule_rules", None));

    match env::args().nth(1).as_deref() {
        Some("add") => {
            // Disabled, so it never fires: 23:59 every day, off. Same shape as the tapo crate's.
            let rule = json!({
                "enable": false, "year": 1970, "month": 1, "day": 1, "mode": "repeat", "week_day": 127,
                "s_type": "normal", "s_min": 1439, "time_offset": 0,
                "e_min": 0, "e_type": "normal", "e_action": "none", "desired_states": { "on": false },
            });
            show("add_schedule_rule, outlet 1", device.child(&outlet, "add_schedule_rule", Some(rule)));
            show("get_schedule_rules, outlet 1, after add", device.child(&outlet, "get_schedule_rules", Some(json!({ "start_index": 0 }))));
            println!("-> now look at outlet 1 in the Tapo app: a disabled schedule at 23:59 should be there.");
        }
        Some("remove") => {
            let id = env::args().nth(2).ok_or("remove <rule id>")?;
            let params = json!({ "remove_all": false, "rule_list": [{ "id": id }] });
            show("remove_schedule_rules, outlet 1", device.child(&outlet, "remove_schedule_rules", Some(params)));
            show("get_schedule_rules, outlet 1, after remove", device.child(&outlet, "get_schedule_rules", Some(json!({ "start_index": 0 }))));
        }
        _ => {}
    }
    Ok(())
}

//! Mock plugin for integration tests: one lamp that confirms every command instantly.
//!
//! Setting `crash_once = "true"` makes the first refresh panic, once (remembered in the KV store),
//! to test crash recovery. Its action `blink { times }` is recorded in the KV store (`blinked`),
//! and so are the schedules it is handed (`schedules:<local id>`, as `<id> <kind>=<value> <at>
//! <days>` lines; plugin-api 1.2).
//!
//! Devices the user adds (plugin-api 1.3): each is a bulb `bulb-<id>` named after its `name`
//! setting, accepted only with the secret `password = "right"`.

use std::{collections::BTreeMap, sync::Mutex};

use homemaid_plugin::{
    ActionDecl, Actions, CapKind, CapMeta, CapValue, DiscoveredEntity, Devices, LOCAL_NETWORK, Manifest, Plugin, Schedule,
    Schedules, report_state,
};

static SETTINGS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

struct Mock;

impl Plugin for Mock {
    fn manifest() -> Manifest {
        Manifest { id: "mock".into(), version: env!("CARGO_PKG_VERSION").into(), permissions: vec![LOCAL_NETWORK.into()] }
    }

    fn init(settings: BTreeMap<String, String>) -> Result<(), String> {
        *SETTINGS.lock().unwrap() = settings;
        Ok(())
    }

    fn discover() -> Result<Vec<DiscoveredEntity>, String> {
        Ok(vec![DiscoveredEntity {
            local_id: "lamp".into(),
            name: "Mock lamp".into(),
            device: Some("mock-device".into()),
            caps: vec![
                (CapKind::OnOff, CapMeta::default()),
                (CapKind::Brightness, CapMeta { max: Some(100.0), unit: Some("%".into()), ..Default::default() }),
                (CapKind::Power, CapMeta { read_only: true, unit: Some("W".into()), ..Default::default() }),
            ],
        }])
    }

    fn apply(local_id: &str, kind: CapKind, value: CapValue) -> Result<(), String> {
        if local_id != "lamp" && !local_id.starts_with("bulb-") {
            return Err(format!("unknown entity {local_id}"));
        }
        report_state(local_id, &kind, &value);
        Ok(())
    }

    fn refresh() -> u32 {
        let crash_once = SETTINGS.lock().unwrap().get("crash_once").is_some_and(|v| v == "true");
        if crash_once && homemaid_plugin::kv_get("crashed").is_none() {
            homemaid_plugin::kv_set("crashed", "1");
            panic!("crash requested by crash_once");
        }
        report_state("lamp", &CapKind::Power, &CapValue::Power { watts: 4.2 });
        1000
    }
}

impl Actions for Mock {
    fn actions() -> Vec<ActionDecl> {
        let times = serde_json::json!({ "type": "object", "properties": { "times": { "type": "integer" } } });
        vec![ActionDecl { local_id: "lamp".into(), name: "blink".into(), params: Some(times) }]
    }

    fn invoke(local_id: &str, name: &str, params: serde_json::Value) -> Result<(), String> {
        if (local_id, name) != ("lamp", "blink") {
            return Err(format!("unknown action {local_id}/{name}"));
        }
        homemaid_plugin::kv_set("blinked", &params["times"].as_u64().unwrap_or(1).to_string());
        Ok(())
    }
}

impl Schedules for Mock {
    fn set_schedules(local_id: &str, schedules: Vec<Schedule>) -> Result<(), String> {
        let lines: Vec<String> = schedules
            .iter()
            .map(|s| format!("{} {}={} {:?} {:#09b}", s.id, s.kind, serde_json::to_string(&s.value).unwrap_or_default(), s.at, s.days))
            .collect();
        homemaid_plugin::kv_set(&format!("schedules:{local_id}"), &lines.join("\n"));
        Ok(())
    }
}

impl Devices for Mock {
    fn add_device(id: &str, settings: BTreeMap<String, String>) -> Result<Vec<DiscoveredEntity>, String> {
        if settings.get("password").map(String::as_str) != Some("right") {
            return Err("wrong password".into());
        }
        let name = settings.get("name").cloned().unwrap_or_else(|| "Mock bulb".into());
        Ok(vec![DiscoveredEntity { local_id: format!("bulb-{id}"), name, device: Some(id.into()), caps: vec![(CapKind::OnOff, CapMeta::default())] }])
    }
}

homemaid_plugin::export_with_devices!(Mock with_types_in homemaid_plugin::bindings_devices);

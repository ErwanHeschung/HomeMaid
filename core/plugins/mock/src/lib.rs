//! Mock plugin for integration tests: one lamp that confirms every command instantly.
//!
//! Setting `crash_once = "true"` makes the first refresh panic, once (remembered in the KV store),
//! to test crash recovery.

use std::{collections::BTreeMap, sync::Mutex};

use homemaid_plugin::{CapKind, CapMeta, CapValue, DiscoveredEntity, LOCAL_NETWORK, Manifest, Plugin, report_state};

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
        if local_id != "lamp" {
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

homemaid_plugin::export!(Mock with_types_in homemaid_plugin::bindings);

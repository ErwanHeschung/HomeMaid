//! End to end: the mock plugin compiled to wasm32-wasip2, driven by the real host and reconciler.

mod common;

use std::time::{Duration, Instant};

use common::{Plugin, start, start_wasm, wait_for};
use homemaid::model::{CapKind, CapValue, CommandStatus, entity_uuid};

fn mock(permissions: &'static [&'static str], settings: &'static [(&'static str, &'static str)]) -> Plugin<'static> {
    Plugin { id: "mock", package: "mock-plugin", permissions, settings, secrets: &[] }
}

#[tokio::test(flavor = "multi_thread")]
async fn mock_lamp_is_discovered_survives_a_crash_and_confirms_commands() {
    let (core, data) = start(mock(&["local-network"], &[("crash_once", "true")]));
    let lamp = entity_uuid("mock", "lamp");
    let available = || core.with(|r| r.entities.get(&lamp).is_some_and(|e| e.available));

    // The first refresh panics right after discovery; the plugin is restarted after 1 s.
    wait_for("discovery", Duration::from_secs(10), available).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let kv = std::fs::read_to_string(data.0.join("plugins/mock.kv.json")).unwrap();
    assert!(kv.contains("crashed"), "the plugin crashed once");
    assert!(available(), "restarted, and the crash flag persisted in the KV store prevents a second crash");
    let power = core.with(|r| r.entities[&lamp].caps[&CapKind::Power].observed.clone());
    assert_eq!(power, Some(CapValue::Power { watts: 4.2 }));

    core.with(|r| r.set_desired(lamp, &CapKind::OnOff, CapValue::OnOff(true), Instant::now())).unwrap();
    let status = || core.with(|r| r.entities[&lamp].caps[&CapKind::OnOff].status.clone());
    wait_for("confirmation", Duration::from_secs(2), || status() == CommandStatus::Confirmed).await;

    // plugin-api 1.1: a declared action, invoked through the host.
    assert_eq!(core.with(|r| r.entities[&lamp].actions.iter().map(|a| a.name.clone()).collect::<Vec<_>>()), ["blink"]);
    let plugins = core.plugins.get().unwrap();
    plugins.invoke(&core, lamp, "blink", serde_json::json!({ "times": 3 })).await.unwrap();
    let kv = std::fs::read_to_string(data.0.join("plugins/mock.kv.json")).unwrap();
    assert!(kv.contains(r#""blinked":"3""#), "{kv}");
    assert!(plugins.invoke(&core, lamp, "explode", serde_json::json!({})).await.is_err(), "undeclared");
}

#[tokio::test(flavor = "multi_thread")]
async fn plugin_asking_for_ungranted_permissions_is_refused() {
    let (core, _data) = start(mock(&[], &[]));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(core.with(|r| r.entities.is_empty()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_built_for_api_1_0_still_runs() {
    // Built from the mock before plugin-api 1.1; it imports host@1.0.0 and has no actions.
    let wasm = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock-plugin-api-1.0.wasm");
    let (core, _data) = start_wasm(mock(&["local-network"], &[]), wasm);
    let lamp = entity_uuid("mock", "lamp");
    wait_for("discovery", Duration::from_secs(10), || core.with(|r| r.entities.get(&lamp).is_some_and(|e| e.available))).await;
    assert!(core.with(|r| r.entities[&lamp].actions.is_empty()));
    let plugins = core.plugins.get().unwrap();
    assert!(plugins.invoke(&core, lamp, "blink", serde_json::json!({})).await.is_err(), "no actions in 1.0");
}

//! End to end: the pc plugin compiled to wasm32-wasip2 against a real agent (fallback desktop).
// Fallback desktop only: on Windows these would move the real mouse and change the real volume.
#![cfg(not(windows))]

mod common;

use std::{collections::BTreeMap, time::{Duration, Instant}};

use common::{Plugin, start, wait_for};
use homemaid::{
    agent::{self, Request},
    config::AgentConfig,
    model::{CapKind, CapValue, CommandStatus, entity_uuid},
};

async fn start_agent() -> (String, tokio::task::JoinHandle<()>) {
    let config = AgentConfig {
        enabled: true,
        bind: "127.0.0.1:0".parse().unwrap(),
        token: "secret".into(),
        apps: BTreeMap::from([("notepad".to_string(), "notepad.exe".into())]),
        shortcuts: vec![],
        discover_apps: false,
    };
    let (stopping, _) = tokio::sync::watch::channel(());
    let app = agent::router(&config, &stopping).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    (addr, tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
        drop(stopping);
    }))
}

fn pc<'a>(agent: &'a str, token: &'a str) -> Plugin<'a> {
    Plugin {
        id: "pc",
        package: "pc-plugin",
        permissions: &["local-network", "loopback"],
        settings: Box::leak(Box::new([("agent", agent)])),
        secrets: Box::leak(Box::new([("token", token)])),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pc_is_an_entity_with_volume_and_sensors() {
    let (addr, server) = start_agent().await;
    let (core, _data) = start(pc(&addr, "secret"));
    let id = entity_uuid("pc", "pc");
    let available = || core.with(|r| r.entities.get(&id).is_some_and(|e| e.available));
    let slot = |kind: CapKind| core.with(|r| r.entities[&id].caps[&kind].clone());

    wait_for("discovery", Duration::from_secs(10), available).await;
    let memory = CapKind::Custom("memory".into());
    wait_for("sensors", Duration::from_secs(5), || slot(memory.clone()).observed.is_some()).await;
    assert!(slot(memory).meta.read_only);

    core.with(|r| r.set_desired(id, &CapKind::Volume, CapValue::Volume(30), Instant::now())).unwrap();
    wait_for("volume confirmed", Duration::from_secs(5), || slot(CapKind::Volume).status == CommandStatus::Confirmed).await;
    assert!(agent::recorded().contains(&Request::SetVolume { level: 30 }));

    // Actions (plugin-api 1.1): sent to the agent, not states.
    let plugins = core.plugins.get().unwrap();
    plugins.invoke(&core, id, "next", serde_json::json!({})).await.unwrap();
    plugins.invoke(&core, id, "lock", serde_json::Value::Null).await.unwrap();
    assert!(agent::recorded().contains(&Request::Media { action: agent::Media::Next }));
    assert!(agent::recorded().contains(&Request::Power { action: agent::Power::Lock }));
    let err = plugins.invoke(&core, id, "next", serde_json::json!({ "x": 1 })).await.unwrap_err();
    assert!(err.contains("no parameters"), "{err}");
    let err = plugins.invoke(&core, id, "open", serde_json::json!({})).await.unwrap_err();
    assert!(err.contains("needs `shortcut`"), "{err}");
    // The editor offers the agent's apps; a wrong one is refused in the agent's words.
    let launch = core.with(|r| r.entities[&id].actions.iter().find(|a| a.name == "launch").cloned()).unwrap();
    assert_eq!(launch.params.unwrap()["properties"]["app"]["enum"], serde_json::json!(["notepad"]));
    let err = plugins.invoke(&core, id, "launch", serde_json::json!({ "app": "Netflix" })).await.unwrap_err();
    assert_eq!(err, "app `Netflix` is not in [agent.apps]");

    server.abort();
    wait_for("offline", Duration::from_secs(10), || !available()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_token_is_refused() {
    let (addr, _server) = start_agent().await;
    let (core, _data) = start(pc(&addr, "wrong"));
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(core.with(|r| r.entities.is_empty()));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_on_this_pc_needs_the_loopback_permission() {
    let (addr, _server) = start_agent().await;
    let (core, _data) = start(Plugin { permissions: &["local-network"], ..pc(&addr, "secret") });
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(core.with(|r| r.entities.is_empty()), "loopback is not part of local-network");
}

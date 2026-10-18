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
    let config = AgentConfig { bind: "127.0.0.1:0".parse().unwrap(), token: "secret".into(), apps: BTreeMap::new(), shortcuts: vec![] };
    let app = agent::router(&config).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    (addr, tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }))
}

fn pc<'a>(agent: &'a str, token: &'a str) -> Plugin<'a> {
    Plugin {
        id: "pc",
        package: "pc-plugin",
        permissions: &["local-network"],
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

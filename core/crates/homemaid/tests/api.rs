//! The public API end to end, with the mock plugin: live notifications, `entity.update` saved to
//! the config file, and the pairing page.

mod common;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use common::{Plugin, start, wait_for};
use futures_util::{SinkExt, StreamExt};
use homemaid::{Core, api, config, model::entity_uuid};
use serde_json::{Value, json};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const CONFIG: &str = "# kept\n[core]\nbind = \"127.0.0.1:0\"\ndata_dir = \"data\"\ntoken = \"0123456789abcdef\"\n";

async fn serve(core: Arc<Core>, config_path: &std::path::Path) -> SocketAddr {
    let config = config::load(config_path).unwrap().core.unwrap();
    let app = api::router(core, &config, config_path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap()
    });
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn update_is_saved_and_notified() {
    let (core, data) = start(Plugin { id: "mock", package: "mock-plugin", permissions: &["local-network"], settings: &[], secrets: &[] });
    let lamp = entity_uuid("mock", "lamp");
    wait_for("discovery", Duration::from_secs(10), || core.with(|r| r.entities.contains_key(&lamp))).await;
    let config_path = data.0.join("homemaid.toml");
    std::fs::write(&config_path, CONFIG).unwrap();
    let addr = serve(core.clone(), &config_path).await;

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws?token=0123456789abcdef")).await.unwrap();
    let mut call = async |id: u32, method: &str, params: Value| {
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        ws.send(Message::Text(request.to_string().into())).await.unwrap();
        let mut seen = Vec::new();
        loop {
            let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("closed") };
            let message: Value = serde_json::from_str(&text).unwrap();
            if message["id"] == id {
                return (message, seen);
            }
            seen.push(message);
        }
    };
    call(1, "subscribe", json!(null)).await;
    let (reply, _) = call(2, "entity.update", json!({ "id": lamp, "room": "salon", "name": "Lampe" })).await;
    assert_eq!(reply["result"], true, "{reply}");
    // The notification may come before or after the reply.
    let (_, mut seen) = call(3, "entities.list", json!(null)).await;
    let (reply, _) = call(4, "entity.update", json!({ "id": "00000000-0000-0000-0000-000000000000", "room": "x" })).await;
    assert!(reply["error"].is_object());
    seen.retain(|m| m["method"] == "entity.changed");
    assert_eq!(seen[0]["params"]["entity"]["room"], "salon");

    assert_eq!(core.with(|r| r.entities[&lamp].room.clone()).as_deref(), Some("salon"));
    let saved = config::load(&config_path).unwrap();
    assert!(std::fs::read_to_string(&config_path).unwrap().starts_with("# kept\n"));
    assert_eq!(saved.entities[0].room.as_deref(), Some("salon"));
    assert_eq!(core.overrides.lock().unwrap()[&lamp].name.as_deref(), Some("Lampe"), "rediscovery uses it");

    // Actions and automations, through the same socket.
    let (reply, _) = call(5, "entity.invoke", json!({ "id": lamp, "action": "blink", "params": { "times": 2 } })).await;
    assert_eq!(reply["result"], true, "{reply}");
    let automations: Vec<homemaid::automations::AutomationConfig> = toml::from_str::<toml::Table>(
        r#"
        [[automation]]
        id = "dim"
        [[automation.trigger]]
        at = "07:00"
        [[automation.action]]
        set = { select = "room:salon & cap:brightness", cap = "brightness", value = 30 }
        "#,
    )
    .unwrap()["automation"]
        .clone()
        .try_into()
        .unwrap();
    let _ = core.automations.set(Arc::new(homemaid::automations::Automations::new(&automations, None).unwrap()));
    let (reply, _) = call(6, "automations.list", json!(null)).await;
    assert_eq!(reply["result"][0]["id"], "dim", "{reply}");
    assert!(reply["result"][0]["next_run"].is_string());
    let (reply, _) = call(7, "automation.dry_run", json!({ "id": "dim" })).await;
    assert_eq!(reply["result"]["actions"][0]["targets"][0]["name"], "Lampe", "{reply}");
    let (reply, seen) = call(8, "automation.run", json!({ "id": "dim" })).await;
    assert_eq!(reply["result"]["results"][0]["outcome"], "pending", "{reply}");
    let (_, more) = call(9, "entities.list", json!(null)).await;
    assert!(seen.iter().chain(&more).any(|m| m["method"] == "automation.ran"), "notified");
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_page_is_served_to_the_pc_itself() {
    let (core, data) = start(Plugin { id: "mock", package: "mock-plugin", permissions: &[], settings: &[], secrets: &[] });
    let config_path = data.0.join("homemaid.toml");
    std::fs::write(&config_path, CONFIG).unwrap();
    let addr = serve(core, &config_path).await;
    let get = async |path: &str, host: &str| {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        let mut page = String::new();
        stream.read_to_string(&mut page).await.unwrap();
        page
    };
    let page = get("/pair", "localhost:8123").await;
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("<svg"), "{page}");
    let rebound = get("/pair", "evil.example:8123").await;
    assert!(rebound.starts_with("HTTP/1.1 403"), "a page rebound to 127.0.0.1 gets nothing: {rebound}");

    let nonce = "00112233445566778899aabbccddeeff";
    let hello = get(&format!("/hello?nonce={nonce}"), "x").await;
    assert!(hello.ends_with(&homemaid::pairing::proof("0123456789abcdef", nonce)), "{hello}");
}

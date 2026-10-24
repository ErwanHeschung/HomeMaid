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
    core.set_automations(homemaid::automations::Automations::new(&automations, None).unwrap());
    let (reply, _) = call(6, "automations.list", json!(null)).await;
    assert_eq!(reply["result"][0]["id"], "dim", "{reply}");
    assert!(reply["result"][0]["next_run"].is_string());
    let (reply, _) = call(7, "automation.dry_run", json!({ "id": "dim" })).await;
    assert_eq!(reply["result"]["actions"][0]["targets"][0]["name"], "Lampe", "{reply}");
    let (reply, seen) = call(8, "automation.run", json!({ "id": "dim" })).await;
    assert_eq!(reply["result"]["results"][0]["outcome"], "pending", "{reply}");
    // automation.run answers aside (it may wait for a plugin): its notification can come before
    // or after the reply.
    let ran = |m: &Value| m["method"] == "automation.ran";
    let notified = seen.iter().any(ran)
        || tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(Ok(Message::Text(text))) = ws.next().await {
                if ran(&serde_json::from_str(&text).unwrap()) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
    assert!(notified, "notified");
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

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Sends a request and returns its reply; notifications read meanwhile go to `inbox`.
async fn rpc(ws: &mut Ws, inbox: &mut Vec<Value>, id: u32, method: &str, params: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    ws.send(Message::Text(request.to_string().into())).await.unwrap();
    loop {
        let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("closed") };
        let message: Value = serde_json::from_str(&text).unwrap();
        if message["id"] == id {
            return message;
        }
        inbox.push(message);
    }
}

/// Whether the notification `method` came, or comes within 5 s.
async fn notified(ws: &mut Ws, inbox: &mut Vec<Value>, method: &str) -> bool {
    if inbox.iter().any(|m| m["method"] == method) {
        inbox.retain(|m| m["method"] != method);
        return true;
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(Message::Text(text))) = ws.next().await {
            if serde_json::from_str::<Value>(&text).unwrap()["method"] == method {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

#[tokio::test(flavor = "multi_thread")]
async fn automations_are_edited_live_and_saved() {
    use homemaid::model::{CapKind, CapValue};

    let (core, data) = start(Plugin { id: "mock", package: "mock-plugin", permissions: &["local-network"], settings: &[], secrets: &[] });
    let lamp = entity_uuid("mock", "lamp");
    wait_for("discovery", Duration::from_secs(10), || core.with(|r| r.entities.contains_key(&lamp))).await;
    let config_path = data.0.join("homemaid.toml");
    std::fs::write(&config_path, CONFIG).unwrap();
    let addr = serve(core.clone(), &config_path).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws?token=0123456789abcdef")).await.unwrap();
    let (ws, inbox) = (&mut ws, &mut Vec::new());
    rpc(ws, inbox, 1, "subscribe", json!(null)).await;

    let mut follow = json!({
        "id": "follow", "name": "Suit la lampe",
        "trigger": [{ "when": { "select": "plugin:mock", "cap": "on_off", "becomes": true } }],
        "action": [{ "set": { "select": "plugin:mock", "cap": "brightness", "value": 30 } }],
    });
    let reply = rpc(ws, inbox, 2, "automation.save", json!({ "automation": follow })).await;
    assert_eq!(reply["result"], true, "{reply}");
    assert!(notified(ws, inbox, "automations.changed").await);
    let text = std::fs::read_to_string(&config_path).unwrap();
    assert!(text.starts_with("# kept\n") && text.contains("[[automation]]\nid = \"follow\""), "{text}");
    let list = rpc(ws, inbox, 3, "automations.list", json!(null)).await;
    assert_eq!(list["result"][0]["config"]["trigger"][0]["when"]["cap"], "on_off", "{list}");

    // In force without a restart: switching the lamp on runs it.
    let brightness = || core.with(|r| r.entities[&lamp].caps[&CapKind::Brightness].desired.clone());
    let set_on = |on| core.with(|r| r.set_desired(lamp, &CapKind::OnOff, CapValue::OnOff(on), std::time::Instant::now())).unwrap();
    set_on(false);
    wait_for("the lamp off", Duration::from_secs(5), || core.with(|r| r.entities[&lamp].caps[&CapKind::OnOff].observed == Some(CapValue::OnOff(false)))).await;
    set_on(true);
    wait_for("the saved automation ran", Duration::from_secs(5), || brightness() == Some(CapValue::Brightness(30))).await;

    // Edited: same id, its runs stay.
    follow["name"] = json!("Suit la lampe allumée");
    rpc(ws, inbox, 4, "automation.save", json!({ "automation": follow })).await;
    let list = rpc(ws, inbox, 5, "automations.list", json!(null)).await;
    assert_eq!(list["result"][0]["name"], "Suit la lampe allumée");
    assert_eq!(list["result"][0]["runs"].as_array().map(Vec::len), Some(1), "{list}");

    // Tried before saving.
    let draft = json!({ "id": "draft", "trigger": [{ "at": "07:00" }], "action": [{ "set": { "select": "plugin:mock", "cap": "on_off", "value": false } }] });
    let dry = rpc(ws, inbox, 6, "automation.dry_run", json!({ "automation": draft })).await;
    assert_eq!(dry["result"]["actions"][0]["targets"][0]["name"], "Mock lamp", "{dry}");

    // Refused as at startup, the file untouched.
    let before = std::fs::read_to_string(&config_path).unwrap();
    let broken = json!({ "id": "broken", "trigger": [{ "at": "25:00" }], "action": [] });
    let reply = rpc(ws, inbox, 7, "automation.save", json!({ "automation": broken })).await;
    assert!(reply["error"]["message"].as_str().unwrap().contains("needs at least one"), "{reply}");
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);

    // Sunset needs where the home is; set from the app, it works at once.
    assert_eq!(rpc(ws, inbox, 20, "home.location", json!(null)).await["result"], Value::Null);
    let dusk = json!({ "id": "dusk", "trigger": [{ "at": "sunset" }], "action": [{ "set": { "select": "plugin:mock", "cap": "on_off", "value": true } }] });
    let reply = rpc(ws, inbox, 21, "automation.save", json!({ "automation": dusk })).await;
    assert!(reply["error"]["message"].as_str().unwrap().contains("latitude"), "{reply}");
    let reply = rpc(ws, inbox, 22, "home.set_location", json!({ "latitude": 95.0, "longitude": 2.35 })).await;
    assert!(reply["error"].is_object(), "out of range: {reply}");
    rpc(ws, inbox, 23, "home.set_location", json!({ "latitude": 48.86, "longitude": 2.35 })).await;
    assert_eq!(rpc(ws, inbox, 24, "home.location", json!(null)).await["result"], json!({ "latitude": 48.86, "longitude": 2.35 }));
    let reply = rpc(ws, inbox, 25, "automation.save", json!({ "automation": dusk })).await;
    assert_eq!(reply["result"], true, "{reply}");
    assert_eq!(config::load(&config_path).unwrap().core.unwrap().latitude, Some(48.86));
    rpc(ws, inbox, 26, "automation.delete", json!({ "id": "dusk" })).await;

    // A workflow: dims, then waits an hour; stopped by hand meanwhile.
    let night = json!({
        "id": "night", "start": ["dim"],
        "trigger": [{ "when": { "select": "plugin:mock", "cap": "on_off", "becomes": false } }],
        "node": [
            { "id": "dim", "set": { "select": "trigger", "cap": "brightness", "value": 10 }, "next": ["pause"] },
            { "id": "pause", "delay": "1h" },
        ],
    });
    assert_eq!(rpc(ws, inbox, 30, "automation.save", json!({ "automation": night })).await["result"], true);
    assert!(std::fs::read_to_string(&config_path).unwrap().contains("[[automation.node]]\nid = \"pause\"\ndelay = \"1h\""));
    set_on(false);
    wait_for("the workflow dimmed", Duration::from_secs(5), || brightness() == Some(CapValue::Brightness(10))).await;
    let list = rpc(ws, inbox, 31, "automations.list", json!(null)).await;
    let listed = list["result"].as_array().unwrap().iter().find(|a| a["id"] == "night").unwrap().clone();
    assert_eq!(listed["running"][0]["nodes"], json!(["pause"]), "{listed}");
    assert_eq!(rpc(ws, inbox, 32, "automation.stop", json!({ "id": "night" })).await["result"], 1);
    let list = rpc(ws, inbox, 33, "automations.list", json!(null)).await;
    let run = &list["result"].as_array().unwrap().iter().find(|a| a["id"] == "night").unwrap()["runs"][0];
    assert_eq!((run["stopped"].as_str(), run["steps"][0]["node"].as_str()), (Some("by hand"), Some("dim")), "{run}");
    rpc(ws, inbox, 34, "automation.delete", json!({ "id": "night" })).await;

    let reply = rpc(ws, inbox, 8, "automation.delete", json!({ "id": "follow" })).await;
    assert_eq!(reply["result"], true, "{reply}");
    let list = rpc(ws, inbox, 9, "automations.list", json!(null)).await;
    assert_eq!(list["result"], json!([]));
    assert!(!std::fs::read_to_string(&config_path).unwrap().contains("follow"));
}

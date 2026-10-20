//! Installing from a registry end to end: a local `index.json` and `.wasm` over HTTP, the mock
//! plugin signed by a test author, and the JSON-RPC methods the app uses.

mod common;

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{Router, extract::State, routing::get};
use common::{build_plugin, hub, wait_for};
use futures_util::{SinkExt, StreamExt};
use homemaid::{api, config, model::entity_uuid};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// What the fake registry serves: the index, and the `.wasm` its versions point to.
#[derive(Clone, Default)]
struct Served {
    index: Arc<Mutex<String>>,
    wasm: Arc<Mutex<Vec<u8>>>,
}

async fn registry(served: Served) -> SocketAddr {
    let app = Router::new()
        .route("/index.json", get(|State(s): State<Served>| async move { s.index.lock().unwrap().clone() }))
        .route("/mock.wasm", get(|State(s): State<Served>| async move { s.wasm.lock().unwrap().clone() }))
        .with_state(served);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

struct Author {
    key: String,
    sk: minisign::SecretKey,
}

impl Author {
    fn new() -> Self {
        let minisign::KeyPair { pk, sk } = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        Self { key: pk.to_base64(), sk }
    }

    /// The index with one version of the mock, signed by this author, under `key`.
    fn index(&self, key: &str, version: &str, wasm: &[u8], at: SocketAddr) -> String {
        let signature = minisign::sign(None, &self.sk, wasm, None, None).unwrap().into_string();
        let sha256: String = Sha256::digest(wasm).iter().map(|b| format!("{b:02x}")).collect();
        json!({ "format": 1, "plugins": [{
            "id": "mock", "name": "Mock", "description": "A lamp that is not there", "key": key,
            "settings": [{ "key": "token", "label": "Token", "secret": true, "required": true }],
            "versions": [{ "version": version, "api": "1.2", "permissions": ["local-network"],
                "url": format!("http://{at}/mock.wasm"), "sha256": sha256, "signature": signature }],
        }]})
        .to_string()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_is_installed_checked_updated_and_removed() {
    let wasm = std::fs::read(build_plugin("mock-plugin")).unwrap();
    let served = Served::default();
    let at = registry(served.clone()).await;
    let author = Author::new();
    *served.index.lock().unwrap() = author.index(&author.key, "0.1.0", &wasm, at);

    let (core, dir) = hub();
    let config_path = dir.0.join("homemaid.toml");
    let text = format!("[core]\nbind = \"127.0.0.1:0\"\ndata_dir = \"data\"\ntoken = \"0123456789abcdef\"\nregistry = \"http://{at}/index.json\"\n");
    std::fs::write(&config_path, text).unwrap();
    let app = api::router(core.clone(), &config::load(&config_path).unwrap().core.unwrap(), &config_path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws?token=0123456789abcdef")).await.unwrap();
    let mut next_id = 0;
    let mut call = async |method: &str, params: Value| {
        next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": next_id, "method": method, "params": params });
        ws.send(Message::Text(request.to_string().into())).await.unwrap();
        loop {
            let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("closed") };
            let message: Value = serde_json::from_str(&text).unwrap();
            if message["id"] == next_id {
                return message;
            }
        }
    };
    let error = |reply: &Value| reply["error"]["message"].as_str().unwrap_or_else(|| panic!("an error: {reply}")).to_string();
    let install = |version: &str, permissions: &[&str], settings: Value| {
        json!({ "id": "mock", "version": version, "permissions": permissions, "settings": settings })
    };

    let offered = call("registry.list", json!(null)).await;
    assert_eq!(offered["result"][0]["version"], "0.1.0", "{offered}");
    assert_eq!(offered["result"][0]["installed"], Value::Null);

    // Refused before anything is downloaded or written.
    let reply = call("plugin.install", install("0.1.0", &[], json!({ "token": "t" }))).await;
    assert!(error(&reply).contains("local-network"), "{reply}");
    let reply = call("plugin.install", install("0.1.0", &["local-network"], json!({}))).await;
    assert!(error(&reply).contains("Token is required"), "{reply}");
    *served.wasm.lock().unwrap() = b"\0asm not the signed one".to_vec();
    let reply = call("plugin.install", install("0.1.0", &["local-network"], json!({ "token": "t" }))).await;
    assert!(error(&reply).contains("checksum"), "{reply}");
    assert!(config::load(&config_path).unwrap().plugins.is_empty());

    *served.wasm.lock().unwrap() = wasm.clone();
    let reply = call("plugin.install", install("0.1.0", &["local-network"], json!({ "token": "s3cret" }))).await;
    assert_eq!(reply["result"], true, "{reply}");
    let lamp = entity_uuid("mock", "lamp");
    let available = || core.with(|r| r.entities.get(&lamp).is_some_and(|e| e.available));
    wait_for("the installed plugin's lamp", Duration::from_secs(10), available).await;
    let listed = call("plugins.list", json!(null)).await;
    let mock = &listed["result"][0];
    assert_eq!((&mock["source"], &mock["version"], &mock["name"]), (&json!("registry"), &json!("0.1.0"), &json!("Mock")), "{listed}");
    assert_eq!(mock["secrets"], json!(["token"]));
    assert!(!listed.to_string().contains("s3cret"), "secret values never go back out");

    // The index now names another key: refused, even signed by it.
    let usurper = Author::new();
    *served.index.lock().unwrap() = usurper.index(&usurper.key, "0.2.0", &wasm, at);
    let reply = call("plugin.install", install("0.2.0", &["local-network"], json!({}))).await;
    assert!(error(&reply).contains("another key"), "{reply}");

    // A real update: same key, settings kept, same entity.
    *served.index.lock().unwrap() = author.index(&author.key, "0.2.0", &wasm, at);
    let offered = call("registry.list", json!(null)).await;
    assert_eq!((&offered["result"][0]["installed"], &offered["result"][0]["update"]), (&json!("0.1.0"), &json!(true)));
    let reply = call("plugin.install", install("0.2.0", &["local-network"], json!({}))).await;
    assert_eq!(reply["result"], true, "{reply}");
    let installed = config::load(&config_path).unwrap().plugins;
    assert_eq!(installed[0].version.as_deref(), Some("0.2.0"));
    assert_eq!(config::load_secrets(&dir.0.join("data/secrets.toml")).unwrap()["mock"]["token"], "s3cret");
    wait_for("the lamp back after the update", Duration::from_secs(10), available).await;

    let reply = call("plugin.remove", json!({ "id": "mock" })).await;
    assert_eq!(reply["result"], true, "{reply}");
    assert!(core.with(|r| r.entities.is_empty()));
    assert!(config::load(&config_path).unwrap().plugins.is_empty());
    assert!(!dir.0.join("data/plugins/mock.wasm").exists());
}

//! The agent on the fallback desktop (non-Windows): the phone's direct WebSocket channel.
// Fallback desktop only: on Windows these would move the real mouse and change the real volume.
#![cfg(not(windows))]

use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf};

use futures_util::{SinkExt, StreamExt};
use homemaid::{
    agent::{self, Media, Request},
    config::AgentConfig,
};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Keep the sender: dropping it stops the remote sessions.
async fn start_agent() -> (SocketAddr, watch::Sender<()>) {
    let config = AgentConfig {
        enabled: true,
        bind: "127.0.0.1:0".parse().unwrap(),
        token: "secret".into(),
        apps: BTreeMap::from([("true".to_string(), PathBuf::from("true"))]),
        shortcuts: vec![],
        discover_apps: false,
    };
    let (stopping, _) = watch::channel(());
    let app = agent::router(&config, &stopping, None).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (addr, stopping)
}

#[tokio::test]
async fn remote_channel_drives_the_desktop() {
    let (addr, _stopping) = start_agent().await;
    assert!(connect_async(format!("ws://{addr}/ws")).await.is_err(), "no token, no remote");

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws?token=secret")).await.unwrap();
    let mut next = async || -> Value {
        let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("connection closed") };
        serde_json::from_str(&text).unwrap()
    };
    let hello = next().await;
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["apps"], json!(["true"]));

    let mut ws = ws;
    for request in [
        json!({ "type": "move", "dx": 3.0, "dy": -2.0 }),
        json!({ "type": "text", "text": "salut" }),
        json!({ "type": "media", "action": "play_pause" }),
        json!({ "type": "launch", "app": "true" }),
        json!({ "type": "launch", "app": "rm" }),
        json!({ "type": "set_volume", "level": 150 }),
        json!({ "type": "state" }),
    ] {
        ws.send(Message::Text(request.to_string().into())).await.unwrap();
    }
    let mut replies = Vec::new();
    while let Some(Ok(Message::Text(text))) = ws.next().await {
        let reply: Value = serde_json::from_str(&text).unwrap();
        let done = reply["type"] == "state";
        replies.push(reply);
        if done {
            break;
        }
    }
    // Requests run in order: only the refused ones and the state answer come back.
    assert_eq!(replies.len(), 3, "{replies:?}");
    assert!(replies[0]["message"].as_str().unwrap().contains("not in [agent.apps]"));
    assert!(replies[1]["message"].as_str().unwrap().contains("0-100"));
    assert!(replies[2]["state"]["memory"].as_f64().unwrap() > 0.0);

    let recorded = agent::recorded();
    assert!(recorded.contains(&Request::Move { dx: 3.0, dy: -2.0 }));
    assert!(recorded.contains(&Request::Text { text: "salut".into() }));
    assert!(recorded.contains(&Request::Media { action: Media::PlayPause }));
    assert!(!recorded.contains(&Request::SetVolume { level: 150 }));
}

#[tokio::test]
async fn test_page_is_served() {
    let (addr, _stopping) = start_agent().await;
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(b"GET /remote HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await.unwrap();
    let mut page = String::new();
    stream.read_to_string(&mut page).await.unwrap();
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("Trackpad"));
}

#[tokio::test]
async fn switching_the_agent_off_ends_live_sessions() {
    let (addr, stopping) = start_agent().await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws?token=secret")).await.unwrap();
    assert!(matches!(ws.next().await, Some(Ok(Message::Text(_)))), "hello");
    drop(stopping);
    let end = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.expect("the session must end");
    assert!(!matches!(end, Some(Ok(Message::Text(_)))), "{end:?}");
}

#[tokio::test]
async fn the_phone_adds_sites_never_programs_and_the_agent_restarts() {
    let path = std::env::temp_dir().join(format!("homemaid-agent-manage-{}.toml", std::process::id()));
    std::fs::write(&path, "# mine\n[agent]\nbind = \"127.0.0.1:0\"\ntoken = \"secret0123456789\"\n").unwrap();
    let config = homemaid::config::load(&path).unwrap().agent.unwrap();
    let restart = std::sync::Arc::new(tokio::sync::Notify::new());
    let manage = agent::Manage { config_path: path.clone(), restart: restart.clone() };
    let (stopping, _) = watch::channel(());
    let app = agent::router(&config, &stopping, Some(manage)).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws?token=secret0123456789")).await.unwrap();
    let hello = ws.next().await;
    assert!(matches!(hello, Some(Ok(Message::Text(_)))));
    let mut ask = async |request: Value| -> Value {
        ws.send(Message::Text(request.to_string().into())).await.unwrap();
        let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("closed") };
        serde_json::from_str(&text).unwrap()
    };

    let site = json!({ "type": "save_shortcut", "shortcut": { "name": "Plex", "icon": "plex", "open": "https://app.plex.tv", "keys": { "skip_intro": "s" } } });
    assert_eq!(ask(site).await["type"], "saved");
    let program = json!({ "type": "save_shortcut", "shortcut": { "name": "Evil", "open": "C:\\Windows\\System32\\cmd.exe" } });
    assert!(ask(program).await["message"].as_str().unwrap().contains("web addresses"));
    let unknown = json!({ "type": "pin_app", "name": "cmd" });
    assert!(ask(unknown).await["message"].as_str().unwrap().contains("Start menu"), "only an app the agent found");
    assert_eq!(ask(json!({ "type": "list_apps" })).await, json!({ "type": "apps", "apps": [] }));

    let shortcuts = homemaid::config::load(&path).unwrap().agent.unwrap().shortcuts;
    assert_eq!((shortcuts.len(), shortcuts[0].name.as_str()), (1, "Plex"));
    assert!(std::fs::read_to_string(&path).unwrap().starts_with("# mine\n"));
    tokio::time::timeout(std::time::Duration::from_secs(2), restart.notified()).await.expect("the agent restarts to read it");

    assert_eq!(ask(json!({ "type": "delete_shortcut", "name": "Plex" })).await["type"], "saved");
    let left = homemaid::config::load(&path).unwrap().agent.unwrap().shortcuts;
    std::fs::remove_file(&path).unwrap();
    assert!(left.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_change_the_agent_restarts_with_it() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let path = std::env::temp_dir().join(format!("homemaid-agent-restart-{}.toml", std::process::id()));
    std::fs::write(&path, format!("[agent]\nbind = \"127.0.0.1:{port}\"\ntoken = \"secret0123456789\"\n")).unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(homemaid::server::run(path.clone(), false, Some(homemaid::server::always_on()), async {
        let _ = stopped.await;
    }));
    let url = format!("ws://127.0.0.1:{port}/ws?token=secret0123456789");
    let connect = async || loop {
        if let Ok((ws, _)) = connect_async(&url).await {
            return ws;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    let hello = async |ws: &mut tokio_tungstenite::WebSocketStream<_>| -> Value {
        let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("no hello") };
        serde_json::from_str(&text).unwrap()
    };

    let mut ws = connect().await;
    assert_eq!(hello(&mut ws).await["shortcuts"], json!([]));
    let site = json!({ "type": "save_shortcut", "shortcut": { "name": "Plex", "open": "https://app.plex.tv" } });
    ws.send(Message::Text(site.to_string().into())).await.unwrap();
    // The answer, then the session ends with the restart.
    while let Some(Ok(_)) = ws.next().await {}

    let mut ws = connect().await;
    assert_eq!(hello(&mut ws).await["shortcuts"][0]["name"], "Plex", "read again at restart");
    let _ = stop.send(());
    server.await.unwrap().unwrap();
    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn icons_are_for_known_apps_and_the_token_holder() {
    let (addr, _stopping) = start_agent().await;
    let get = async |auth: &str| {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!("GET /icon?app=..%5C..%5Cwindows%5Cwin.ini HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.unwrap();
        answer
    };
    assert!(get("").await.starts_with("HTTP/1.1 401"));
    assert!(get("Authorization: Bearer secret\r\n").await.starts_with("HTTP/1.1 404"), "not an app it found: nothing");
}

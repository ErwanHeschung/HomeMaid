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
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn start_agent() -> SocketAddr {
    let config = AgentConfig {
        enabled: true,
        bind: "127.0.0.1:0".parse().unwrap(),
        token: "secret".into(),
        apps: BTreeMap::from([("true".to_string(), PathBuf::from("true"))]),
        shortcuts: vec![],
    };
    let app = agent::router(&config).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

#[tokio::test]
async fn remote_channel_drives_the_desktop() {
    let addr = start_agent().await;
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
    let addr = start_agent().await;
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(b"GET /remote HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await.unwrap();
    let mut page = String::new();
    stream.read_to_string(&mut page).await.unwrap();
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("Trackpad"));
}

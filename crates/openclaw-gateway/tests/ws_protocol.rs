//! End-to-end wire protocol test: real listener, real WebSocket client.
//!
//! Guards the handshake and response envelope shape (every `res` frame must
//! carry `type: "res"`), which is easy to break when serialising payloads
//! directly instead of through `GatewayFrame`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use openclaw_gateway::config::{AuthConfig, AuthMode, Config, ModelConfig};
use openclaw_gateway::{build_state, router};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

fn test_state_dir() -> PathBuf {
    std::env::temp_dir().join(format!("openclaw-rust-it-{}", uuid::Uuid::new_v4()))
}

fn test_config() -> Config {
    let state_dir = test_state_dir();
    Config {
        bind: "127.0.0.1".to_string(),
        port: 0,
        auth: AuthConfig {
            mode: AuthMode::Token,
            token: Some("test-token".to_string()),
            password: None,
        },
        db_path: state_dir.join("state.db"),
        state_dir,
        model: ModelConfig {
            base_url: "http://127.0.0.1:1/v1".to_string(),
            api_key: None,
            model: "test-model".to_string(),
            system_prompt: "test".to_string(),
            max_history: 10,
        },
        max_payload: 1024 * 1024,
    }
}

async fn start_gateway() -> String {
    let state = build_state(test_config()).expect("state");
    let app = router(Arc::clone(&state));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("ws://{address}/")
}

type Socket = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn next_frame(socket: &mut Socket) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("frame timeout")
            .expect("socket closed")
            .expect("socket error");
        if let Message::Text(text) = message {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["event"] == "tick" {
                continue;
            }
            return value;
        }
    }
}

/// Reads until the response for `id` arrives, skipping interleaved events.
async fn next_response(socket: &mut Socket, id: &str) -> Value {
    loop {
        let frame = next_frame(socket).await;
        if frame["type"] == "res" && frame["id"] == id {
            return frame;
        }
    }
}

async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("send frame");
}

#[tokio::test]
async fn connect_and_chat_roundtrip() {
    let url = start_gateway().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.expect("connect");

    send(
        &mut socket,
        json!({
            "type": "req",
            "id": "connect-1",
            "method": "connect",
            "params": {
                "minProtocol": 4,
                "maxProtocol": 4,
                "client": { "id": "it-cli", "version": "0.0.0", "platform": "test", "mode": "cli" },
                "auth": { "token": "test-token" }
            }
        }),
    )
    .await;

    let hello = next_response(&mut socket, "connect-1").await;
    assert_eq!(hello["type"], "res");
    assert_eq!(hello["id"], "connect-1");
    assert_eq!(hello["ok"], true);
    assert_eq!(hello["payload"]["type"], "hello-ok");
    assert_eq!(hello["payload"]["protocol"], 4);
    assert!(hello["payload"]["features"]["methods"]
        .as_array()
        .unwrap()
        .contains(&json!("sessions.create")));

    send(
        &mut socket,
        json!({
            "type": "req",
            "id": "create-1",
            "method": "sessions.create",
            "params": { "key": "agent:main:it", "label": "IT" }
        }),
    )
    .await;

    let created = next_response(&mut socket, "create-1").await;
    assert_eq!(created["type"], "res");
    assert_eq!(created["id"], "create-1");
    assert_eq!(created["ok"], true);
    assert_eq!(created["payload"]["key"], "agent:main:it");

    send(
        &mut socket,
        json!({
            "type": "req",
            "id": "send-1",
            "method": "chat.send",
            "params": { "sessionKey": "agent:main:it", "message": "ping" }
        }),
    )
    .await;

    let accepted = next_response(&mut socket, "send-1").await;
    assert_eq!(accepted["type"], "res");
    assert_eq!(accepted["id"], "send-1");
    assert_eq!(accepted["payload"]["accepted"], true);

    let final_text = loop {
        let frame = next_frame(&mut socket).await;
        if frame["type"] == "event" && frame["event"] == "chat" && frame["payload"]["kind"] == "final"
        {
            break frame["payload"]["text"].as_str().unwrap().to_string();
        }
    };
    assert!(
        final_text.contains("ping"),
        "echo transport should quote the prompt: {final_text}"
    );

    let _ = socket.close(None).await;
}

#[tokio::test]
async fn rejects_bad_token() {
    let url = start_gateway().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.expect("connect");

    send(
        &mut socket,
        json!({
            "type": "req",
            "id": "connect-1",
            "method": "connect",
            "params": {
                "minProtocol": 4,
                "maxProtocol": 4,
                "client": { "id": "it-cli", "version": "0.0.0", "platform": "test", "mode": "cli" },
                "auth": { "token": "wrong" }
            }
        }),
    )
    .await;

    let response = next_frame(&mut socket).await;
    assert_eq!(response["type"], "res");
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn unknown_method_returns_structured_error() {
    let url = start_gateway().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.expect("connect");

    send(
        &mut socket,
        json!({
            "type": "req",
            "id": "connect-1",
            "method": "connect",
            "params": {
                "minProtocol": 4,
                "maxProtocol": 4,
                "client": { "id": "it-cli", "version": "0.0.0", "platform": "test", "mode": "cli" },
                "auth": { "token": "test-token" }
            }
        }),
    )
    .await;
    let _hello = next_response(&mut socket, "connect-1").await;

    send(
        &mut socket,
        json!({ "type": "req", "id": "nope-1", "method": "does.not.exist" }),
    )
    .await;

    let response = next_frame(&mut socket).await;
    assert_eq!(response["id"], "nope-1");
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "not_found");
}

//! HTTP surface: probes plus an OpenAI-compatible chat endpoint.
//!
//! Mirrors `src/gateway/gateway-http-route-contracts.ts` (probes) and
//! `src/gateway/openai-compatible-http.ts`, limited to what this build serves.

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;

use crate::llm::ChatTurn;
use crate::state::AppState;
use crate::store::{now_ms, Store};

pub async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "ts": now_ms(),
        "uptimeMs": state.uptime_ms(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "ready": true,
        "sessions": state.store.session_count().unwrap_or_default(),
    }))
}

pub async fn models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "object": "list",
        "data": [{
            "id": state.llm.model(),
            "object": "model",
            "created": 0,
            "owned_by": "openclaw",
        }],
    }))
}

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = authorize(&state, &headers) {
        return response;
    }

    let turns = match parse_turns(&body) {
        Ok(turns) => turns,
        Err(message) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "message": message, "type": "invalid_request_error" } })),
            )
                .into_response()
        }
    };

    let session_key = headers
        .get("x-openclaw-session")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("agent:main:openai")
        .to_string();
    if let Err(error) = state.store.ensure_session(&session_key) {
        tracing::warn!(%error, "failed to ensure session for http chat");
    }

    let streaming = body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(state.llm.model())
        .to_string();

    if !streaming {
        let mut text = String::new();
        let result = state
            .llm
            .stream_chat(&turns, |delta| text.push_str(delta))
            .await;
        return match result {
            Ok(content) => {
                persist(&state.store, &session_key, &turns, &content);
                Json(json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
                    "object": "chat.completion",
                    "created": (now_ms() / 1000.0) as u64,
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": content },
                        "finish_reason": "stop",
                    }],
                }))
                .into_response()
            }
            Err(error) => provider_error(error.to_string()),
        };
    }

    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let state_for_task = state.clone();
    let model_for_task = model.clone();
    tokio::spawn(async move {
        let mut accumulated = String::new();
        let send = |tx: &mpsc::UnboundedSender<String>, value: Value| {
            let _ = tx.send(format!("data: {value}\n\n"));
        };

        send(
            &tx,
            json!({
                "id": "chatcmpl-stream",
                "object": "chat.completion.chunk",
                "model": model_for_task,
                "choices": [{ "index": 0, "delta": { "role": "assistant" }, "finish_reason": null }],
            }),
        );

        let result = state_for_task
            .llm
            .stream_chat(&turns, |delta| {
                accumulated.push_str(delta);
                let _ = tx.send(format!(
                    "data: {}\n\n",
                    json!({
                        "id": "chatcmpl-stream",
                        "object": "chat.completion.chunk",
                        "model": model_for_task,
                        "choices": [{ "index": 0, "delta": { "content": delta }, "finish_reason": null }],
                    })
                ));
            })
            .await;

        match result {
            Ok(content) => {
                persist(&state_for_task.store, &session_key, &turns, &content);
                send(
                    &tx,
                    json!({
                        "id": "chatcmpl-stream",
                        "object": "chat.completion.chunk",
                        "model": model_for_task,
                        "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }],
                    }),
                );
            }
            Err(error) => {
                let _ = tx.send(format!(
                    "data: {}\n\n",
                    json!({ "error": { "message": error.to_string(), "type": "provider_error" } })
                ));
            }
        }
        let _ = tx.send("data: [DONE]\n\n".to_string());
    });

    let body_stream = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|chunk| (Ok::<_, Infallible>(chunk), rx))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(body_stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[allow(clippy::result_large_err)]
fn authorize(state: &Arc<AppState>, headers: &HeaderMap) -> Result<(), Response> {
    let expected = match state.config.auth.mode {
        crate::config::AuthMode::Token => state.config.auth.token.as_deref(),
        _ => None,
    };
    let Some(expected) = expected else {
        return Ok(());
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match presented {
        Some(token) if constant_time_eq(token, expected) => Ok(()),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": { "message": "invalid api key", "type": "authentication_error" } })),
        )
            .into_response()),
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

fn parse_turns(body: &Value) -> Result<Vec<ChatTurn>, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "messages is required".to_string())?;
    if messages.is_empty() {
        return Err("messages must not be empty".to_string());
    }
    Ok(messages
        .iter()
        .filter_map(|message| {
            let role = message.get("role")?.as_str()?.to_string();
            let content = match message.get("content") {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => return None,
            };
            Some(ChatTurn { role, content })
        })
        .collect())
}

fn persist(store: &Store, session_key: &str, turns: &[ChatTurn], reply: &str) {
    if let Some(user) = turns.iter().rev().find(|t| t.role == "user") {
        let _ = store.append_message(session_key, openclaw_protocol::MessageRole::User, &user.content);
    }
    let _ = store.append_message(
        session_key,
        openclaw_protocol::MessageRole::Assistant,
        reply,
    );
}

fn provider_error(message: String) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({ "error": { "message": message, "type": "provider_error" } })),
    )
        .into_response()
}

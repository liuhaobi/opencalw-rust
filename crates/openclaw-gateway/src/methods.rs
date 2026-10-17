//! RPC method dispatch.
//!
//! Mirrors the method table in `src/gateway/methods/core-descriptors.ts`.
//! Unknown methods return a structured `not_found` error rather than closing
//! the socket, matching the reference implementation.

use std::sync::Arc;

use openclaw_protocol::{
    ChatSendParams, ErrorShape, RequestFrame, ResponseFrame, SessionsCreateParams,
    SessionsHistoryResult, SessionsKeyParams, SessionsListParams, SessionsListResult,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::agent::run_turn;
use crate::auth::AuthGrant;
use crate::store::now_ms;
use crate::state::AppState;

pub async fn dispatch(
    state: Arc<AppState>,
    grant: &AuthGrant,
    frame: &RequestFrame,
) -> ResponseFrame {
    match execute(state, grant, &frame.method, frame.params.clone()).await {
        Ok(payload) => ResponseFrame::ok(frame.id.clone(), payload),
        Err(error) => ResponseFrame::err(frame.id.clone(), error),
    }
}

async fn execute(
    state: Arc<AppState>,
    grant: &AuthGrant,
    method: &str,
    params: Option<Value>,
) -> Result<Value, ErrorShape> {
    match method {
        "health" => Ok(json!({
            "ok": true,
            "ts": now_ms(),
            "uptimeMs": state.uptime_ms(),
            "version": env!("CARGO_PKG_VERSION"),
        })),
        "status" => Ok(json!({
            "ok": true,
            "uptimeMs": state.uptime_ms(),
            "sessions": state.store.session_count().map_err(internal)?,
            "connections": state.events.subscriber_count(),
            "model": {
                "name": state.llm.model(),
                "configured": state.llm.is_configured(),
            },
        })),
        "config.get" => Ok(state.config.public_view()),
        "sessions.list" => {
            require_read(grant)?;
            let params: SessionsListParams = parse(params)?;
            let sessions = state
                .store
                .list_sessions(params.archived.unwrap_or(false), params.limit.unwrap_or(100))
                .map_err(internal)?;
            serde_json::to_value(SessionsListResult { sessions }).map_err(internal)
        }
        "sessions.create" => {
            require_write(grant)?;
            let params: SessionsCreateParams = parse(params)?;
            let row = state
                .store
                .create_session(
                    params.key.as_deref(),
                    params.label.as_deref(),
                    params.kind.unwrap_or_default(),
                    params.agent_id.as_deref(),
                )
                .map_err(internal)?;
            state
                .events
                .publish("sessions.changed", json!({ "key": row.key, "reason": "created" }));
            serde_json::to_value(row).map_err(internal)
        }
        "sessions.get" => {
            require_read(grant)?;
            let key = key_param(params)?;
            let row = state
                .store
                .get_session(&key)
                .map_err(internal)?
                .ok_or_else(|| ErrorShape::not_found(format!("unknown session: {key}")))?;
            serde_json::to_value(row).map_err(internal)
        }
        "sessions.history" => {
            require_read(grant)?;
            let params: SessionsKeyParams = parse(params)?;
            let messages = state
                .store
                .history(&params.key, params.limit.unwrap_or(200))
                .map_err(internal)?;
            serde_json::to_value(SessionsHistoryResult {
                key: params.key,
                messages,
            })
            .map_err(internal)
        }
        "sessions.delete" => {
            require_write(grant)?;
            let key = key_param(params)?;
            let removed = state.store.delete_session(&key).map_err(internal)?;
            if removed {
                state
                    .events
                    .publish("sessions.changed", json!({ "key": key, "reason": "deleted" }));
            }
            Ok(json!({ "ok": removed, "key": key }))
        }
        "chat.send" => {
            require_write(grant)?;
            let params: ChatSendParams = parse(params)?;
            if params.message.trim().is_empty() {
                return Err(ErrorShape::invalid_params("message must not be empty"));
            }
            let session_key = params
                .session_key()
                .map(str::to_string)
                .unwrap_or_else(|| "agent:main:main".to_string());
            state.store.ensure_session(&session_key).map_err(internal)?;

            let run_id = Uuid::new_v4().to_string();
            let abort = state.register_run(&run_id);
            let state_clone = state.clone();
            let run_id_clone = run_id.clone();
            let session_clone = session_key.clone();
            let message = params.message.clone();
            tokio::spawn(async move {
                run_turn(
                    state_clone,
                    run_id_clone,
                    session_clone,
                    message,
                    abort,
                )
                .await;
            });

            Ok(json!({
                "accepted": true,
                "runId": run_id,
                "sessionKey": session_key,
            }))
        }
        "chat.abort" => {
            require_write(grant)?;
            let run_id = params
                .as_ref()
                .and_then(|p| p.get("runId"))
                .and_then(Value::as_str)
                .ok_or_else(|| ErrorShape::invalid_params("runId is required"))?;
            let aborted = state.abort_run(run_id);
            Ok(json!({ "ok": aborted, "runId": run_id }))
        }
        other => Err(ErrorShape::not_found(format!("unknown method: {other}"))),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, ErrorShape> {
    serde_json::from_value(params.unwrap_or(Value::Null))
        .map_err(|e| ErrorShape::invalid_params(e.to_string()))
}

fn key_param(params: Option<Value>) -> Result<String, ErrorShape> {
    let value = params.unwrap_or(Value::Null);
    value
        .get("key")
        .or_else(|| value.get("sessionKey"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ErrorShape::invalid_params("key is required"))
}

fn require_read(grant: &AuthGrant) -> Result<(), ErrorShape> {
    if grant.has_scope(crate::auth::SCOPE_READ) || grant.has_scope(crate::auth::SCOPE_ADMIN) {
        Ok(())
    } else {
        Err(ErrorShape::unauthorized("missing operator.read scope"))
    }
}

fn require_write(grant: &AuthGrant) -> Result<(), ErrorShape> {
    if grant.can_write() {
        Ok(())
    } else {
        Err(ErrorShape::unauthorized("missing operator.write scope"))
    }
}

fn internal(error: impl std::fmt::Display) -> ErrorShape {
    ErrorShape::internal(error.to_string())
}

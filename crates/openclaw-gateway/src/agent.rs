//! Agent turn execution.
//!
//! Ported from `packages/agent-core/src/agent-loop.ts` and
//! `src/agents/embedded-agent-runner/run-orchestrator.ts`, reduced to the
//! single-turn text loop this gateway currently supports: persist the user
//! message, stream the assistant reply, persist it, and broadcast lifecycle
//! events.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use openclaw_protocol::{MessageRole, RunStatus};
use serde_json::json;

use crate::llm::ChatTurn;
use crate::state::AppState;

pub async fn run_turn(
    state: Arc<AppState>,
    run_id: String,
    session_key: String,
    user_text: String,
    abort: Arc<AtomicBool>,
) {
    if let Err(error) = run_turn_inner(
        state.clone(),
        &run_id,
        &session_key,
        &user_text,
        abort.clone(),
    )
    .await
    {
        tracing::warn!(run_id = %run_id, %error, "agent turn failed");
        let _ = state.store.set_status(&session_key, RunStatus::Failed);
        state.events.publish(
            "chat",
            json!({
                "sessionKey": session_key,
                "runId": run_id,
                "kind": "error",
                "error": error.to_string(),
            }),
        );
    }

    state.finish_run(&run_id);
    state
        .events
        .publish("sessions.changed", json!({ "key": session_key, "reason": "run" }));
}

async fn run_turn_inner(
    state: Arc<AppState>,
    run_id: &str,
    session_key: &str,
    user_text: &str,
    abort: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    state.store.set_status(session_key, RunStatus::Running)?;

    let user_message = state
        .store
        .append_message(session_key, MessageRole::User, user_text)?;
    state.events.publish(
        "session.message",
        json!({ "sessionKey": session_key, "message": user_message }),
    );
    state
        .events
        .publish("sessions.changed", json!({ "key": session_key, "reason": "message" }));

    let history = state
        .store
        .history(session_key, state.config.model.max_history as u32)?;
    let turns: Vec<ChatTurn> = history
        .iter()
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| ChatTurn {
            role: match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            }
            .to_string(),
            content: m.content.clone(),
        })
        .collect();

    state.events.publish(
        "chat",
        json!({
            "sessionKey": session_key,
            "runId": run_id,
            "kind": "start",
        }),
    );

    let mut aborted = false;
    let reply = state
        .llm
        .stream_chat(&turns, |delta| {
            if abort.load(Ordering::SeqCst) {
                aborted = true;
                return;
            }
            state.events.publish(
                "chat",
                json!({
                    "sessionKey": session_key,
                    "runId": run_id,
                    "kind": "delta",
                    "delta": delta,
                }),
            );
        })
        .await?;

    if aborted || abort.load(Ordering::SeqCst) {
        state
            .events
            .publish("chat", json!({ "sessionKey": session_key, "runId": run_id, "kind": "aborted" }));
        state.store.set_status(session_key, RunStatus::Killed)?;
        return Ok(());
    }

    let assistant_message =
        state
            .store
            .append_message(session_key, MessageRole::Assistant, &reply)?;
    state.store.set_status(session_key, RunStatus::Done)?;

    state.events.publish(
        "chat",
        json!({
            "sessionKey": session_key,
            "runId": run_id,
            "kind": "final",
            "text": reply,
            "messageId": assistant_message.id,
        }),
    );
    state.events.publish(
        "session.message",
        json!({ "sessionKey": session_key, "message": assistant_message }),
    );
    state
        .events
        .publish("sessions.changed", json!({ "key": session_key, "reason": "message" }));

    Ok(())
}

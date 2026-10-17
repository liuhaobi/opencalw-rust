//! Rust control-plane gateway for OpenClaw.
//!
//! Scope of this build: the frame protocol, connect handshake, token/password
//! authentication, SQLite-backed sessions, streaming chat turns, and an
//! OpenAI-compatible HTTP surface. Channel adapters, plugins, tools and the
//! native companion apps live outside this crate.

pub mod agent;
pub mod auth;
pub mod config;
pub mod events;
pub mod http;
pub mod llm;
pub mod methods;
pub mod state;
pub mod store;
pub mod ws;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::config::Config;
use crate::state::AppState;

/// Builds the gateway application state.
pub fn build_state(config: Config) -> anyhow::Result<Arc<AppState>> {
    AppState::new(config)
}

/// Builds the HTTP/WebSocket router.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(ws::gateway_ws))
        .route("/gateway", get(ws::gateway_ws))
        .route("/health", get(http::health))
        .route("/healthz", get(http::health))
        .route("/ready", get(http::ready))
        .route("/readyz", get(http::ready))
        .route("/startup", get(http::health))
        .route("/startupz", get(http::health))
        .route("/v1/models", get(http::models))
        .route("/v1/chat/completions", post(http::chat_completions))
        .with_state(state)
}

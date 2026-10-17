//! Runtime configuration, resolved from environment variables.

use std::env;
use std::path::PathBuf;

use openclaw_protocol::DEFAULT_GATEWAY_PORT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    None,
    Token,
    Password,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::None => "none",
            AuthMode::Token => "token",
            AuthMode::Password => "password",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub token: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub system_prompt: String,
    pub max_history: usize,
}

impl ModelConfig {
    pub fn is_configured(&self) -> bool {
        self.api_key.as_deref().is_some_and(|k| !k.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub port: u16,
    pub auth: AuthConfig,
    pub state_dir: PathBuf,
    pub db_path: PathBuf,
    pub model: ModelConfig,
    pub max_payload: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("OPENCLAW_RUST_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("OPENCLAW_RUST_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(DEFAULT_GATEWAY_PORT);

        let token = non_empty(env::var("OPENCLAW_RUST_TOKEN").ok());
        let password = non_empty(env::var("OPENCLAW_RUST_PASSWORD").ok());
        let allow_anonymous = env::var("OPENCLAW_RUST_ALLOW_ANONYMOUS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let mode = match (&token, &password) {
            (Some(_), _) => AuthMode::Token,
            (None, Some(_)) => AuthMode::Password,
            (None, None) if allow_anonymous => AuthMode::None,
            (None, None) => {
                anyhow::bail!(
                    "refusing to start without authentication: set OPENCLAW_RUST_TOKEN \
                     (or OPENCLAW_RUST_PASSWORD), or OPENCLAW_RUST_ALLOW_ANONYMOUS=1 \
                     for local-only testing"
                )
            }
        };

        let state_dir = env::var("OPENCLAW_RUST_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_state_dir());
        let db_path = state_dir.join("state.db");

        let base_url = env::var("OPENCLAW_RUST_MODEL_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let api_key = non_empty(
            env::var("OPENCLAW_RUST_MODEL_API_KEY")
                .ok()
                .or_else(|| env::var("OPENAI_API_KEY").ok()),
        );
        let model =
            env::var("OPENCLAW_RUST_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let max_history = env::var("OPENCLAW_RUST_MAX_HISTORY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(30);

        Ok(Self {
            bind,
            port,
            auth: AuthConfig {
                mode,
                token,
                password,
            },
            state_dir,
            db_path,
            model: ModelConfig {
                base_url,
                api_key,
                model,
                system_prompt: env::var("OPENCLAW_RUST_SYSTEM_PROMPT").unwrap_or_else(|_| {
                    "You are OpenClaw, a concise and helpful personal assistant.".to_string()
                }),
                max_history,
            },
            max_payload: env::var("OPENCLAW_RUST_MAX_PAYLOAD")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(4 * 1024 * 1024),
        })
    }

    /// Client-facing view with secrets removed.
    pub fn public_view(&self) -> serde_json::Value {
        serde_json::json!({
            "bind": self.bind,
            "port": self.port,
            "authMode": self.auth.mode.as_str(),
            "stateDir": self.state_dir.to_string_lossy(),
            "model": {
                "baseUrl": self.model.base_url,
                "model": self.model.model,
                "configured": self.model.is_configured(),
            },
            "maxPayload": self.max_payload,
        })
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn default_state_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".openclaw-rust")
}

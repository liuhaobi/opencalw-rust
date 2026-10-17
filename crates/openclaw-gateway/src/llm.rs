//! OpenAI-compatible streaming client.
//!
//! The TypeScript implementation abstracts providers behind `StreamFn`
//! (`packages/llm-core`). This port keeps the same shape at a smaller scale:
//! one `chat/completions` transport that works with any OpenAI-compatible
//! endpoint, plus a deterministic local echo transport used when no API key is
//! configured (so the gateway is usable out of the box).

use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::config::ModelConfig;

#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

pub struct LlmClient {
    http: reqwest::Client,
    config: ModelConfig,
}

impl LlmClient {
    pub fn new(config: ModelConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("openclaw-rust/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(300))
            .build()?;
        Ok(Self { http, config })
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub fn is_configured(&self) -> bool {
        self.config.is_configured()
    }

    /// Streams assistant text, invoking `on_delta` for every chunk.
    /// Returns the accumulated text.
    pub async fn stream_chat<F>(&self, messages: &[ChatTurn], mut on_delta: F) -> anyhow::Result<String>
    where
        F: FnMut(&str),
    {
        if !self.is_configured() {
            return self.echo_stream(messages, on_delta).await;
        }

        let mut payload = json!({
            "model": self.config.model,
            "stream": true,
            "messages": messages
                .iter()
                .map(|m| json!({ "role": m.role, "content": m.content }))
                .collect::<Vec<_>>(),
        });
        payload["messages"]
            .as_array_mut()
            .expect("messages is an array")
            .insert(0, json!({ "role": "system", "content": self.config.system_prompt }));

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let response = self
            .http
            .post(&url)
            .bearer_auth(self.config.api_key.clone().unwrap_or_default())
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("model provider returned {status}: {}", truncate(&body, 500));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut accumulated = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim_end_matches('\r').to_string();
                buffer.drain(..=index);

                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                if let Some(delta) = value["choices"][0]["delta"]["content"].as_str() {
                    if !delta.is_empty() {
                        accumulated.push_str(delta);
                        on_delta(delta);
                    }
                }
            }
        }

        Ok(accumulated)
    }

    async fn echo_stream<F>(
        &self,
        messages: &[ChatTurn],
        mut on_delta: F,
    ) -> anyhow::Result<String>
    where
        F: FnMut(&str),
    {
        let last_user = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or_default();
        let reply = format!(
            "This gateway is running without a model API key, so this is a local echo reply. \
             You said: {last_user}"
        );

        let chars: Vec<char> = reply.chars().collect();
        for chunk in chars.chunks(12) {
            let piece: String = chunk.iter().collect();
            on_delta(&piece);
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
        Ok(reply)
    }
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

//! OpenAI-compatible streaming model client.
//!
//! Works against any endpoint that speaks the OpenAI chat completions API:
//!   - llama-server (local)
//!   - LiteLLM proxy
//!   - OpenRouter
//!   - Anthropic (via their OpenAI-compatible endpoint)
//!
//! Configuration via environment variables:
//!   OPENAI_BASE_URL   — e.g. http://localhost:8080/v1  (default: https://api.openai.com/v1)
//!   OPENAI_API_KEY    — bearer token (required for hosted endpoints, any value for local)

use std::time::Instant;

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;
use tracing::{debug, instrument, warn};

use mswea_core::{message::Message, ToolCall};

use crate::extract::extract_tool_call;
use crate::sse::ChatCompletionChunk;

/// A typed API error carrying the HTTP status code.
/// Used by the retry policy to distinguish permanent (4xx) from transient (5xx/network) failures.
#[derive(Debug, thiserror::Error)]
#[error("API error {status}: {body}")]
pub struct ApiError {
    pub status: u16,
    pub body: String,
}

/// Returns true if this error is worth retrying.
pub fn is_retryable(e: &anyhow::Error) -> bool {
    if let Some(api_err) = e.downcast_ref::<ApiError>() {
        // 4xx = permanent client error, don't retry.
        // 5xx / network errors = transient, do retry.
        api_err.status >= 500
    } else {
        // Network / IO errors — retry.
        true
    }
}

/// A completed model turn.
#[derive(Debug)]
pub struct ModelResponse {
    pub tool_call: ToolCall,
    /// Full raw text the model produced (reasoning + JSON).
    pub raw_text: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// USD cost as reported by the endpoint; 0.0 if unavailable.
    pub cost_usd: f64,
    pub latency_ms: u64,
}

/// Stateless OpenAI-compatible streaming client.
/// Construct once and reuse across requests.
pub struct LitellmClient {
    http: Client,
    base_url: String,
    api_key: String,
    model_name: String,
}

impl LitellmClient {
    /// Build from environment variables and a model name from `ModelConfig`.
    pub fn from_env(model_name: impl Into<String>) -> Result<Self> {
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());
        let api_key = std::env::var("OPENAI_API_KEY")
            .unwrap_or_else(|_| {
                warn!("OPENAI_API_KEY not set — using placeholder (fine for local endpoints)");
                "sk-placeholder".into()
            });

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Building HTTP client")?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model_name: model_name.into(),
        })
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Send a conversation and stream back a complete `ModelResponse`.
    ///
    /// Accumulates all text deltas, then extracts a `ToolCall` from the
    /// accumulated content. Returns an error if no valid `ToolCall` is found.
    #[instrument(skip(self, messages), fields(model = %self.model_name, msg_count = messages.len()))]
    pub async fn complete(&self, messages: &[Message]) -> Result<ModelResponse> {
        let started = Instant::now();

        let wire_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    mswea_core::message::Role::System    => "system",
                    mswea_core::message::Role::User      => "user",
                    mswea_core::message::Role::Assistant => "assistant",
                    mswea_core::message::Role::Tool      => "user", // fallback
                };
                json!({ "role": role, "content": m.content })
            })
            .collect();

        let body = json!({
            "model": self.model_name,
            "messages": wire_messages,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        debug!(url = %format!("{}/chat/completions", self.base_url), "Sending request");

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("HTTP request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // Preserve the numeric status so the retry policy can skip 4xx.
            bail!(ApiError { status: status.as_u16(), body });
        }

        // ── Stream accumulation ──────────────────────────────────────────────
        let mut text = String::new();
        let mut tokens_in: u32 = 0;
        let mut tokens_out: u32 = 0;
        let mut cost_usd: f64 = 0.0;

        let mut stream = resp.bytes_stream();

        // SSE streams arrive as raw bytes. We buffer across chunk boundaries
        // since a single HTTP chunk may contain multiple SSE lines, or a
        // single SSE line may be split across HTTP chunks.
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("Stream read error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            // Process all complete lines in the buffer.
            while let Some(newline) = buf.find('\n') {
                let line = buf[..newline].trim().to_string();
                buf = buf[newline + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }

                let Some(json_str) = line.strip_prefix("data: ") else {
                    continue;
                };

                match serde_json::from_str::<ChatCompletionChunk>(json_str) {
                    Ok(chunk) => {
                        // Accumulate text deltas.
                        for choice in &chunk.choices {
                            if let Some(content) = &choice.delta.content {
                                text.push_str(content);
                            }
                        }
                        // Capture usage from the final chunk.
                        if let Some(usage) = chunk.usage {
                            tokens_in = usage.prompt_tokens;
                            tokens_out = usage.completion_tokens;
                            if let Some(c) = usage.cost {
                                cost_usd = c;
                            }
                        }
                    }
                    Err(e) => {
                        debug!(line = %json_str, error = %e, "Skipping unparseable SSE line");
                    }
                }
            }
        }

        let latency_ms = started.elapsed().as_millis() as u64;
        debug!(text = %text, "Model output accumulated");

        // ── Tool call extraction ─────────────────────────────────────────────
        let tool_call = extract_tool_call(&text)
            .with_context(|| format!("No ToolCall found in model output: {text:?}"))?;

        Ok(ModelResponse {
            tool_call,
            raw_text: text,
            tokens_in,
            tokens_out,
            cost_usd,
            latency_ms,
        })
    }
}

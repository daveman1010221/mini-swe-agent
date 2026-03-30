//! OpenAI-compatible streaming response types.
//!
//! These are the wire types for the SSE stream from any OpenAI-compatible
//! endpoint — LiteLLM proxy, llama-server, OpenRouter, or Anthropic via proxy.
//! Only the fields we actually use are deserialized; everything else is ignored.

use serde::Deserialize;

/// A single `data: {...}` line from the SSE stream.
#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    pub choices: Vec<ChunkChoice>,
    /// Present in the final chunk when the endpoint supports it.
    #[serde(default)]
    pub usage: Option<UsageStats>,
}

#[derive(Debug, Deserialize)]
pub struct ChunkChoice {
    pub delta: Delta,
    /// "stop", "length", "tool_calls", or null mid-stream.
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Delta {
    /// Text content fragment — may be absent in non-content chunks.
    #[serde(default)]
    pub content: Option<String>,
}

/// Token usage — present in the final chunk from most endpoints.
/// llama-server may omit this; all fields default to 0.
#[derive(Debug, Default, Deserialize)]
pub struct UsageStats {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Some endpoints (LiteLLM with cost tracking) include this.
    #[serde(default)]
    pub cost: Option<f64>,
}

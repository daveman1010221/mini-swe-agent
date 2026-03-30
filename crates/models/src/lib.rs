//! `models` — LLM client and actor for mini-swe-agent.
//!
//! Public surface:
//!   - `LitellmClient`  — stateless OpenAI-compatible streaming client
//!   - `ModelActor`     — retry-aware wrapper with event emission
//!   - `ModelRequest`   — what the agent loop sends
//!   - `ModelReply`     — what the actor returns
//!   - `ModelResponse`  — the raw parsed response from the client

pub mod actor;
pub mod client;
pub mod extract;
pub mod sse;

pub use actor::{EventSink, ModelActor, ModelReply, ModelRequest};
pub use client::{ApiError, is_retryable, LitellmClient, ModelResponse};
pub use extract::{extract_tool_call, ExtractionError};

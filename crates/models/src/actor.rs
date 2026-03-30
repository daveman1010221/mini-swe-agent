//! `ModelActor` — ractor actor wrapping `LitellmClient`.
//!
//! Handles:
//!   - Retry via `backon` exponential backoff (up to 5 attempts)
//!   - Event emission: `ModelRequestStarted`, `ModelResponseReceived`,
//!     `ModelRetried`, `ModelFailed`
//!   - Maps `anyhow::Error` → `AgentError::ModelError` for the agent loop

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use backon::{ExponentialBuilder, Retryable};
use tracing::{error, info, warn};
use ulid::Ulid;

use mswea_core::{
    event::{Event, EventKind},
    message::Message,
    AgentError,
};

use crate::client::{is_retryable, LitellmClient, ModelResponse};

/// Sent by the agent loop to the model actor.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub messages: Vec<Message>,
    pub correlation_id: String,
}

impl ModelRequest {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            correlation_id: Ulid::new().to_string(),
        }
    }
}

/// Returned by the model actor to the agent loop.
#[derive(Debug)]
pub struct ModelReply {
    pub response: ModelResponse,
    pub correlation_id: String,
}

/// Callback the actor uses to emit events to the bus.
/// In the full system this will be an `OutputPort<Event>`; for now it's a
/// simple closure so the actor compiles without ractor wiring.
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// The model actor — owns the client and handles retry logic.
pub struct ModelActor {
    client: Arc<LitellmClient>,
    actor_id: String,
    event_sink: Option<EventSink>,
}

impl ModelActor {
    /// Construct with a pre-built client.
    pub fn new(client: LitellmClient, actor_id: impl Into<String>) -> Self {
        Self {
            client: Arc::new(client),
            actor_id: actor_id.into(),
            event_sink: None,
        }
    }

    /// Attach an event sink (called before the actor starts handling messages).
    pub fn with_event_sink(mut self, sink: EventSink) -> Self {
        self.event_sink = Some(sink);
        self
    }

    fn emit(&self, kind: EventKind, correlation_id: Option<&str>) {
        if let Some(sink) = &self.event_sink {
            let mut event = Event::new(&self.actor_id, kind);
            if let Some(cid) = correlation_id {
                event = event.with_correlation(cid);
            }
            sink(event);
        }
    }

    /// Handle a `ModelRequest`, returning a `ModelReply` or an `AgentError`.
    ///
    /// Retries up to 5 times with exponential backoff on transient failures.
    /// 4xx errors (auth, bad request) are NOT retried.
    pub async fn handle(&self, req: ModelRequest) -> Result<ModelReply, AgentError> {
        let cid = req.correlation_id.clone();
        let started = Instant::now();

        self.emit(
            EventKind::ModelRequestStarted {
                model: self.client_model_name(),
                message_count: req.messages.len(),
            },
            Some(&cid),
        );

        let client = self.client.clone();
        let messages = req.messages.clone();
        let actor_id = self.actor_id.clone();
        let event_sink = self.event_sink.clone();
        let cid_clone = cid.clone();

        let mut attempt = 0u32;

        let result = (|| {
            let client = client.clone();
            let messages = messages.clone();
            async move { client.complete(&messages).await }
        })
        .retry(
            ExponentialBuilder::default()
                .with_max_times(5)
                .with_min_delay(std::time::Duration::from_millis(500))
                .with_max_delay(std::time::Duration::from_secs(30)),
        )
        .when(is_retryable)
        .notify(|err, backoff| {
            attempt += 1;
            let backoff_ms = backoff.as_millis() as u64;
            warn!(
                attempt,
                backoff_ms,
                error = %err,
                "Model request failed, retrying"
            );
            if let Some(sink) = &event_sink {
                let event = Event::new(&actor_id, EventKind::ModelRetried {
                    attempt,
                    error: err.to_string(),
                    backoff_ms,
                })
                .with_correlation(&cid_clone);
                sink(event);
            }
        })
        .await;

        match result {
            Ok(response) => {
                let latency_ms = started.elapsed().as_millis() as u64;
                info!(
                    tokens_in = response.tokens_in,
                    tokens_out = response.tokens_out,
                    cost_usd = response.cost_usd,
                    latency_ms,
                    tool_call = %response.tool_call.summary(),
                    "Model response received"
                );
                self.emit(
                    EventKind::ModelResponseReceived {
                        tokens_in: response.tokens_in,
                        tokens_out: response.tokens_out,
                        cost_usd: response.cost_usd,
                        latency_ms: response.latency_ms,
                    },
                    Some(&cid),
                );
                Ok(ModelReply { response, correlation_id: cid })
            }
            Err(e) => {
                let attempts = attempt + 1;
                error!(attempts, error = %e, "Model request failed after all retries");
                self.emit(
                    EventKind::ModelFailed {
                        error: e.to_string(),
                        attempts,
                    },
                    Some(&cid),
                );
                Err(AgentError::ModelError { message: e.to_string() })
            }
        }
    }

    fn client_model_name(&self) -> String {
        self.client.model_name().to_string()
    }
}

//! Structured event log.
//!
//! Every actor emits events via `OutputPort<Event>` (ractor dport).
//! Events are immutable, timestamped, causally linked via `correlation_id`,
//! and rkyv-archived for zero-copy trajectory storage.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::capability::CommandCapability;
use crate::error::ExitStatus;
use crate::observation::ObservationArchive;
use crate::tool_call::ToolCall;

/// A single immutable event in the system log.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct Event {
    /// Sortable unique ID with embedded timestamp.
    pub id: String,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Which actor emitted this.
    pub actor_id: String,
    /// Links an effect back to its cause.
    pub correlation_id: Option<String>,
    pub kind: EventKind,
}

impl Event {
    pub fn new(actor_id: impl Into<String>, kind: EventKind) -> Self {
        Self {
            id: Ulid::new().to_string(),
            timestamp_ms: now_ms(),
            actor_id: actor_id.into(),
            correlation_id: None,
            kind,
        }
    }

    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    // ── Agent lifecycle ──────────────────────────────────────────────────────
    AgentStarted       { task: String, model: String },
    AgentStep          { step: u32, cost_so_far: f64 },
    AgentFinished      { exit_status: ExitStatus, submission: String, total_cost: f64, total_steps: u32, duration_ms: u64 },

    // ── Tool call / observation ───────────────────────────────────────────────
    ToolCallEmitted    { call: ToolCall, step: u32 },
    ObservationReceived { observation: ObservationArchive, duration_ms: u64 },

    // ── Shell actor ──────────────────────────────────────────────────────────
    ShellCommandStarted   { command: String, cwd: String },
    ShellCommandCompleted { exit_code: i64, duration_ms: u64, structured: bool },
    ShellCommandFailed    { error: String, exit_code: Option<i64> },

    // ── File actor ───────────────────────────────────────────────────────────
    FileRead    { path: String, size_bytes: usize },
    FileWritten { path: String, lines_changed: i64 },
    FileEdited  { path: String, old_len: usize, new_len: usize },

    // ── Model actor ──────────────────────────────────────────────────────────
    ModelRequestStarted  { model: String, message_count: usize },
    ModelResponseReceived { tokens_in: u32, tokens_out: u32, cost_usd: f64, latency_ms: u64 },
    ModelRetried         { attempt: u32, error: String, backoff_ms: u64 },
    ModelFailed          { error: String, attempts: u32 },

    // ── Capability discovery ─────────────────────────────────────────────────
    CapabilitiesPublished  { actor_id: String, command_count: usize, commands: Vec<CommandCapability> },
    CapabilityMapUpdated   { total_commands: usize, actor_count: usize },
    SystemPromptRegenerated { prompt_len: usize },

    // ── Orchestrator / batch ─────────────────────────────────────────────────
    AgentSpawned        { instance_id: String },
    AgentInstanceFailed { instance_id: String, error: String },
    BatchStarted        { instance_count: usize, worker_count: usize },
    BatchFinished       { completed: usize, failed: usize, duration_ms: u64 },

    // ── Task lifecycle ────────────────────────────────────────────────────────
    TaskLoaded   { crate_name: String, op: String, first_step: String },
    TaskAdvanced { crate_name: String, previous_step: String, current_step: String, step_index: u32 },
    TaskCompleted { crate_name: String, op: String, verification: String },
    TaskHalted   { crate_name: String, op: String, step: String, reason: String },
    TaskDeferred { crate_name: String, op: String, reason: String },
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

//! Policy and constraint checking types.
//!
//! These types form the contract between the normalization pipeline,
//! constraint checking actors, and the execution layer.
//!
//! Pipeline:
//!   ToolCall (raw)
//!     → ArgNormalizerActor    — fix types, collect FeedbackNotes
//!     → ConstraintCheckerActor — fan out to policy actors, reduce verdicts
//!     → ToolRouterActor        — pure dispatch, no business logic
//!     → FeedbackAssembler      — attach notes to Observation
//!     → OrchestratorActor      — deliver to model

use serde::{Deserialize, Serialize};
use crate::ToolCall;

// ── Feedback ──────────────────────────────────────────────────────────────────

/// A single coaching note generated during normalization or constraint checking.
/// Accumulated across the pipeline and delivered to the model alongside the
/// tool result so it can self-correct in subsequent steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackNote {
    /// Which actor or stage generated this note.
    pub source: String,
    pub severity: FeedbackSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSeverity {
    /// Something was fixed automatically. Model should adjust future calls.
    Info,
    /// A pattern that will cause problems. Model should stop doing this.
    Warning,
    /// A hard requirement. Model must satisfy this before proceeding.
    Required,
}

impl FeedbackNote {
    pub fn info(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self { source: source.into(), severity: FeedbackSeverity::Info, message: message.into() }
    }

    pub fn warning(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self { source: source.into(), severity: FeedbackSeverity::Warning, message: message.into() }
    }

    pub fn required(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self { source: source.into(), severity: FeedbackSeverity::Required, message: message.into() }
    }

    pub fn render(&self) -> String {
        let prefix = match self.severity {
            FeedbackSeverity::Info     => "ℹ️",
            FeedbackSeverity::Warning  => "⚠️",
            FeedbackSeverity::Required => "🚫",
        };
        format!("[{}] {}: {}", prefix, self.source, self.message)
    }
}

// ── Normalized call ───────────────────────────────────────────────────────────

/// A ToolCall that has passed through ArgNormalizerActor.
/// Types have been coerced, flag names normalized, and feedback collected.
/// Ready for constraint checking.
#[derive(Debug, Clone)]
pub struct NormalizedToolCall {
    pub call: ToolCall,
    pub feedback: Vec<FeedbackNote>,
}

impl NormalizedToolCall {
    pub fn unchanged(call: ToolCall) -> Self {
        Self { call, feedback: vec![] }
    }

    pub fn with_feedback(call: ToolCall, feedback: Vec<FeedbackNote>) -> Self {
        Self { call, feedback }
    }
}

// ── Policy context ────────────────────────────────────────────────────────────

/// Snapshot of orchestration state passed to every policy actor.
/// Gives policy actors everything they need without coupling them
/// to each other or to the orchestrator directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// Current agent step number.
    pub step: u32,
    /// Current playbook step name e.g. "survey", "orient", "scaffold", "write".
    pub playbook_step: String,
    /// Index of the current playbook step.
    pub playbook_index: u32,
    /// Tools approved for the current playbook step.
    pub approved_tools: Vec<String>,
    /// Tools forbidden in the current playbook step.
    pub forbidden_tools: Vec<String>,
    /// Summary of the previous tool call (from ToolCall::summary()).
    pub last_tool_call: Option<String>,
    /// Step number of the previous tool call.
    pub last_tool_step: Option<u32>,
    /// Summary of the last compile/check call, if any.
    pub last_compile_check: Option<LastCompileCheck>,
    /// Summary of the last test file write, if any.
    pub last_test_write: Option<LastTestWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastCompileCheck {
    pub step: u32,
    pub clean: bool,
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTestWrite {
    pub step: u32,
    pub path: String,
}

impl PolicyContext {
    pub fn initial() -> Self {
        Self {
            step: 0,
            playbook_step: "survey".to_string(),
            playbook_index: 0,
            approved_tools: vec![],
            forbidden_tools: vec![],
            last_tool_call: None,
            last_tool_step: None,
            last_compile_check: None,
            last_test_write: None,
        }
    }
}

// ── Policy verdict ────────────────────────────────────────────────────────────

/// The decision returned by each policy actor.
#[derive(Debug, Clone)]
pub enum PolicyVerdict {
    /// Call is valid. Proceed.
    Approved,
    /// Call is invalid. Do not execute. Return all feedback to model.
    Rejected {
        reason: String,
        feedback: Vec<FeedbackNote>,
    },
    /// Call was modified. Execute the new version. Tell model what changed.
    Modified {
        call: ToolCall,
        feedback: Vec<FeedbackNote>,
    },
}

impl PolicyVerdict {
    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }

    pub fn feedback(&self) -> &[FeedbackNote] {
        match self {
            Self::Approved => &[],
            Self::Rejected { feedback, .. } => feedback,
            Self::Modified { feedback, .. } => feedback,
        }
    }
}

// ── Pipeline result ───────────────────────────────────────────────────────────

/// Final output of the full normalization + constraint pipeline.
/// Either the call is cleared for execution (with accumulated feedback),
/// or it is blocked (with aggregated rejection reasons).
#[derive(Debug, Clone)]
pub enum PipelineResult {
    /// Execute this call. Attach feedback to the observation afterward.
    Execute {
        call: ToolCall,
        feedback: Vec<FeedbackNote>,
    },
    /// Block execution. Return this as an error observation to the model.
    Block {
        reason: String,
        feedback: Vec<FeedbackNote>,
    },
}

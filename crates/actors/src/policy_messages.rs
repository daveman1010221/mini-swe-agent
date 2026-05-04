//! Message types for the policy pipeline actors.
//!
//! Each policy actor receives a request message and replies via RpcReplyPort.
//!
//! Pipeline order:
//!   1. ArgNormalizerActor     — NormalizeRequest → NormalizedToolCall
//!   2. ConstraintCheckerActor — ConstraintRequest → PipelineResult
//!   3. ToolRouterActor        — RouteRequest (unchanged, pre-validated)
//!
//! Post-execution notifications:
//!   - ToolCallCompleted — sent after successful execution, drives PolicyContext updates
//!   - ToolCallRejected  — sent after pipeline blocks, drives loop enforcement

use ractor::RpcReplyPort;
use mswea_core::{
    policy::{NormalizedToolCall, PipelineResult, PolicyContext},
    ToolCall,
};

// ── ArgNormalizerActor messages ───────────────────────────────────────────────

/// Request to normalize a raw ToolCall from the model.
/// ArgNormalizerActor fixes types, coerces bools, normalizes flag names,
/// and collects FeedbackNotes for anything it changed.
pub struct NormalizeRequest {
    pub call: ToolCall,
    pub context: PolicyContext,
    pub step: u32,
    pub reply: RpcReplyPort<NormalizedToolCall>,
}

impl std::fmt::Debug for NormalizeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalizeRequest")
            .field("call", &self.call.summary())
            .field("step", &self.step)
            .finish()
    }
}

// ── ConstraintCheckerActor messages ──────────────────────────────────────────

/// Request to validate a normalized ToolCall against active constraints.
/// ConstraintCheckerActor fans out to domain policy actors and reduces
/// their verdicts into a single PipelineResult.
pub struct ConstraintRequest {
    pub normalized: NormalizedToolCall,
    pub step: u32,
    pub reply: RpcReplyPort<PipelineResult>,
}

impl std::fmt::Debug for ConstraintRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstraintRequest")
            .field("call", &self.normalized.call.summary())
            .field("step", &self.step)
            .finish()
    }
}

// ── PolicyContextUpdate ───────────────────────────────────────────────────────

/// Broadcast from OrchestratorActor whenever playbook state changes.
/// All policy actors receive this and update their cached context.
#[derive(Debug)]
pub struct PolicyContextUpdate {
    pub context: PolicyContext,
    pub reply: RpcReplyPort<()>,
}

/// Sent to ConstraintCheckerActor after each tool call completes successfully.
/// Allows it to update last_* tracking fields in PolicyContext
/// for use in subsequent constraint checks, and resets loop detection counters.
#[derive(Debug)]
pub struct ToolCallCompleted {
    pub call_summary: String,
    pub step: u32,
    pub path: Option<String>,        // set if this was a Write call
    pub was_compile_check: bool,
    pub compile_clean: Option<bool>, // Some(true/false) if was_compile_check
}

/// Sent to ConstraintCheckerActor after the pipeline blocks a tool call.
/// Drives consecutive rejection tracking and sequence loop detection.
/// Distinct from ToolCallCompleted — rejected calls never reach ToolRouter.
#[derive(Debug)]
pub struct ToolCallRejected {
    pub call_summary: String,
    pub step: u32,
    pub reason: String,
}

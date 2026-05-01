//! Message types for the policy pipeline actors.
//!
//! Each policy actor receives a request message and replies via RpcReplyPort.
//!
//! Pipeline order:
//!   1. ArgNormalizerActor   — NormalizeRequest → NormalizedToolCall
//!   2. ConstraintCheckerActor — ConstraintRequest → PipelineResult
//!   3. ToolRouterActor      — RouteRequest (unchanged, pre-validated)

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
    pub context: PolicyContext,
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
#[derive(Debug, Clone)]
pub struct PolicyContextUpdate {
    pub context: PolicyContext,
}

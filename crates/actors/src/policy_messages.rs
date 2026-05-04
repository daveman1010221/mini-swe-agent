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
//!
//! All message types defined in mswea-core and re-exported here to break
//! the actors ← nu-plugin-mswea ← actors dependency cycle.

pub use mswea_core::policy::{
    ConstraintCheckerMsg, ConstraintRequest, NormalizeRequest, PolicyContextUpdate,
    ToolCallCompleted, ToolCallRejected,
};

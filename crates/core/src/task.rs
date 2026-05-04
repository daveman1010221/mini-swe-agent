//! Task state types owned by TaskActor.
//!
//! These are the authoritative runtime types for agent task state.
//! TaskActor owns an instance of TaskFile in memory and writes it
//! to disk as a backing store after every mutation.
//!
//! Distinct from config::TaskFile which is a thin boot-time type
//! used only to load the initial mission briefing.

use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use ractor::RpcReplyPort;
use ractor_cluster::RactorClusterMessage;

// ── Coverage plan ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct PlannedTest {
    pub name: String,
    #[serde(rename = "type")]
    pub test_type: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct CoveragePlan {
    pub public_interfaces: Vec<String>,
    pub failure_modes: Vec<String>,
    pub boundary_conditions: Vec<String>,
    pub serde_required: bool,
    pub rkyv_required: bool,
    pub existing_tests: u32,
    pub planned_tests: Vec<PlannedTest>,
    pub written_at: String,
}

// ── Orient record ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct OrientRecord {
    pub step: String,
    pub observed: String,
    pub decision: String,
    #[serde(default)]
    pub blockers: Option<String>,
    pub recorded_at: String,
}

// ── Attempt record ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct AttemptRecord {
    pub action: String,
    pub result: String,
    pub recorded_at: String,
}

// ── Current task ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct RuntimeTask {
    /// Crate name (serialized as "crate" for JSON compat with nushell tools)
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub crate_path: String,
    pub op: String,

    // Playbook execution state
    pub step: String,
    pub step_index: u32,
    pub step_attempts: u32,
    pub step_budget: u32,

    // Step history
    #[serde(default)]
    pub last_orient: Option<OrientRecord>,
    #[serde(default)]
    pub last_verification: Option<String>,
    #[serde(default)]
    pub last_advanced_at: Option<String>,
    #[serde(default)]
    pub attempts: Vec<AttemptRecord>,

    // Coverage plan — set during orient step
    #[serde(default)]
    pub coverage_plan: Option<CoveragePlan>,

    // Optional metadata
    #[serde(default)]
    pub review: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
}

impl RuntimeTask {
    pub fn budget_remaining(&self) -> i32 {
        self.step_budget as i32 - self.step_attempts as i32
    }

    pub fn budget_exhausted(&self) -> bool {
        self.step_attempts >= self.step_budget
    }
}

// ── Completed task ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct CompletedTask {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub op: String,
    pub status: String,
    pub verification: String,
    pub completed_at: String,
}

// ── Halted task ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct HaltedTask {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub op: String,
    pub step: String,
    pub reason: String,
    pub halted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct DeferredTask {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub op: String,
    pub step: String,
    pub reason: String,
    pub deferred_at: String,
}

// ── Runtime task file ─────────────────────────────────────────────────────────

/// The authoritative runtime task file owned by TaskActor.
/// Loaded from disk at boot, mutated in memory, written back as backing store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTaskFile {
    pub workspace_root: String,
    pub current_task: Option<RuntimeTask>,
    #[serde(default)]
    pub pending: Vec<serde_json::Value>,
    #[serde(default)]
    pub completed: Vec<CompletedTask>,
    #[serde(default)]
    pub halted: Vec<HaltedTask>,
    #[serde(default)]
    pub deferred: Vec<DeferredTask>,
    #[serde(default)]
    pub last_updated: Option<String>,
}

impl RuntimeTaskFile {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read taskfile {}: {e}", path.display()))?;
        let tf: Self = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse taskfile {}: {e}", path.display()))?;
        Ok(tf)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize taskfile: {e}"))?;
        std::fs::write(path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write taskfile {}: {e}", path.display()))?;
        Ok(())
    }
}

// ── RPC request/response types ────────────────────────────────────────────────
// These are the HTTP request/response bodies for the TaskActor RPC server.

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct AdvanceRequest {
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct AdvanceResponse {
    pub ok: bool,
    pub advanced: bool,
    pub previous_step: Option<String>,
    pub current_step: Option<String>,
    pub task_completed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct WriteCoveragePlanRequest {
    pub public_interfaces: Vec<String>,
    pub failure_modes: Vec<String>,
    pub boundary_conditions: Vec<String>,
    pub serde_required: bool,
    pub rkyv_required: bool,
    pub existing_tests: u32,
    pub planned_tests: Vec<PlannedTest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct WriteCoveragePlanResponse {
    pub ok: bool,
    pub plan_recorded: bool,
    pub planned_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct RecordAttemptRequest {
    pub action: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct RecordAttemptResponse {
    pub ok: bool,
    pub step_attempts: u32,
    pub budget_remaining: i32,
    pub budget_exhausted: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct RecordOrientRequest {
    pub observed: String,
    pub decision: String,
    pub blockers: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct RecordOrientResponse {
    pub ok: bool,
    pub recorded: bool,
    pub step: String,
    pub budget_remaining: i32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct HaltRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct HaltResponse {
    pub ok: bool,
    pub halted: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct LoadTaskRequest {
    // No fields — pops the next pending task, no input required
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct LoadTaskResponse {
    pub ok: bool,
    pub has_task: bool,
    pub crate_name: Option<String>,
    pub op: Option<String>,
    pub first_step: Option<String>,
    pub playbook_found: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct DeferTaskRequest {
    pub crate_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct DeferTaskResponse {
    pub ok: bool,
    pub deferred: bool,
    pub crate_name: Option<String>,
    pub reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct TaskStateResponse {
    pub ok: bool,
    pub data: Option<TaskStateData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct TaskStateData {
    pub has_task: bool,
    pub crate_name: Option<String>,
    pub crate_path: Option<String>,
    pub op: Option<String>,
    pub step: Option<String>,
    pub step_index: Option<u32>,
    pub step_attempts: Option<u32>,
    pub step_budget: Option<u32>,
    pub budget_remaining: Option<i32>,
    pub budget_exhausted: Option<bool>,
    pub coverage_plan: Option<CoveragePlan>,
    pub last_orient: Option<OrientRecord>,
    pub pending_count: usize,
    pub completed_count: usize,
    pub halted_count: usize,
}

// ── Actor message type ────────────────────────────────────────────────────────
// Defined in mswea-core so nu-plugin-mswea can hold ActorRef<TaskMsg>
// without depending on the actors crate.

#[derive(Debug, RactorClusterMessage)]
pub enum TaskMsg {
    #[rpc]
    Advance {
        req: AdvanceRequest,
        reply: RpcReplyPort<AdvanceResponse>,
    },
    #[rpc]
    WriteCoveragePlan {
        req: WriteCoveragePlanRequest,
        reply: RpcReplyPort<WriteCoveragePlanResponse>,
    },
    #[rpc]
    RecordAttempt {
        req: RecordAttemptRequest,
        reply: RpcReplyPort<RecordAttemptResponse>,
    },
    #[rpc]
    RecordOrient {
        req: RecordOrientRequest,
        reply: RpcReplyPort<RecordOrientResponse>,
    },
    #[rpc]
    Halt {
        req: HaltRequest,
        reply: RpcReplyPort<HaltResponse>,
    },
    #[rpc]
    GetState {
        reply: RpcReplyPort<TaskStateResponse>,
    },
    #[rpc]
    LoadTask {
        req: LoadTaskRequest,
        reply: RpcReplyPort<LoadTaskResponse>,
    },
    #[rpc]
    DeferTask {
        req: DeferTaskRequest,
        reply: RpcReplyPort<DeferTaskResponse>,
    },
}

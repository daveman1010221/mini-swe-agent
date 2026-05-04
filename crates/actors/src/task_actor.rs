//! TaskActor — authoritative owner of agent task state.
//!
//! Owns RuntimeTaskFile in memory, exposes an axum HTTP RPC server
//! on localhost:8000 for nushell tools to query and mutate state.
//!
//! On every state mutation:
//!   1. Update in-memory state
//!   2. call!(constraint_checker, UpdateContext) — blocks for ack
//!   3. call!(orchestrator, UpdatePlaybookStep) — blocks for ack  
//!   4. Write backing JSON file
//!   5. Return response to HTTP caller

use std::path::PathBuf;

use axum::{
    extract::State,
    response::Json,
    routing::post,
    Router,
};

use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::info;

use mswea_core::{
    task::{
        AdvanceRequest, AdvanceResponse, HaltRequest, HaltResponse,
        RecordAttemptRequest, RecordAttemptResponse, RecordOrientRequest,
        RecordOrientResponse, RuntimeTaskFile, TaskStateResponse,
        WriteCoveragePlanRequest, WriteCoveragePlanResponse, OrientRecord,
        AttemptRecord, CompletedTask, HaltedTask, DeferredTask, CoveragePlan,
        LoadTaskRequest, LoadTaskResponse, DeferTaskRequest, DeferTaskResponse,
    },
    event::{Event, EventKind},  // ← add this
    PolicyContext, RuntimeTask, TaskStateData,
};

use crate::{
    constraint_checker::ConstraintCheckerMsg,
    orchestrator::OrchestratorMsg,
    policy_messages::PolicyContextUpdate,
    event_bus::EventBus,
};

use tokio::net::TcpListener;

pub use mswea_core::task::TaskMsg;

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct TaskActor;

pub struct TaskActorArgs {
    pub taskfile_path: PathBuf,
    pub rpc_port: u16,
    pub constraint_checker: ActorRef<ConstraintCheckerMsg>,
    pub orchestrator: ActorRef<OrchestratorMsg>,
    pub event_bus: EventBus,
    pub server_cert_pem: String,
    pub server_key_pem: String,
    pub ca_cert_pem: String,
}

pub struct TaskActorState {
    pub taskfile: RuntimeTaskFile,
    pub taskfile_path: PathBuf,
    pub constraint_checker: ActorRef<ConstraintCheckerMsg>,
    pub orchestrator: ActorRef<OrchestratorMsg>,
    pub event_bus: EventBus,
}

pub type TaskRpcState = ActorRef<TaskMsg>;

// ── Actor impl ────────────────────────────────────────────────────────────────

impl Actor for TaskActor {
    type Msg = TaskMsg;
    type State = TaskActorState;
    type Arguments = TaskActorArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!("TaskActor starting, loading taskfile from {}", args.taskfile_path.display());

        let taskfile = RuntimeTaskFile::load(&args.taskfile_path)
            .map_err(|e| ActorProcessingErr::from(format!("Failed to load taskfile: {e}")))?;

        // Spawn axum RPC server — passes actor ref to handlers
        let actor_ref = myself.clone();
        let port = args.rpc_port;

        tokio::spawn(async move {
            let app = Router::new()
                .route("/task/state",               post(handle_get_state))
                .route("/task/advance",             post(handle_advance))
                .route("/task/write-coverage-plan", post(handle_write_coverage_plan))
                .route("/task/record-attempt",      post(handle_record_attempt))
                .route("/task/record-orient",       post(handle_record_orient))
                .route("/task/halt",                post(handle_halt))
                .route("/task/load",                post(handle_load_task))
                .route("/task/defer",               post(handle_defer_task))
                .with_state(actor_ref);

            let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
                .await
                .expect("TaskActor RPC server failed to bind");
            info!("TaskActor RPC server listening on http://127.0.0.1:{port}");
            axum::serve(listener, app)
                .await
                .expect("TaskActor RPC server failed");
        });

        Ok(TaskActorState {
            taskfile,
            taskfile_path: args.taskfile_path,
            constraint_checker: args.constraint_checker,
            orchestrator: args.orchestrator,
            event_bus: args.event_bus,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            TaskMsg::GetState { reply } => {
                let response = build_state_response(&state.taskfile);
                let _ = reply.send(response);
            }

            TaskMsg::Advance { req, reply } => {
                let response = handle_advance_msg(req, state).await;
                let _ = reply.send(response);
            }

            TaskMsg::WriteCoveragePlan { req, reply } => {
                let response = handle_write_coverage_plan_msg(req, state).await;
                let _ = reply.send(response);
            }

            TaskMsg::RecordAttempt { req, reply } => {
                let response = handle_record_attempt_msg(req, state).await;
                let _ = reply.send(response);
            }

            TaskMsg::RecordOrient { req, reply } => {
                let response = handle_record_orient_msg(req, state).await;
                let _ = reply.send(response);
            }

            TaskMsg::Halt { req, reply } => {
                let response = handle_halt_msg(req, state).await;
                let _ = reply.send(response);
            }
            TaskMsg::LoadTask { req, reply } => {
                let response = handle_load_task_msg(req, state).await;
                let _ = reply.send(response);
            }
            TaskMsg::DeferTask { req, reply } => {
                let response = handle_defer_task_msg(req, state).await;
                let _ = reply.send(response);
            }
        }
        Ok(())
    }
}

// ── Axum HTTP handlers (thin shims → actor via ractor RPC) ────────────────────

async fn handle_get_state(
    State(actor): State<TaskRpcState>,
) -> Json<TaskStateResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::GetState { reply }
    ).unwrap_or_else(|e| TaskStateResponse {
        ok: false,
        data: None,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_advance(
    State(actor): State<TaskRpcState>,
    Json(req): Json<AdvanceRequest>,
) -> Json<AdvanceResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::Advance { req, reply }
    ).unwrap_or_else(|e| AdvanceResponse {
        ok: false,
        advanced: false,
        previous_step: None,
        current_step: None,
        task_completed: false,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_write_coverage_plan(
    State(actor): State<TaskRpcState>,
    Json(req): Json<WriteCoveragePlanRequest>,
) -> Json<WriteCoveragePlanResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::WriteCoveragePlan { req, reply }
    ).unwrap_or_else(|e| WriteCoveragePlanResponse {
        ok: false,
        plan_recorded: false,
        planned_count: 0,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_record_attempt(
    State(actor): State<TaskRpcState>,
    Json(req): Json<RecordAttemptRequest>,
) -> Json<RecordAttemptResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::RecordAttempt { req, reply }
    ).unwrap_or_else(|e| RecordAttemptResponse {
        ok: false,
        step_attempts: 0,
        budget_remaining: 0,
        budget_exhausted: false,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_record_orient(
    State(actor): State<TaskRpcState>,
    Json(req): Json<RecordOrientRequest>,
) -> Json<RecordOrientResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::RecordOrient { req, reply }
    ).unwrap_or_else(|e| RecordOrientResponse {
        ok: false,
        recorded: false,
        step: String::new(),
        budget_remaining: 0,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_halt(
    State(actor): State<TaskRpcState>,
    Json(req): Json<HaltRequest>,
) -> Json<HaltResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::Halt { req, reply }
    ).unwrap_or_else(|e| HaltResponse {
        ok: false,
        halted: false,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_load_task(
    State(actor): State<TaskRpcState>,
    Json(req): Json<LoadTaskRequest>,
) -> Json<LoadTaskResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::LoadTask { req, reply }
    ).unwrap_or_else(|e| LoadTaskResponse {
        ok: false,
        has_task: false,
        crate_name: None,
        op: None,
        first_step: None,
        playbook_found: false,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_defer_task(
    State(actor): State<TaskRpcState>,
    Json(req): Json<DeferTaskRequest>,
) -> Json<DeferTaskResponse> {
    let result = ractor::call!(
        actor,
        |reply| TaskMsg::DeferTask { req, reply }
    ).unwrap_or_else(|e| DeferTaskResponse {
        ok: false,
        deferred: false,
        crate_name: None,
        reason: None,
        error: Some(format!("RPC failed: {e}")),
    });
    Json(result)
}

async fn handle_load_task_msg(
    _req: LoadTaskRequest,
    state: &mut TaskActorState,
) -> LoadTaskResponse {
    // If there's already a non-halted current task, return it
    if let Some(ref task) = state.taskfile.current_task {
        return LoadTaskResponse {
            ok: true,
            has_task: true,
            crate_name: Some(task.crate_name.clone()),
            op: Some(task.op.clone()),
            first_step: Some(task.step.clone()),
            playbook_found: true,
            error: None,
        };
    }

    // Pop next from pending
    if state.taskfile.pending.is_empty() {
        return LoadTaskResponse {
            ok: true,
            has_task: false,
            crate_name: None,
            op: None,
            first_step: None,
            playbook_found: false,
            error: None,
        };
    }

    let next_raw = state.taskfile.pending.remove(0);
    let next: RuntimeTask = match serde_json::from_value(next_raw) {
        Ok(t) => t,
        Err(e) => return LoadTaskResponse {
            ok: false,
            has_task: false,
            crate_name: None,
            op: None,
            first_step: None,
            playbook_found: false,
            error: Some(format!("Failed to deserialize pending task: {e}")),
        },
    };

    let crate_name = next.crate_name.clone();
    let op = next.op.clone();
    let first_step = next.step.clone();

    state.taskfile.current_task = Some(next);
    state.taskfile.last_updated = Some(now_utc());

    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        tracing::error!("Failed to save taskfile: {e}");
    }

    state.event_bus.send(Event::new(
        "task-actor",
        EventKind::TaskLoaded {
            crate_name: crate_name.clone(),
            op: op.clone(),
            first_step: first_step.clone(),
        },
    ));

    if let Some(ref task) = state.taskfile.current_task {
        if let Err(e) = notify_step_change(task, state).await {
            tracing::error!("notify_step_change failed: {e}");
        }
    }

    LoadTaskResponse {
        ok: true,
        has_task: true,
        crate_name: Some(crate_name),
        op: Some(op),
        first_step: Some(first_step),
        playbook_found: true,
        error: None,
    }
}

async fn handle_defer_task_msg(
    req: DeferTaskRequest,
    state: &mut TaskActorState,
) -> DeferTaskResponse {
    let Some(ref task) = state.taskfile.current_task else {
        return DeferTaskResponse {
            ok: false,
            deferred: false,
            crate_name: None,
            reason: None,
            error: Some("no current task".to_string()),
        };
    };

    if task.crate_name != req.crate_name {
        return DeferTaskResponse {
            ok: false,
            deferred: false,
            crate_name: None,
            reason: None,
            error: Some(format!(
                "current task is '{}', not '{}'",
                task.crate_name, req.crate_name
            )),
        };
    }

    let deferred = DeferredTask {
        crate_name: task.crate_name.clone(),
        op: task.op.clone(),
        step: task.step.clone(),
        reason: req.reason.clone(),
        deferred_at: now_utc(),
    };

    state.event_bus.send(Event::new(
        "task-actor",
        EventKind::TaskDeferred {
            crate_name: deferred.crate_name.clone(),
            op: deferred.op.clone(),
            reason: req.reason.clone(),
        },
    ));

    state.taskfile.deferred.push(deferred);
    state.taskfile.current_task = None;
    state.taskfile.last_updated = Some(now_utc());

    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        tracing::error!("Failed to save taskfile: {e}");
    }

    DeferTaskResponse {
        ok: true,
        deferred: true,
        crate_name: Some(req.crate_name),
        reason: Some(req.reason),
        error: None,
    }
}

// ── Business logic ────────────────────────────────────────────────────────────

fn build_state_response(taskfile: &RuntimeTaskFile) -> TaskStateResponse {
    let Some(ref task) = taskfile.current_task else {
        return TaskStateResponse {
            ok: true,
            data: Some(TaskStateData {
                has_task: false,
                crate_name: None,
                crate_path: None,
                op: None,
                step: None,
                step_index: None,
                step_attempts: None,
                step_budget: None,
                budget_remaining: None,
                budget_exhausted: None,
                coverage_plan: None,
                last_orient: None,
                pending_count: taskfile.pending.len(),
                completed_count: taskfile.completed.len(),
                halted_count: taskfile.halted.len(),
            }),
            error: None,
        };
    };

    TaskStateResponse {
        ok: true,
        data: Some(TaskStateData {
            has_task: true,
            crate_name: Some(task.crate_name.clone()),
            crate_path: Some(task.crate_path.clone()),
            op: Some(task.op.clone()),
            step: Some(task.step.clone()),
            step_index: Some(task.step_index),
            step_attempts: Some(task.step_attempts),
            step_budget: Some(task.step_budget),
            budget_remaining: Some(task.budget_remaining()),
            budget_exhausted: Some(task.budget_exhausted()),
            coverage_plan: task.coverage_plan.clone(),
            last_orient: task.last_orient.clone(),
            pending_count: taskfile.pending.len(),
            completed_count: taskfile.completed.len(),
            halted_count: taskfile.halted.len(),
        }),
        error: None,
    }
}

/// Notify downstream actors of step change in guaranteed order.
/// ConstraintCheckerActor must ack before OrchestratorActor is notified.
async fn notify_step_change(
    task: &RuntimeTask,
    state: &TaskActorState,
) -> Result<(), ActorProcessingErr> {
    // Build new PolicyContext from current task state
    let ctx = PolicyContext {
        step: 0, // agent step not known here — set per tool call in main.rs
        playbook_step: task.step.clone(),
        playbook_index: task.step_index,
        approved_tools: vec![],   // ConstraintChecker gets these from playbook
        forbidden_tools: vec![],
        global_approved_tools: vec![],
        last_tool_call: None,
        last_tool_step: None,
        last_compile_check: None,
        last_test_write: None,
    };

    // 1. ConstraintCheckerActor — blocks until ack
    ractor::call!(
        state.constraint_checker,
        |reply| ConstraintCheckerMsg::UpdateContext(
            PolicyContextUpdate { context: ctx, reply }
        )
    ).map_err(|e| ActorProcessingErr::from(
        format!("ConstraintChecker UpdateContext failed: {e}")
    ))?;

    // 2. OrchestratorActor — blocks until prompt regenerated
    state.orchestrator
        .cast(OrchestratorMsg::PlaybookStepChanged {
            step: task.step.clone(),
            step_index: task.step_index,
        })
        .map_err(|e| ActorProcessingErr::from(
            format!("Orchestrator PlaybookStepChanged failed: {e}")
        ))?;

    Ok(())
}

async fn handle_advance_msg(
    req: AdvanceRequest,
    state: &mut TaskActorState,
) -> AdvanceResponse {
    if req.verification.is_empty() {
        return AdvanceResponse {
            ok: false,
            advanced: false,
            previous_step: None,
            current_step: None,
            task_completed: false,
            error: Some("verification cannot be empty".to_string()),
        };
    }

    let Some(ref mut task) = state.taskfile.current_task else {
        return AdvanceResponse {
            ok: false,
            advanced: false,
            previous_step: None,
            current_step: None,
            task_completed: false,
            error: Some("no current task".to_string()),
        };
    };

    // Playbook steps — will come from PlaybookActor in future
    let playbook_steps = ["survey", "orient", "scaffold", "write", "verify", "finalize"];
    let next_index = task.step_index + 1;
    let task_completed = next_index as usize >= playbook_steps.len();

    let previous_step = task.step.clone();
    let next_step = if task_completed {
        None
    } else {
        Some(playbook_steps[next_index as usize].to_string())
    };

    if task_completed {
        // Move to completed list
        let completed = CompletedTask {
            crate_name: task.crate_name.clone(),
            op: task.op.clone(),
            status: "done".to_string(),
            verification: req.verification.clone(),
            completed_at: now_utc(),
        };
        state.taskfile.completed.push(completed);
        state.taskfile.current_task = state.taskfile.pending
            .first()
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        if !state.taskfile.pending.is_empty() {
            state.taskfile.pending.remove(0);
        }
    } else {
        let next = next_step.clone().unwrap();
        task.step = next;
        task.step_index = next_index;
        task.step_attempts = 0;
        task.last_verification = Some(req.verification.clone());
        task.last_advanced_at = Some(now_utc());
    }

    state.taskfile.last_updated = Some(now_utc());

    // Notify downstream actors in order
    if let Some(ref task) = state.taskfile.current_task {
        if let Err(e) = notify_step_change(task, state).await {
            tracing::error!("notify_step_change failed: {e}");
        }
    }

    // Write backing store
    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        tracing::error!("Failed to save taskfile: {e}");
    }

    // Emit task lifecycle event
    if task_completed {
        state.event_bus.send(Event::new(
            "task-actor",
            EventKind::TaskCompleted {
                crate_name: previous_step.clone(), // already captured before mutation
                op: String::new(), // task moved to completed list
                verification: req.verification.clone(),
            },
        ));
    } else {
        state.event_bus.send(Event::new(
            "task-actor",
            EventKind::TaskAdvanced {
                crate_name: state.taskfile.current_task
                    .as_ref()
                    .map(|t| t.crate_name.clone())
                    .unwrap_or_default(),
                previous_step: previous_step.clone(),
                current_step: next_step.clone().unwrap_or_default(),
                step_index: state.taskfile.current_task
                    .as_ref()
                    .map(|t| t.step_index)
                    .unwrap_or(0),
            },
        ));
    }

    AdvanceResponse {
        ok: true,
        advanced: true,
        previous_step: Some(previous_step),
        current_step: next_step,
        task_completed,
        error: None,
    }
}

async fn handle_write_coverage_plan_msg(
    req: WriteCoveragePlanRequest,
    state: &mut TaskActorState,
) -> WriteCoveragePlanResponse {
    if req.planned_tests.is_empty() {
        return WriteCoveragePlanResponse {
            ok: false,
            plan_recorded: false,
            planned_count: 0,
            error: Some("planned_tests cannot be empty".to_string()),
        };
    }

    let Some(ref mut task) = state.taskfile.current_task else {
        return WriteCoveragePlanResponse {
            ok: false,
            plan_recorded: false,
            planned_count: 0,
            error: Some("no current task".to_string()),
        };
    };

    let planned_count = req.planned_tests.len();
    task.coverage_plan = Some(CoveragePlan {
        public_interfaces: req.public_interfaces,
        failure_modes: req.failure_modes,
        boundary_conditions: req.boundary_conditions,
        serde_required: req.serde_required,
        rkyv_required: req.rkyv_required,
        existing_tests: req.existing_tests,
        planned_tests: req.planned_tests,
        written_at: now_utc(),
    });

    state.taskfile.last_updated = Some(now_utc());

    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        return WriteCoveragePlanResponse {
            ok: false,
            plan_recorded: false,
            planned_count: 0,
            error: Some(format!("Failed to save taskfile: {e}")),
        };
    }

    WriteCoveragePlanResponse {
        ok: true,
        plan_recorded: true,
        planned_count,
        error: None,
    }
}

async fn handle_record_attempt_msg(
    req: RecordAttemptRequest,
    state: &mut TaskActorState,
) -> RecordAttemptResponse {
    let Some(ref mut task) = state.taskfile.current_task else {
        return RecordAttemptResponse {
            ok: false,
            step_attempts: 0,
            budget_remaining: 0,
            budget_exhausted: false,
            error: Some("no current task".to_string()),
        };
    };

    task.step_attempts += 1;
    task.attempts.push(AttemptRecord {
        action: req.action,
        result: req.result,
        recorded_at: now_utc(),
    });

    let step_attempts = task.step_attempts;
    let budget_remaining = task.budget_remaining();
    let budget_exhausted = task.budget_exhausted();

    state.taskfile.last_updated = Some(now_utc());

    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        tracing::error!("Failed to save taskfile: {e}");
    }

    RecordAttemptResponse {
        ok: true,
        step_attempts,
        budget_remaining,
        budget_exhausted,
        error: None,
    }
}

async fn handle_record_orient_msg(
    req: RecordOrientRequest,
    state: &mut TaskActorState,
) -> RecordOrientResponse {
    let Some(ref mut task) = state.taskfile.current_task else {
        return RecordOrientResponse {
            ok: false,
            recorded: false,
            step: String::new(),
            budget_remaining: 0,
            error: Some("no current task".to_string()),
        };
    };

    let step = task.step.clone();
    let budget_remaining = task.budget_remaining();

    task.last_orient = Some(OrientRecord {
        step: step.clone(),
        observed: req.observed,
        decision: req.decision,
        blockers: req.blockers,
        recorded_at: now_utc(),
    });

    state.taskfile.last_updated = Some(now_utc());

    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        tracing::error!("Failed to save taskfile: {e}");
    }

    RecordOrientResponse {
        ok: true,
        recorded: true,
        step,
        budget_remaining,
        error: None,
    }
}

async fn handle_halt_msg(
    req: HaltRequest,
    state: &mut TaskActorState,
) -> HaltResponse {
    let Some(ref task) = state.taskfile.current_task else {
        return HaltResponse {
            ok: false,
            halted: false,
            error: Some("no current task".to_string()),
        };
    };

    let halted = HaltedTask {
        crate_name: task.crate_name.clone(),
        op: task.op.clone(),
        step: task.step.clone(),
        reason: req.reason.clone(),
        halted_at: now_utc(),
    };

    state.taskfile.halted.push(halted.clone());
    state.taskfile.current_task = None;
    state.taskfile.last_updated = Some(now_utc());

    if let Err(e) = state.taskfile.save(&state.taskfile_path) {
        tracing::error!("Failed to save taskfile: {e}");
    }

    state.event_bus.send(Event::new(
        "task-actor",
        EventKind::TaskHalted {
            crate_name: halted.crate_name.clone(),
            op: halted.op.clone(),
            step: halted.step.clone(),
            reason: req.reason.clone(),
        },
    ));

    HaltResponse {
        ok: true,
        halted: true,
        error: None,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

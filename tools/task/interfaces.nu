# task/ namespace — tool interface specifications
#
# These are the foundational tools. Every OODA cycle starts here.
# The agent cannot proceed without a valid task state.
#
# All tools follow the standard contract:
#   Input:  structured record
#   Output: {ok: bool, data: any, error: string | null}
#   Exit:   always 0 — errors live in the record

# =============================================================================
# task/state
#
# The first call in every OBSERVE phase.
# Returns the complete current task state including playbook position.
# =============================================================================
#
# Input:
#   --taskfile: path   # path to agent-task.json (default: $TASKFILE env var)
#
# Output:
# {
#   ok: true,
#   data: {
#     has_task: bool,
#     task_id: string,
#     type: string,               # "write-tests", "fix-clippy", etc.
#     crate: string,
#     crate_path: string,
#     playbook: string,           # which playbook is loaded
#     step: string,               # current step name
#     step_index: int,            # 0-based position in playbook
#     step_attempts: int,         # how many times we've tried this step
#     step_budget: int,           # max attempts before halt required
#     budget_remaining: int,      # step_budget - step_attempts
#     budget_exhausted: bool,     # true if step_attempts >= step_budget
#     coverage_plan: any,         # null until orient phase writes it
#     pending_count: int,
#     completed_count: int,
#     blocked_count: int,
#     started_at: string,
#     notes: string
#   },
#   error: null
# }
#
# On no current task:
# {
#   ok: true,
#   data: { has_task: false, pending_count: int, ... },
#   error: null
# }


# =============================================================================
# task/advance
#
# Called when a verification gate passes. Moves to the next playbook step.
# Resets step_attempts to 0. If on last step, marks task complete.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --verification: string   # evidence that the gate passed (e.g. "cargo test: 12 passed, 0 failed")
#
# Output:
# {
#   ok: true,
#   data: {
#     advanced: bool,
#     previous_step: string,
#     current_step: string,       # null if task completed
#     task_completed: bool,
#     next_task: any              # null if no pending tasks
#   },
#   error: null
# }


# =============================================================================
# task/halt
#
# Called when the agent cannot proceed. Records the reason and OODA phase.
# Human must intervene before work can resume.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --reason: string         # specific, actionable description of why
#   --ooda-phase: string     # "observe" | "orient" | "decide" | "act"
#   --context: string        # what the agent was attempting
#
# Output:
# {
#   ok: true,
#   data: {
#     halted: true,
#     task_id: string,
#     step: string,
#     reason: string,
#     ooda_phase: string,
#     halted_at: string
#   },
#   error: null
# }


# =============================================================================
# task/block
#
# Called when a task cannot be completed due to external dependency,
# missing infrastructure, or explicit out-of-scope constraint.
# Different from halt — block is a known, named reason. Halt is unexpected.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --crate: string
#   --reason: string
#
# Output:
# {
#   ok: true,
#   data: {
#     blocked: true,
#     crate: string,
#     reason: string,
#     surfaced_at: string
#   },
#   error: null
# }


# =============================================================================
# task/record-attempt
#
# Called after every ACT phase. Increments step_attempts.
# Agent calls this BEFORE checking if the attempt succeeded.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --action: string         # what tool was called
#   --result: string         # brief description of outcome
#
# Output:
# {
#   ok: true,
#   data: {
#     step_attempts: int,
#     budget_remaining: int,
#     budget_exhausted: bool   # agent must halt if true
#   },
#   error: null
# }


# =============================================================================
# task/write-coverage-plan
#
# Called during ORIENT phase before writing any tests.
# Documents the agent's reasoning about what needs to be tested.
# Becomes the verification contract for mark_done.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --plan: record {
#       public_interfaces: [string],
#       failure_modes: [{interface: string, modes: [string]}],
#       boundary_conditions: [string],
#       rkyv_required: bool,
#       serde_required: bool,
#       existing_tests: int,
#       planned_tests: [{name: string, type: string, rationale: string}]
#     }
#
# Output:
# {
#   ok: true,
#   data: {
#     plan_recorded: true,
#     planned_count: int
#   },
#   error: null
# }


# =============================================================================
# task/next
#
# Pops the next pending task and sets it as current.
# Looks up the playbook for the task type.
# Halts if no playbook exists for the task type.
# =============================================================================
#
# Input:
#   --taskfile: path
#
# Output:
# {
#   ok: true,
#   data: {
#     has_task: bool,
#     task_type: string,
#     playbook_found: bool,   # false = agent must halt
#     crate: string,
#     first_step: string
#   },
#   error: null
# }

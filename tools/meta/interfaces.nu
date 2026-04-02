# meta/ namespace — tool interface specifications
#
# These are the OBSERVE phase tools. Called at the start of every OODA cycle.
# They give the agent visibility into its own execution state.
# Without these, the agent is flying blind.

# =============================================================================
# meta/loop-detect
#
# Analyzes the trajectory to detect reasoning loops.
# A loop is: same tool + same inputs appearing > threshold times
# without an intervening step-advance.
#
# This is the most important safety tool in the toolbox.
# =============================================================================
#
# Input:
#   --trajectory: path       # path to trajectory JSONL
#   --window: int            # how many recent events to analyze (default: 20)
#   --threshold: int         # repeat count that triggers detection (default: 3)
#
# Output:
# {
#   ok: true,
#   data: {
#     loop_detected: bool,
#     loops: [{
#       tool: string,
#       command: string,      # the repeated command/input
#       count: int,
#       first_seen: string,   # timestamp
#       last_seen: string,
#       recommendation: string  # "try-alternate" | "halt"
#     }],
#     window_analyzed: int,
#     unique_actions: int
#   },
#   error: null
# }


# =============================================================================
# meta/trajectory-summary
#
# Returns a structured summary of recent agent activity.
# Used in OBSERVE to understand what has been done this session.
# Abbreviated by default — full history on request.
# =============================================================================
#
# Input:
#   --trajectory: path
#   --last-n: int            # most recent N events (default: 10)
#   --full: bool             # return full history (default: false)
#
# Output:
# {
#   ok: true,
#   data: {
#     total_events: int,
#     total_steps: int,
#     total_tool_calls: int,
#     shell_commands: [{
#       command: string,
#       exit_code: int,
#       timestamp: string
#     }],
#     files_read: [string],
#     files_written: [string],
#     files_edited: [string],
#     compile_attempts: int,
#     compile_successes: int,
#     test_runs: int,
#     test_successes: int,
#     recent_events: [{
#       kind: string,
#       summary: string,
#       timestamp: string
#     }]
#   },
#   error: null
# }


# =============================================================================
# meta/step-verify
#
# Checks whether the current playbook step's verification gate has been met.
# Called before task/advance — agent must not advance without calling this.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --trajectory: path
#   --step: string           # step name to verify (default: current step)
#
# Output:
# {
#   ok: true,
#   data: {
#     step: string,
#     gate: string,           # human-readable description of the gate
#     passed: bool,
#     evidence: string,       # what in the trajectory proves it passed
#     missing: [string]       # what is still required if not passed
#   },
#   error: null
# }


# =============================================================================
# meta/session-stats
#
# High-level session statistics. Used in ORIENT to assess overall progress.
# =============================================================================
#
# Input:
#   --trajectory: path
#   --taskfile: path
#
# Output:
# {
#   ok: true,
#   data: {
#     session_duration_mins: float,
#     tasks_completed: int,
#     tasks_remaining: int,
#     total_tool_calls: int,
#     loop_incidents: int,     # how many times loop-detect fired this session
#     halt_incidents: int,
#     compile_success_rate: float,
#     test_success_rate: float,
#     budget_warnings: int     # steps where budget_remaining < 2
#   },
#   error: null
# }


# =============================================================================
# meta/orient-report
#
# Synthesizes observe data into a structured orient report.
# Answers the orient questions from the standing orders explicitly.
# Agent writes this to the task file before deciding.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --trajectory: path
#   --observations: record   # output from task/state + meta/loop-detect + meta/trajectory-summary
#
# Output:
# {
#   ok: true,
#   data: {
#     coverage_plan_valid: bool,
#     last_gate_passed: bool,
#     loop_risk: string,       # "none" | "low" | "high" | "critical"
#     recommended_decision: string,  # "continue" | "alternate" | "halt"
#     reasoning: string        # explicit chain of reasoning, logged to trajectory
#   },
#   error: null
# }

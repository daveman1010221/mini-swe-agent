# playbook/ namespace — tool interface specifications
#
# The playbook system is how we encode engineering process discipline
# into the agent. Every known task type has exactly one playbook.
# Unknown task types are an immediate halt condition.
#
# Playbooks live as structured nushell records in:
#   mswea/tools/playbooks/<task-type>.nu
#
# The agent never reads playbook files directly — it goes through these tools.

# =============================================================================
# playbook/lookup
#
# The FIRST thing called when a new task is loaded.
# If no playbook exists for the task type, agent halts immediately.
# No improvisation on unknown task types.
# =============================================================================
#
# Input:
#   --type: string           # task type e.g. "write-tests", "fix-clippy"
#
# Output (found):
# {
#   ok: true,
#   data: {
#     found: true,
#     type: string,
#     version: string,
#     description: string,
#     steps: [{
#       name: string,
#       index: int,
#       description: string,
#       tools: [string],       # approved tools for this step
#       verification_gate: string,
#       budget: int,           # max attempts
#       on_budget_exhausted: string  # "halt" | "block" | "alternate:<step>"
#     }],
#     preconditions: [string],
#     success_condition: string
#   },
#   error: null
# }
#
# Output (not found):
# {
#   ok: true,
#   data: {
#     found: false,
#     type: string,
#     known_types: [string],   # list of all available playbook types
#     recommendation: "halt — no playbook for this task type"
#   },
#   error: null
# }


# =============================================================================
# playbook/current-step
#
# Returns full details of the current step including approved tools.
# Agent should call this after task/state to get step-specific guidance.
# =============================================================================
#
# Input:
#   --taskfile: path
#
# Output:
# {
#   ok: true,
#   data: {
#     step_name: string,
#     step_index: int,
#     description: string,
#     approved_tools: [string],   # ONLY these tools may be used this step
#     forbidden_tools: [string],  # explicitly forbidden (safety rail)
#     verification_gate: string,
#     budget: int,
#     budget_remaining: int,
#     orient_questions: [string], # step-specific orient questions
#     example_actions: [string]   # concrete examples of valid actions
#   },
#   error: null
# }


# =============================================================================
# playbook/list
#
# Returns all available playbooks with descriptions.
# Used by tools/search for discovery.
# =============================================================================
#
# Input: none
#
# Output:
# {
#   ok: true,
#   data: {
#     count: int,
#     playbooks: [{
#       type: string,
#       version: string,
#       description: string,
#       step_count: int,
#       typical_duration: string  # rough estimate e.g. "3-5 tool calls"
#     }]
#   },
#   error: null
# }


# =============================================================================
# playbook/validate
#
# Checks that the current task execution is consistent with its playbook.
# Called during ORIENT to catch drift — agent went off-playbook somehow.
# =============================================================================
#
# Input:
#   --taskfile: path
#   --trajectory: path
#
# Output:
# {
#   ok: true,
#   data: {
#     on_playbook: bool,
#     violations: [{
#       step: string,
#       expected: string,
#       actual: string,
#       severity: string    # "warning" | "critical"
#     }],
#     recommendation: string  # "continue" | "reset-step" | "halt"
#   },
#   error: null
# }

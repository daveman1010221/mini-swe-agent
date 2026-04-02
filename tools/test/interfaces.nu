# test/ namespace — tool interface specifications
#
# Wraps cargo test with structured output.
# Agent never reads raw test output — always structured records.

# =============================================================================
# test/run
#
# Runs tests for a crate and returns structured pass/fail data.
# =============================================================================
#
# Input:
#   --workspace-root: path
#   --crate: string
#   --target: string         # "unit" | "props" | "integration" | "all"
#   --filter: string         # optional test name filter
#
# Output:
# {
#   ok: true,
#   data: {
#     crate: string,
#     target: string,
#     passed: int,
#     failed: int,
#     ignored: int,
#     total: int,
#     success: bool,          # true only if failed == 0
#     failures: [{
#       name: string,
#       output: string,       # captured stdout/stderr
#       panic_message: string # extracted panic message if applicable
#     }],
#     duration_secs: float
#   },
#   error: null
# }


# =============================================================================
# test/count
#
# Counts existing tests without running them.
# Call before writing new tests to establish baseline.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --target: string         # "unit" | "props" | "all"
#
# Output:
# {
#   ok: true,
#   data: {
#     crate: string,
#     unit_count: int,
#     props_count: int,
#     total: int,
#     by_file: [{
#       file: string,
#       count: int,
#       names: [string]
#     }]
#   },
#   error: null
# }


# =============================================================================
# test/verify-coverage
#
# Checks actual test coverage against the coverage plan.
# Called as the verification gate for write-tests playbook completion.
# =============================================================================
#
# Input:
#   --taskfile: path         # reads coverage_plan from current task
#   --crate-path: path
#
# Output:
# {
#   ok: true,
#   data: {
#     plan_item_count: int,
#     covered: int,
#     uncovered: int,
#     coverage_rate: float,
#     uncovered_items: [{
#       name: string,
#       type: string,
#       rationale: string     # from the coverage plan
#     }],
#     gate_passed: bool       # true if all planned tests exist and pass
#   },
#   error: null
# }

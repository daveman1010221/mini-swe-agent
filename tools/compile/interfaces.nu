# compile/ namespace — tool interface specifications
#
# Wraps cargo check and error interpretation.
# The agent never runs raw cargo commands — always goes through these tools.
# Errors are structured records, not raw compiler output.

# =============================================================================
# compile/check
#
# Runs cargo check on a crate and returns structured error/warning data.
# =============================================================================
#
# Input:
#   --workspace-root: path
#   --crate: string          # crate name e.g. "cassini-broker"
#   --package: string        # optional: specific package if workspace has multiple
#
# Output:
# {
#   ok: true,
#   data: {
#     crate: string,
#     clean: bool,
#     error_count: int,
#     warning_count: int,
#     errors: [{
#       file: string,
#       line: int,
#       col: int,
#       code: string,         # e.g. "E0308"
#       message: string,
#       hint: string,         # compiler hint if present
#       context: string       # surrounding code lines
#     }],
#     warnings: [{
#       file: string,
#       line: int,
#       message: string,
#       lint: string          # e.g. "unused_imports"
#     }]
#   },
#   error: null
# }


# =============================================================================
# compile/fix-hint
#
# Given a specific compiler error, returns structured fix guidance.
# Interprets common error codes into actionable steps.
# Does NOT make the fix — tells the agent what to do.
# =============================================================================
#
# Input:
#   --error: record          # single error record from compile/check
#   --context: string        # surrounding code for additional context
#
# Output:
# {
#   ok: true,
#   data: {
#     error_code: string,
#     pattern: string,        # recognized pattern e.g. "type-mismatch" | "missing-import"
#     fix_steps: [string],    # ordered list of what to do
#     tools_to_use: [string], # which tools to call for the fix
#     example: string         # concrete example if available
#   },
#   error: null
# }

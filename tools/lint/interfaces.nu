# lint/ namespace — tool interface specifications
#
# Wraps clippy with structured output.
# Agent never reads raw clippy output — always structured records.

# =============================================================================
# lint/check
#
# Runs clippy and returns structured warning data.
# =============================================================================
#
# Input:
#   --workspace-root: path
#   --crate: string
#   --deny-warnings: bool    # default: true (matches CI behavior)
#
# Output:
# {
#   ok: true,
#   data: {
#     crate: string,
#     clean: bool,
#     warning_count: int,
#     warnings: [{
#       file: string,
#       line: int,
#       lint: string,          # e.g. "clippy::needless_return"
#       message: string,
#       suggestion: string,    # clippy's suggested fix if available
#       machine_applicable: bool  # true if clippy can auto-fix
#     }]
#   },
#   error: null
# }


# =============================================================================
# lint/fix-hint
#
# Given a clippy warning, returns structured fix guidance.
# =============================================================================
#
# Input:
#   --warning: record        # single warning from lint/check
#
# Output:
# {
#   ok: true,
#   data: {
#     lint: string,
#     fix_type: string,       # "trivial" | "refactor" | "design"
#     auto_fixable: bool,
#     fix_steps: [string],
#     example: string
#   },
#   error: null
# }

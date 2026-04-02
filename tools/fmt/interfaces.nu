# fmt/ namespace — tool interface specifications
#
# Wraps rustfmt with structured output.
# Agent never runs rustfmt directly — always goes through these tools.

# =============================================================================
# fmt/check
#
# Checks formatting without modifying files.
# Returns list of files that would be changed.
# =============================================================================
#
# Input:
#   --workspace-root: path
#   --crate: string
#
# Output:
# {
#   ok: true,
#   data: {
#     clean: bool,
#     unformatted_files: [string]
#   },
#   error: null
# }


# =============================================================================
# fmt/apply
#
# Applies rustfmt to a crate.
# Returns list of files that were changed.
# =============================================================================
#
# Input:
#   --workspace-root: path
#   --crate: string
#
# Output:
# {
#   ok: true,
#   data: {
#     files_changed: [string],
#     count: int
#   },
#   error: null
# }

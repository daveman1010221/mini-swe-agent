# create/ namespace — tool interface specifications
#
# Tools for creating new files, modules, and test scaffolding.
# These are high-risk operations — they touch the filesystem.
# Each tool checks preconditions before creating anything.
# Agent never creates files with raw write tool if a create/ tool exists.

# =============================================================================
# create/test-file
#
# Creates a new test file for a crate with correct scaffolding.
# Checks that tests/ dir exists, Cargo.toml has the [[test]] entry,
# and the file doesn't already exist before creating anything.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --target: string         # "unit" | "props"
#   --imports: [string]      # crate items to import e.g. ["MyActor", "MyMsg"]
#
# Output:
# {
#   ok: true,
#   data: {
#     created: bool,
#     file: string,
#     already_existed: bool,
#     cargo_toml_updated: bool,
#     scaffold: string        # the content written — agent appends to this
#   },
#   error: null              # error if preconditions fail
# }
#
# Preconditions checked (in order):
#   1. crate-path exists
#   2. tests/ dir exists — create if not
#   3. [[test]] entry in Cargo.toml — add if missing
#   4. file does not already exist (never overwrite)
#
# On precondition failure:
# {
#   ok: false,
#   data: {
#     failed_precondition: string,
#     attempted_fix: bool,
#     fix_result: string
#   },
#   error: "precondition failed: ..."
# }


# =============================================================================
# create/tests-dir
#
# Creates the tests/ directory structure for a crate that has none.
# =============================================================================
#
# Input:
#   --crate-path: path
#
# Output:
# {
#   ok: true,
#   data: {
#     created: bool,
#     already_existed: bool,
#     path: string
#   },
#   error: null
# }


# =============================================================================
# create/cargo-test-entry
#
# Adds a [[test]] entry to a crate's Cargo.toml.
# Checks for existing entry before adding.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --name: string           # test binary name e.g. "unit"
#   --path: string           # e.g. "tests/unit.rs"
#
# Output:
# {
#   ok: true,
#   data: {
#     added: bool,
#     already_existed: bool,
#     entry: string           # the TOML that was added
#   },
#   error: null
# }


# =============================================================================
# create/dev-dep
#
# Adds a dev-dependency to a crate's Cargo.toml.
# Checks workspace Cargo.toml first — prefers workspace = true form.
# Never adds a pinned version if workspace form is available.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --dep: string            # dependency name e.g. "proptest"
#   --workspace-root: path
#
# Output:
# {
#   ok: true,
#   data: {
#     added: bool,
#     already_existed: bool,
#     form: string,           # "workspace" | "versioned"
#     entry: string,          # the TOML that was added
#     workspace_available: bool  # whether workspace form was available
#   },
#   error: null
# }

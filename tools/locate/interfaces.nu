# locate/ namespace — tool interface specifications
#
# The first tools called in any ACT phase that involves reading code.
# Standing order: ALWAYS locate before you extract.
# Never read a file without knowing what you're looking for first.
#
# These tools answer "where is the thing?" not "what does it say?"
# Extract answers "what does it say?"

# =============================================================================
# locate/actors
#
# Finds all ractor Actor implementations in a crate.
# Returns enough to know what to read next — not the full source.
# =============================================================================
#
# Input:
#   --crate-path: path       # e.g. $WORKSPACE_ROOT/cassini/broker
#
# Output:
# {
#   ok: true,
#   data: {
#     crate_path: string,
#     count: int,
#     actors: [{
#       name: string,         # e.g. "TopicManager"
#       file: string,         # relative to crate root
#       line: int,
#       msg_type: string,     # e.g. "TopicManagerMessage"
#       state_type: string,   # e.g. "TopicManagerState"
#       args_type: string,    # e.g. "TopicManagerArgs"
#       has_pre_start: bool,
#       has_post_stop: bool,
#       has_handle: bool,
#       has_supervisor_evt: bool
#     }]
#   },
#   error: null
# }


# =============================================================================
# locate/symbols
#
# Finds public symbols (fns, structs, enums, traits) in a crate.
# Use before writing tests to understand the public API surface.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --kind: string           # "fn" | "struct" | "enum" | "trait" | "all"
#   --pattern: string        # optional grep pattern to narrow results
#
# Output:
# {
#   ok: true,
#   data: {
#     count: int,
#     symbols: [{
#       name: string,
#       kind: string,         # "fn" | "struct" | "enum" | "trait"
#       file: string,
#       line: int,
#       visibility: string,   # "pub" | "pub(crate)" | "private"
#       signature: string     # one-line signature
#     }]
#   },
#   error: null
# }


# =============================================================================
# locate/derives
#
# Finds all derive macros used in a crate.
# Critical for deciding what tests to write — serde? rkyv? both?
# =============================================================================
#
# Input:
#   --crate-path: path
#
# Output:
# {
#   ok: true,
#   data: {
#     has_serde: bool,
#     has_rkyv: bool,
#     has_partial_eq: bool,
#     has_debug: bool,
#     has_clone: bool,
#     types_with_serde: [string],
#     types_with_rkyv: [string],
#     types_with_both: [string],   # need both roundtrip test sets
#     all_derives: [{
#       type_name: string,
#       file: string,
#       line: int,
#       derives: [string]
#     }]
#   },
#   error: null
# }


# =============================================================================
# locate/tests
#
# Finds existing tests in a crate — what exists, what's missing.
# Always call before writing new tests to avoid duplication.
# =============================================================================
#
# Input:
#   --crate-path: path
#
# Output:
# {
#   ok: true,
#   data: {
#     has_tests_dir: bool,
#     test_files: [{
#       file: string,
#       test_count: int,
#       test_names: [string]
#     }],
#     total_tests: int,
#     has_unit: bool,
#     has_props: bool,
#     cargo_toml_declares: [string]  # [[test]] entries in Cargo.toml
#   },
#   error: null
# }


# =============================================================================
# locate/files
#
# Lists source files in a crate. Starting point when exploring unfamiliar code.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --pattern: string        # optional glob e.g. "*.rs"
#
# Output:
# {
#   ok: true,
#   data: {
#     count: int,
#     files: [{
#       path: string,         # relative to crate root
#       size_bytes: int,
#       modified: string
#     }]
#   },
#   error: null
# }


# =============================================================================
# locate/deps
#
# Finds dependencies declared in a crate's Cargo.toml.
# Call before adding dev-dependencies to check workspace vs local.
# =============================================================================
#
# Input:
#   --crate-path: path
#   --kind: string           # "all" | "deps" | "dev-deps" | "build-deps"
#
# Output:
# {
#   ok: true,
#   data: {
#     workspace_root: string,
#     deps: [{
#       name: string,
#       kind: string,          # "normal" | "dev" | "build"
#       version: string,
#       workspace: bool,       # true if {workspace = true}
#       features: [string]
#     }],
#     workspace_deps: [string] # deps available via workspace = true
#   },
#   error: null
# }

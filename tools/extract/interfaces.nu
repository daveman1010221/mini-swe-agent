# extract/ namespace — tool interface specifications
#
# Called after locate/ — locate answers "where?", extract answers "what?"
# These tools retrieve precise content with line numbers.
# The agent never reads entire files speculatively.
# Always know what you're looking for before you extract.

# =============================================================================
# extract/file
#
# Reads a complete file with line numbers.
# Use only for small files or when you need full context.
# Prefer extract/range or extract/symbol for large files.
# =============================================================================
#
# Input:
#   --file: path
#   --workspace-root: path   # for relative path display
#
# Output:
# {
#   ok: true,
#   data: {
#     file: string,
#     line_count: int,
#     size_bytes: int,
#     content: string,        # full content with line numbers
#     warning: string         # set if file > 500 lines: "large file — consider extract/range"
#   },
#   error: null
# }


# =============================================================================
# extract/range
#
# Reads a specific line range from a file.
# Use when locate/ gives you a line number — read N lines of context around it.
# =============================================================================
#
# Input:
#   --file: path
#   --start: int
#   --end: int
#   --context: int           # extra lines of context above/below (default: 5)
#
# Output:
# {
#   ok: true,
#   data: {
#     file: string,
#     start: int,
#     end: int,
#     content: string         # requested range with line numbers
#   },
#   error: null
# }


# =============================================================================
# extract/symbol
#
# Extracts the complete definition of a named symbol.
# Handles fn, struct, enum, impl blocks, trait impls.
# Finds the matching braces — you get the whole thing.
# =============================================================================
#
# Input:
#   --file: path
#   --symbol: string         # e.g. "TopicManager" or "handle"
#   --kind: string           # "fn" | "struct" | "enum" | "impl" | "any"
#
# Output:
# {
#   ok: true,
#   data: {
#     file: string,
#     symbol: string,
#     kind: string,
#     start_line: int,
#     end_line: int,
#     content: string         # complete definition with line numbers
#   },
#   error: null
# }


# =============================================================================
# extract/actor
#
# Extracts everything needed to write tests for a ractor actor.
# Combines multiple extract calls into one structured result.
# The single best tool to call before writing actor tests.
# =============================================================================
#
# Input:
#   --file: path
#   --actor: string          # actor name e.g. "TopicManager"
#
# Output:
# {
#   ok: true,
#   data: {
#     actor_name: string,
#     msg_enum: {
#       name: string,
#       variants: [{
#         name: string,
#         fields: [{name: string, type: string}],
#         is_rpc: bool,       # true if has RpcReplyPort field
#         reply_type: string  # type inside RpcReplyPort if is_rpc
#       }]
#     },
#     args_struct: {
#       name: string,
#       fields: [{name: string, type: string, visibility: string}],
#       has_private_fields: bool
#     },
#     state_struct: {
#       name: string,
#       is_private: bool      # always true — never construct directly
#     },
#     output_ports: [{
#       field_name: string,
#       event_type: string
#     }],
#     pre_start_signature: string,
#     handle_signature: string
#   },
#   error: null
# }


# =============================================================================
# extract/cargo-toml
#
# Extracts structured data from a crate's Cargo.toml.
# Use before modifying dependencies or adding test targets.
# =============================================================================
#
# Input:
#   --crate-path: path
#
# Output:
# {
#   ok: true,
#   data: {
#     name: string,
#     version: string,
#     edition: string,
#     lib: {exists: bool, path: string},
#     bins: [{name: string, path: string}],
#     tests: [{name: string, path: string}],
#     deps: record,           # [dependencies] section as record
#     dev_deps: record,       # [dev-dependencies] section as record
#     features: record        # [features] section as record
#   },
#   error: null
# }

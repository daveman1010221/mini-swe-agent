# tools/ namespace — tool interface specifications
#
# The self-describing discovery layer. The agent uses these to find
# the right tool for the job without having to memorize the taxonomy.
#
# Every tool in the toolbox registers its own metadata.
# The agent's standing orders say: when you need to do something,
# search the toolbox first. Never freestyle.
#
# Tool metadata lives in:
#   mswea/tools/registry.nu
#
# Each tool entry in the registry follows the ToolSpec record shape below.

# =============================================================================
# ToolSpec — the shape every tool registers with
# =============================================================================
#
# {
#   name: string,           # e.g. "loop-detect"
#   namespace: string,      # e.g. "meta"
#   full_name: string,      # e.g. "meta/loop-detect"
#   description: string,    # what it does
#   when_to_use: string,    # explicit guidance on when to reach for this
#   ooda_phase: string,     # "observe" | "orient" | "decide" | "act" | "any"
#   inputs: [{
#     name: string,
#     type: string,
#     required: bool,
#     default: any,
#     description: string
#   }],
#   output_shape: string,   # human-readable description of output structure
#   tags: [string],         # for fuzzy search e.g. ["loop", "safety", "detect", "cycle"]
#   playbook_steps: [string] # which playbook steps commonly use this tool
# }


# =============================================================================
# tools/search
#
# Fuzzy search across tool metadata. The agent's primary discovery mechanism.
# Searches name, description, when_to_use, and tags.
# Returns ranked candidates.
# =============================================================================
#
# Input:
#   --query: string          # natural language e.g. "find actors in a crate"
#   --namespace: string      # optional filter e.g. "locate"
#   --ooda-phase: string     # optional filter e.g. "observe"
#   --top: int               # max results (default: 5)
#
# Output:
# {
#   ok: true,
#   data: {
#     query: string,
#     count: int,
#     results: [{
#       full_name: string,
#       score: float,          # relevance 0.0-1.0
#       description: string,
#       when_to_use: string,
#       ooda_phase: string
#     }]
#   },
#   error: null
# }


# =============================================================================
# tools/describe
#
# Returns full ToolSpec for a named tool.
# Agent calls this after tools/search to get exact input/output contract.
# =============================================================================
#
# Input:
#   --tool: string           # full name e.g. "meta/loop-detect"
#
# Output:
# {
#   ok: true,
#   data: ToolSpec,          # complete spec including inputs and output shape
#   error: null
# }
#
# On not found:
# {
#   ok: false,
#   data: null,
#   error: "unknown tool: meta/loop-detect — run tools/list to see available tools"
# }


# =============================================================================
# tools/list
#
# Lists all registered tools, optionally filtered by namespace or OODA phase.
# =============================================================================
#
# Input:
#   --namespace: string      # optional filter
#   --ooda-phase: string     # optional filter
#
# Output:
# {
#   ok: true,
#   data: {
#     count: int,
#     namespaces: [string],
#     tools: [{
#       full_name: string,
#       namespace: string,
#       description: string,
#       ooda_phase: string,
#       tags: [string]
#     }]
#   },
#   error: null
# }


# =============================================================================
# tools/check-approved
#
# Verifies that a tool is in the approved list for the current playbook step.
# Agent calls this in DECIDE phase before selecting an action.
# Prevents off-playbook tool use.
# =============================================================================
#
# Input:
#   --tool: string           # full name e.g. "compile/check"
#   --taskfile: path
#
# Output:
# {
#   ok: true,
#   data: {
#     approved: bool,
#     tool: string,
#     current_step: string,
#     approved_tools: [string],
#     reason: string          # why approved or not
#   },
#   error: null
# }


# =============================================================================
# tools/register
#
# Adds a new tool to the registry. Used during toolbox development,
# not by the agent at runtime.
# =============================================================================
#
# Input:
#   --spec: ToolSpec
#
# Output:
# {
#   ok: true,
#   data: {
#     registered: true,
#     full_name: string
#   },
#   error: null
# }

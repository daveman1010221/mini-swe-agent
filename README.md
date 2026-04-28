# mini-swe-agent

An autonomous coding agent built in Rust, running inside an embedded [Nushell](https://www.nushell.sh/) environment, with a structured OODA loop, a 36-tool nushell toolbox, playbook-driven task execution, and a trajectory-aware loop detection system.

## What it does

`mswea` takes a task file, spins up an LLM-powered agent loop, and lets the model use a structured toolbox to accomplish multi-step coding tasks. Every step is recorded to a JSON Lines trajectory file for inspection and analysis. The agent follows an explicit OODA (Observe → Orient → Decide → Act) loop enforced by the system prompt, with a playbook system that governs what tools are available at each step.

```
mswea --task-file agent-task.json --output /tmp/trajectory
```

## Architecture

The agent is built as a ractor actor system with five live actors:

```
EventLoggerActor   ← subscribes to OutputPort<Event>, writes JSONL trajectory
OrchestratorActor  ← maintains CapabilityMap, generates system prompt
ToolRouterActor    ← receives ToolCall via RPC, dispatches to handlers
ModelActor         ← wraps LitellmClient, handles retry with backon
ToolboxActor       ← scans tools/, validates flags, runs preflight survey
```

```
agent loop (main.rs)
    │
    ├─ read system_prompt from Arc<RwLock<String>>  (written by OrchestratorActor)
    ├─ call ModelActor.handle(messages)              (→ ToolCall)
    └─ call_t!(ToolRouterActor, RouteRequest)        (→ Observation)
                │
                ├─ Shell       → ShellWorker (dedicated thread, embedded nu 0.111)
                ├─ Read        → std::fs
                ├─ Write       → std::fs
                ├─ Edit        → exact-match-once replace
                ├─ Search      → rg subprocess
                └─ NushellTool → ToolRegistry lookup → ShellWorker (nu <script> <flags>)
```

All actors emit `Event` values to a shared `OutputPort<Event>` (ractor dport).
`EventLoggerActor` subscribes and writes every event as a JSON line.

## Workspace layout

```
crates/
├── core/          # mswea-core — types, errors, config, events, capabilities, toolbox types
├── models/        # LitellmClient, ModelActor, SSE streaming, ToolCall extraction
├── environments/  # NushellSession, ShellWorker, file_ops, rg search
├── actors/        # EventLoggerActor, OrchestratorActor, ToolRouterActor, ToolboxActor, EventBus
└── cli/           # clap arg parsing, config loading, agent loop, wiring

tools/             # Nushell toolbox — 36 tools across 9 namespaces
├── compile/       # check, fix-hint
├── create/        # tests-dir, test-file, cargo-test-entry, dev-dep
├── extract/       # file, range, symbol, actor, cargo-toml
├── fmt/           # apply, check
├── lint/          # check
├── locate/        # files, symbols, actors, derives, tests, deps
├── meta/          # loop-detect, trajectory-summary, step-verify, help
├── playbook/      # lookup, current-step
└── task/          # state, advance, halt, block, next, write-coverage-plan, record-attempt

skills/            # Agent skills injected into system prompt
playbooks/         # Task playbooks (write-tests, etc.)
```

## Requirements

- Rust (nightly)
- [Nushell](https://www.nushell.sh/) 0.111 (embedded via `nu-engine`/`nu-protocol`/`nu-command` crates)
- [ripgrep](https://github.com/BurntSushi/ripgrep) (`rg`) on PATH
- [fd](https://github.com/sharkdp/fd) on PATH
- An OpenAI-compatible LLM endpoint

## Quick start

```sh
# Clone and build
git clone https://github.com/daveman1010221/mini-swe-agent
cd mini-swe-agent
cargo build --release

# Point at your LLM endpoint
export OPENAI_BASE_URL=http://localhost:8080/v1
export OPENAI_API_KEY=sk-your-key

# Run a task
export TASKFILE=/workspace/agent-task.json
./target/release/mswea --task-file agent-task.json --output /tmp/trajectory

# Or use the Justfile (inside dev container)
just agent-run
```

## Task file format

The agent is driven by a JSON task file rather than a free-form task string. This enables multi-step playbook execution with state tracking across runs.

```json
{
  "workspace_root": "/workspace",
  "current_task": {
    "crate": "mswea-core",
    "crate_path": "crates/core",
    "op": "write-tests",
    "step": "survey",
    "step_index": 0,
    "step_budget": 3
  },
  "pending": [],
  "completed": []
}
```

## OODA Loop

The agent operates an explicit four-phase loop enforced by the system prompt:

1. **OBSERVE** — call `task/state`, `meta/loop-detect`, `meta/trajectory-summary` (mandatory, one per turn)
2. **ORIENT** — reason from observations about current step validity and loop detection
3. **DECIDE** — continue, try alternate approach, or halt
4. **ACT** — execute exactly one approved tool, then return to OBSERVE

Submitting before task completion is a protocol violation. The system prompt enforces hard rules for each playbook step (e.g. write-tests: one test per ACT, compile/check after each test).

## Nushell Toolbox

Tools are `.nu` scripts in `tools/*/`. The `ToolboxActor` scans them at startup, parses flag signatures from `def main [...]`, and validates agent tool calls against the flag metadata before invoking nushell — wrong flag names and type mismatches return actionable errors without triggering stack resets.

Tools follow a standard return convention:

```nushell
{ ok: bool, data: record | null, error: string | null }
```

## Configuration

```
mswea --help

Options:
      --task-file <PATH>   Path to JSON task file     [env: TASKFILE]
      --task <TEXT>        Inline task (alternative to --task-file)
  -c, --config <PATH>      Path to YAML config file
      --model <NAME>       LLM model name
      --step-limit <N>     Maximum steps (default: 50)
      --cost-limit <USD>   Cost ceiling in USD (default: 3.0)
      --output <PATH>      Output path for trajectory (.jsonl appended)
      --cwd <DIR>          Working directory for shell commands
  -v, --verbose            Increase log verbosity
```

## Trajectory format

Each run produces a `.jsonl` file — one JSON object per line per event.

```jsonl
{"id":"01KM...","timestamp_ms":1774845829,"actor_id":"orchestrator","kind":{"kind":"system_prompt_regenerated","prompt_len":19628}}
{"id":"01KM...","timestamp_ms":1774845830,"actor_id":"agent","kind":{"kind":"agent_step","step":1,"cost_so_far":0.0}}
{"id":"01KM...","timestamp_ms":1774845830,"actor_id":"tool-router","kind":{"kind":"shell_command_started","command":"nu /workspace/tools/task/state.nu","cwd":"/workspace"}}
```

Event IDs are [ULIDs](https://github.com/ulid/spec) — sortable and time-stamped.

## Local LLM setup

The Justfile includes recipes for running llama-server with recommended models:

```sh
# Qwen3-Coder-30B on AMD ROCm (RX 9070 XT)
just llm-qwen

# OmniCoder-9B on AMD ROCm (16GB VRAM)
just llm-omnicoder-rocm-16GB

# OmniCoder-9B on NVIDIA (12GB VRAM)
just llm-omnicoder-nvidia-12GB
```

## Dev container

A Nix-built Podman dev container is included with nushell 0.111, ripgrep, fd, cargo nightly, neovim, and a full CI pipeline.

```sh
just build-dev-container   # build the container image
just run-dev-container     # start with SSH on port 2222
just build                 # cargo build --workspace (inside container)
just agent-clean           # remove trajectory files and agent-written tests
just agent-run             # clean + build-release + run agent
just lint                  # cargo clippy -D warnings
just test                  # cargo test --workspace
```

## Status

| Component | Status |
|-----------|--------|
| CLI + config loading | ✅ |
| LitellmClient (SSE streaming) | ✅ |
| ModelActor (retry, events) | ✅ |
| Embedded nushell (nu 0.111) | ✅ |
| File ops + rg search | ✅ |
| EventLoggerActor (JSONL) | ✅ |
| OrchestratorActor (CapabilityMap) | ✅ |
| ToolRouterActor (ractor RPC) | ✅ |
| ToolboxActor (flag validation, preflight) | ✅ |
| Nushell toolbox (36 tools) | ✅ |
| OODA loop enforcement | ✅ |
| Playbook system | ✅ |
| Loop detection | ✅ |
| Task state tracking | ✅ |
| nushell-as-library tool dispatch | 🔲 |
| rkyv trajectory archive | 🔲 |
| Batch mode | 🔲 |

## Origin

The name and initial concept were loosely inspired by [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) by the SWE-agent team, but this project shares no code or architecture with the original. It is an independent Rust implementation built around ractor actors, an embedded nushell engine, playbook-driven task execution, OODA loop enforcement, and a 36-tool nushell toolbox.

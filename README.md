# mini-swe-agent

A Rust port of [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) — an autonomous coding agent that runs inside an embedded [Nushell](https://www.nushell.sh/) environment.

## What it does

`mswea` takes a task description, spins up an LLM-powered agent loop, and lets the model use a set of tools (shell execution, file read/write/edit, ripgrep search) to accomplish the task. Every step is recorded to a JSON Lines trajectory file for inspection and analysis.

```
mswea --task "add error handling to src/lib.rs" --output trajectory
```

## Architecture

The agent is built as a ractor actor system with four live actors:

```
EventLoggerActor   ← subscribes to OutputPort<Event>, writes JSONL trajectory
OrchestratorActor  ← maintains CapabilityMap, generates system prompt
ToolRouterActor    ← receives ToolCall via RPC, dispatches to handlers
ModelActor         ← wraps LitellmClient, handles retry with backon
```

```
agent loop (main.rs)
    │
    ├─ read system_prompt from Arc<RwLock<String>>  (written by OrchestratorActor)
    ├─ call ModelActor.handle(messages)              (→ ToolCall)
    └─ call_t!(ToolRouterActor, RouteRequest)        (→ Observation)
                │
                ├─ Shell   → ShellWorker (dedicated thread, embedded nu 0.111)
                ├─ Read    → std::fs
                ├─ Write   → std::fs
                ├─ Edit    → exact-match-once replace
                └─ Search  → rg subprocess
```

All actors emit `Event` values to a shared `OutputPort<Event>` (ractor dport).
`EventLoggerActor` subscribes and writes every event as a JSON line.

## Workspace layout

```
crates/
├── core/          # mswea-core — types, errors, config, events, capabilities
├── models/        # LitellmClient, ModelActor, SSE streaming, ToolCall extraction
├── environments/  # NushellSession, ShellWorker, file_ops, rg search
├── actors/        # EventLoggerActor, OrchestratorActor, ToolRouterActor, EventBus
└── cli/           # clap arg parsing, config loading, agent loop, wiring
```

## Requirements

- Rust (nightly, recent stable should also work)
- [Nushell](https://www.nushell.sh/) 0.111 (used as embedded engine via `nu-engine`/`nu-protocol`/`nu-command` crates)
- [ripgrep](https://github.com/BurntSushi/ripgrep) (`rg`) on PATH for the search tool
- An OpenAI-compatible LLM endpoint

## Quick start

```sh
# Clone and build
git clone https://github.com/daveman1010221/mini-swe-agent
cd mini-swe-agent
cargo build --release

# Point at your LLM endpoint
export OPENAI_BASE_URL=http://localhost:8080/v1   # llama-server, LiteLLM, OpenRouter, etc.
export OPENAI_API_KEY=sk-your-key                 # any value for local endpoints

# Run a task
./target/release/mswea --task "list the Rust source files in this repo"

# With trajectory logging
./target/release/mswea \
  --task "fix the TODO in src/lib.rs" \
  --output /tmp/trajectory

# Inspect the trajectory
cat /tmp/trajectory.jsonl | jq '.kind | to_entries[0].key'
```

## Configuration

All flags can also be set via environment variables or a YAML config file.

```
mswea --help

Options:
  -t, --task <TEXT>        Task for the agent to solve  [env: MSWEA_TASK]
  -c, --config <PATH>      Path to YAML config file     [env: MSWEA_CONFIG]
      --model <NAME>       LLM model name               [env: MSWEA_MODEL]
      --step-limit <N>     Maximum steps (default: 50)
      --cost-limit <USD>   Cost ceiling in USD (default: 3.0)
      --output <PATH>      Output path for trajectory (.jsonl appended)
      --cwd <DIR>          Working directory for shell commands
  -v, --verbose            Increase log verbosity (-v = debug, -vv = trace)
      --json-logs          Emit structured JSON logs
```

Config file (YAML) — CLI flags override:

```yaml
agent:
  step_limit: 50
  cost_limit: 3.0
  system_template: templates/system.j2
  instance_template: templates/instance.j2

model:
  model_name: claude-sonnet-4-5
  backend: litellm

shell:
  cwd: /workspace
  timeout_secs: 30
```

## Tools

The agent has access to these tools, described automatically from the `CapabilityMap`:

| Tool | Description |
|------|-------------|
| `shell` | Execute a nushell command. Returns structured data (lists, records, tables). |
| `read` | Read the full content of a file. |
| `write` | Write content to a file, creating or overwriting it. |
| `edit` | Replace an exact string in a file. Fails if the string isn't found exactly once. |
| `search` | Search for a pattern using ripgrep. Returns structured match results. |
| `submit` | Mark the task complete and return the final answer. |

## Trajectory format

Each run with `--output` produces a `.jsonl` file — one JSON object per line, one event per line.

```jsonl
{"id":"01KM...","timestamp_ms":1774845829,"actor_id":"orchestrator","kind":{"kind":"system_prompt_regenerated","prompt_len":2456}}
{"id":"01KM...","timestamp_ms":1774845830,"actor_id":"agent","kind":{"kind":"agent_step","step":1,"cost_so_far":0.0}}
{"id":"01KM...","timestamp_ms":1774845830,"actor_id":"tool-router","kind":{"kind":"shell_command_started","command":"ls /workspace","cwd":"/"}}
{"id":"01KM...","timestamp_ms":1774845830,"actor_id":"agent","kind":{"kind":"model_response_received","tokens_in":699,"tokens_out":43,"cost_usd":0.0,"latency_ms":738}}
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

Then point the agent at it:

```sh
export OPENAI_BASE_URL=http://localhost:8080/v1
export OPENAI_API_KEY=sk-local
mswea --task "your task here"
```

## Dev container

A Nix-built Podman dev container is included with nushell 0.111, ripgrep, cargo nightly, neovim, and a full CI pipeline (fmt → clippy → static-analysis → test).

```sh
just build-dev-container   # build the container image
just run-dev-container     # start with SSH on port 2222
just build                 # cargo build --workspace (inside container)
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
| minijinja prompt templates | 🔲 |
| rkyv trajectory archive | 🔲 |
| Batch mode | 🔲 |

## Origin

This is a Rust port of [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) by the SWE-agent team. The original is a minimal Python implementation. This port uses idiomatic Rust throughout: typed errors, rkyv for zero-copy serialization, ractor for the actor system, and an embedded nushell engine for structured shell output.

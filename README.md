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

# mini-swe-agent Justfile
#
# Host-side recipes: build, run containers, dev workflow
# Container-side recipes: start llama-server, start pi, agent task management
#
# Usage:
#   just <recipe>                         # run with defaults
#   just platform=aarch64-linux <recipe>  # override platform
#
# Platform autodetects from uname.

platform := `uname -m | sed 's/x86_64/x86_64-linux/;s/aarch64/aarch64-linux/'`

_taskfile := "/workspace/agent-task.json"

# ── Help ──────────────────────────────────────────────────────────────────────

# List all available recipes
default:
    @just --list

# ── Build ─────────────────────────────────────────────────────────────────────

# Build all workspace crates (debug)
build:
    cargo build --workspace
    ln -sf /var/cache/cargo-target/ /workspace/target

# Build all workspace crates (release)
build-release:
    cargo build --workspace --release
    ln -sf /var/cache/cargo-target/ /workspace/target

# Build a specific crate
# Usage: just build-crate mswea-core
build-crate crate:
    cargo build --package {{crate}}

# Build and load the dev container image
build-dev-container:
    nix build -L ".#devContainer" --system {{platform}} -o result-mswea-{{platform}}
    podman load -i result-mswea-{{platform}}
    podman tag localhost/mswea-dev:latest localhost/mswea-dev-{{platform}}:latest

# ── Run containers ────────────────────────────────────────────────────────────

# Start the agent container (AMD ROCm — RX 9070 XT)
run-rocm:
    podman run --rm -it --privileged --name mswea-agent \
        --user 0 --userns=keep-id \
        --device /dev/kfd \
        --device /dev/dri/ \
        --ulimit memlock=-1:-1 \
        --security-opt=label=disable \
        --cap-add=SYS_PTRACE \
        --ipc=host \
        --network=host \
        -v /sys/class/kfd:/sys/class/kfd:ro \
        -v /sys/bus/pci/devices:/sys/bus/pci/devices:ro \
        -e CREATE_USER="$USER" \
        -e CREATE_UID="$(id -u)" \
        -e CREATE_GID="$(id -g)" \
        -e ATUIN_SESSION_NAME=mswea-dev \
        -e HIP_VISIBLE_DEVICES=0 \
        -v ~/Documents/projects/ai_models/llama:/opt/llama-models:rw \
        -v ~/Documents/projects/ai_models/ollama:/opt/ollama:rw \
        -v ~/Documents/projects/ai_state/pi:/opt/pi:rw \
        -v $PWD:/workspace \
        -v $HOME/.config/atuin:/atuin-config:ro \
        -v $HOME/.local/share/atuin:/atuin-data \
        -e ATUIN_CONFIG_DIR=/atuin-config \
        -e ATUIN_DATA_DIR=/atuin-data \
        localhost/mswea-agent:latest

# Start the agent container (NVIDIA CUDA — RTX 4000 Ada)
run-nvidia:
    podman run --rm -it --privileged --name mswea-agent \
        --user 0 --userns=keep-id \
        --device /dev/nvidia0 \
        --device /dev/nvidiactl \
        --device /dev/nvidia-uvm \
        --device /dev/nvidia-uvm-tools \
        --ulimit memlock=-1:-1 \
        --security-opt=label=disable \
        --cap-add=SYS_PTRACE \
        --ipc=host \
        --network=host \
        -e NVIDIA_VISIBLE_DEVICES=all \
        -e NVIDIA_DRIVER_CAPABILITIES=compute,utility \
        -e CREATE_USER="$USER" \
        -e CREATE_UID="$(id -u)" \
        -e CREATE_GID="$(id -g)" \
        -e ATUIN_SESSION_NAME=mswea-dev \
        -e LD_LIBRARY_PATH=/run/opengl-driver/lib \
        -v ~/Documents/projects/ai_models/llama:/opt/llama-models:rw \
        -v ~/Documents/projects/ai_models/ollama:/opt/ollama:rw \
        -v ~/Documents/projects/ai_state/pi:/opt/pi:rw \
        -v $PWD:/workspace \
        -v $HOME/.config/atuin:/atuin-config:ro \
        -v $HOME/.local/share/atuin:/atuin-data \
        -e ATUIN_CONFIG_DIR=/atuin-config \
        -e ATUIN_DATA_DIR=/atuin-data \
        -v `readlink -f /run/opengl-driver/lib/libcuda.so.1`:/run/opengl-driver/lib/libcuda.so.1:ro \
        -v `readlink -f /run/opengl-driver/lib/libcuda.so`:/run/opengl-driver/lib/libcuda.so:ro \
        -v `readlink -f /run/opengl-driver/lib/libnvidia-ml.so.1`:/run/opengl-driver/lib/libnvidia-ml.so.1:ro \
        localhost/mswea-agent:latest

# Start the dev container
# SSH is available via Dropbear on port 2223 (mapped to host 2222).
# To enable autostart: export DROPBEAR_ENABLE=1 and AUTHORIZED_KEYS_B64=$(base64 -w0 ~/.ssh/authorized_keys)
run-dev-container:
    podman run --rm --name mswea-dev -it \
        --user 0 --userns=keep-id \
        --network=host \
        -e CREATE_USER="$USER" \
        -e CREATE_UID="$(id -u)" \
        -e CREATE_GID="$(id -g)" \
        -e ATUIN_SESSION_NAME=mswea-dev \
        -e DROPBEAR_ENABLE="${DROPBEAR_ENABLE:-0}" \
        -e CONTAINER_NAME=mswea-dev \
        -e DROPBEAR_PORT=2223 \
        -e AUTHORIZED_KEYS_B64="${AUTHORIZED_KEYS_B64:-}" \
        -v $PWD:/workspace \
        -v $HOME/.config/atuin:/atuin-config:ro \
        -v $HOME/.local/share/atuin:/atuin-data \
        -e ATUIN_CONFIG_DIR=/atuin-config \
        -e ATUIN_DATA_DIR=/atuin-data \
        -p 2222:2223 \
        localhost/mswea-dev-{{platform}}:latest

# ── Inside container: LLM server ──────────────────────────────────────────────

# Start Qwen3-Coder-30B (AMD ROCm) — recommended
llm-qwen:
    llama-server \
        --hf-repo unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF \
        --hf-file Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf \
        --ctx-size 65536 \
        --n-gpu-layers 32 \
        --flash-attn on \
        --alias "local-model" \
        --port 8080 --host 0.0.0.0 \
        --temperature 0.7 \
        --top-p 0.8 \
        --top-k 20 \
        --repeat-penalty 1.05 \
        --jinja \
        -ub 512 \
        --threads 20 \
        --threads-batch 20 \
        -ctk q4_0 -ctv q4_0 > /tmp/llama-server.log 2>&1

# Start OmniCoder-9B (AMD ROCm) — fallback for lighter tasks
llm-omnicoder-rocm-16GB:
    llama-server \
        --hf-repo Tesslate/OmniCoder-9B-GGUF \
        --hf-file omnicoder-9b-q4_k_m.gguf \
        --ctx-size 131072 \
        --n-gpu-layers 99 \
        --flash-attn on \
        --alias "local-model" \
        --port 8080 --host 0.0.0.0 \
        --temperature 0.3 \
        --top-p 0.95 \
        --top-k 20 \
        --min-p 0.0 \
        --repeat-penalty 1.0 \
        --jinja \
        -ub 512 \
        -ctk q4_0 -ctv q4_0 > /tmp/llama-server.log 2>&1

# Start OmniCoder-9B (NVIDIA) — fallback for lighter tasks
llm-omnicoder-nvidia-12GB:
    llama-server \
        --hf-repo Tesslate/OmniCoder-9B-GGUF \
        --hf-file omnicoder-9b-q4_k_m.gguf \
        --ctx-size 65536 \
        --n-gpu-layers 99 \
        --flash-attn on \
        --alias "local-model" \
        --port 8080 --host 0.0.0.0 \
        --temperature 0.3 \
        --top-p 0.95 \
        --top-k 20 \
        --min-p 0.0 \
        --repeat-penalty 1.0 \
        --jinja \
        -ub 512 \
        -ctk q4_0 -ctv q4_0 > /tmp/llama-server.log 2>&1

# ── Inside container: Pi agent ────────────────────────────────────────────────

pi_version := `curl -fsSL https://api.github.com/repos/badlogic/pi-mono/releases/latest | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/'`

# Download/update pi to latest version
pi-install:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION="{{pi_version}}"
    INSTALLED=""
    if [ -f ~/pi/pi ]; then
        INSTALLED=$(~/pi/pi --version 2>/dev/null | grep -oP '\d+\.\d+\.\d+' || echo "")
    fi
    if [ "$INSTALLED" = "$VERSION" ]; then
        echo "pi v${VERSION} already installed, skipping."
        exit 0
    fi
    echo "Installing pi v${VERSION}..."
    curl -fsSL "https://github.com/badlogic/pi-mono/releases/download/v${VERSION}/pi-linux-x64.tar.gz" \
        | tar -xz -C ~
    echo "pi v${VERSION} installed."

# Launch pi (installs/updates first)
pi: pi-install
    ~/pi/pi @/workspace/agent-task.json

# ── Agent task management (container-side) ────────────────────────────────────

# Show current agent task status
agent-status:
    #!/usr/bin/env bash
    if [ ! -f {{_taskfile}} ]; then
        echo "No task file found at {{_taskfile}}"
        exit 1
    fi
    echo "=== Current Task ==="
    jq '.current_task' {{_taskfile}}
    echo ""
    echo "=== Pending ($(jq '.pending | length' {{_taskfile}}) tasks) ==="
    jq '.pending[] | {crate, op, review}' {{_taskfile}}
    echo ""
    echo "=== Completed ($(jq '.completed | length' {{_taskfile}}) tasks) ==="
    jq '.completed[] | {crate, op, status}' {{_taskfile}}

# Resume agent from existing task file — prints the resume prompt to paste into pi
agent-resume:
    #!/usr/bin/env bash
    if [ ! -f {{_taskfile}} ]; then
        echo "No task file found at {{_taskfile}}"
        exit 1
    fi
    WORKSPACE_ROOT=$(jq -r '.workspace_root' {{_taskfile}})
    CRATE=$(jq -r '.current_task.crate' {{_taskfile}})
    OP=$(jq -r '.current_task.op' {{_taskfile}})
    echo "Paste this into pi to resume:"
    echo "────────────────────────────────────────────────────────────────"
    printf "export TASKFILE=%s WORKSPACE_ROOT=%s\n" "{{_taskfile}}" "$WORKSPACE_ROOT"
    printf "Read your task file: jq '.' \$TASKFILE\n"
    printf "Your current task is .current_task — crate=%s op=%s\n" "$CRATE" "$OP"
    printf "Your tools are in .tools — substitute {placeholders} as needed. Use locate tools first to find symbols, then extract tools to read exact definitions.\n"
    printf "Begin: cd \$WORKSPACE_ROOT && cargo check --package %s 2>&1 | head -80\n" "$CRATE"
    printf "Follow .current_task.next_action. Update .current_task.current_file as you move between files. When .current_task.success_condition is met run the taskfile.mark_done tool to advance to the next pending task. Do not ask for clarification. Start working.\n"
    echo "────────────────────────────────────────────────────────────────"

# Set agent to work on a specific crate and op, or pop next pending task
# Usage: just agent-task                             # pop next pending, launch pi
#        just agent-task mswea-core fix-rkyv-derives # queue explicit task, launch pi
agent-task crate='' op='':
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f {{_taskfile}} ]; then
        echo "No task file found at {{_taskfile}}"
        exit 1
    fi

    if [ -z "{{crate}}" ]; then
        PENDING_LEN=$(jq '.pending | length' {{_taskfile}})
        if [ "$PENDING_LEN" -eq 0 ]; then
            echo "No pending tasks. Add tasks to pending in {{_taskfile}} first."
            exit 1
        fi
        jq '
            .current_task = .pending[0] |
            .pending = .pending[1:]
        ' {{_taskfile}} > {{_taskfile}}.tmp && mv {{_taskfile}}.tmp {{_taskfile}}
    else
        jq --arg crate "{{crate}}" --arg op "{{op}}" '
            if .current_task != null then
                .pending = .pending + [.current_task]
            else
                .
            end |
            .current_task = {
                "crate": $crate,
                "op": $op,
                "status": "in-progress"
            }
        ' {{_taskfile}} > {{_taskfile}}.tmp && mv {{_taskfile}}.tmp {{_taskfile}}
    fi

    echo "=== Current Task ==="
    jq '.current_task' {{_taskfile}}
    echo ""
    just pi

# ── Dev workflow ──────────────────────────────────────────────────────────────

# Check all crates compile
check:
    cargo check --workspace

# Check a single crate
# Usage: just check-crate mswea-core
check-crate crate:
    cargo check --package {{crate}}

# Run tests — optionally scoped to a single package
# Usage: just test
#        just test mswea-core
test package='':
    #!/usr/bin/env bash
    if [ -n "{{package}}" ]; then
        cargo test --package {{package}} -- --nocapture
    else
        cargo test --workspace -- --nocapture
    fi

# Run clippy with warnings as errors
lint:
    cargo clippy --workspace -- -D warnings

# Format all crates
fmt:
    cargo fmt --all

# Run CI pipeline locally
ci:
    podman run --rm \
        --name mswea-dev \
        --env-file=ci.env \
        --env CI_COMMIT_REF_NAME=$(git symbolic-ref --short HEAD) \
        --env CI_COMMIT_SHORT_SHA=$(git rev-parse --short=8 HEAD) \
        --user 0 \
        --userns=keep-id \
        -v $(pwd):/workspace:rw \
        -p 2222:2223 \
        localhost/mswea-dev-{{platform}}:latest \
        bash -c "cargo check --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings"

# ── Dhall rendering ────────────────────────────────────────────────────────────
#
# container.dhall is the authoring format (typed, composable).
# container.nix is the pre-rendered output that Nix imports at build time.
#
# The Nix sandbox has no network access, so dhall-to-nix cannot run inside
# a nix build. We pre-render here and commit container.nix so the build works.
#
# Run this after every edit to src/flake/container.dhall.

# Render src/flake/container.dhall → src/flake/container.nix
render-container:
    @echo "Rendering src/flake/container.dhall → src/flake/container.nix..."
    @echo "(requires dhall-nix: nix shell nixpkgs#dhall-nix)"
    dhall-to-nix < src/flake/container.dhall > src/flake/container.nix
    @echo "Done. Commit src/flake/container.nix alongside src/flake/container.dhall."

# Type-check the container config without rendering
check-container-dhall:
    @echo "Type-checking src/flake/container.dhall..."
    dhall type < src/flake/container.dhall
    @echo "OK."


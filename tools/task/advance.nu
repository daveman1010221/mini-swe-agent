#!/usr/bin/env nu
# task/advance.nu
#
# Called when a verification gate passes. Moves to the next playbook step.
# Resets step_attempts to 0. If on last step, marks task complete.
#
# Usage:
#   nu tools/task/advance.nu --verification "cargo test: 12 passed, 0 failed"

def main [
    --verification: string   # evidence that the gate passed
    --taskfile: path = ""    # ignored — kept for backwards compat during migration
] {
    if ($verification | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --verification" }
    }

    let base = if ("MSWEA_RPC_BASE" in $env) {
        $env.MSWEA_RPC_BASE
    } else {
        "http://127.0.0.1:8000"
    }

    let result = (
        try {
            http post $"($base)/task/advance" ({verification: $verification} | to json) --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: $"TaskActor RPC failed: ($err.msg)" }
        }
    )

    # Normalize to the envelope format the agent expects
    {
        ok: $result.ok,
        data: {
            advanced: $result.advanced,
            previous_step: $result.previous_step,
            current_step: $result.current_step,
            task_completed: $result.task_completed,
        },
        error: $result.error
    }
}

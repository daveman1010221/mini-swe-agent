#!/usr/bin/env nu
# task/block.nu
#
# Called when a task cannot be completed due to external dependency,
# missing infrastructure, or explicit out-of-scope constraint.
# Different from halt — block is a known, named reason.
# Halt is unexpected. Block is a deliberate decision.
#
# Usage:
#   nu tools/task/block.nu \
#     --taskfile /workspace/agent-task.json \
#     --crate cassini-types \
#     --reason "crate has compile errors unrelated to test writing — needs fix first"

def main [
    --taskfile: path = "",
    --crate: string,
    --reason: string
] {
    let tf_path = if ($taskfile | str length) > 0 {
        $taskfile
    } else if ("TASKFILE" in $env) {
        $env.TASKFILE
    } else {
        ""
    }

    if ($tf_path | str length) == 0 {
        return { ok: false, data: null, error: "no taskfile path" }
    }

    if ($crate | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --crate" }
    }

    if ($reason | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --reason" }
    }

    let tf = (
        try { open $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let now = (date now | format date "%Y-%m-%dT%H:%M:%SZ")

    let block_entry = {
        crate: $crate,
        reason: $reason,
        surfaced_at: $now,
    }

    # Move current task to blocked if it matches
    let current = ($tf | get current_task? | default null)
    let updated_current = if $current != null and ($current | get crate? | default "") == $crate {
        null  # clear current task
    } else {
        $current
    }

    let updated_tf = $tf
        | upsert current_task $updated_current
        | upsert blocked ($tf | get blocked? | default [] | append $block_entry)
        | upsert last_updated $now

    try {
        $updated_tf | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            blocked: true,
            crate: $crate,
            reason: $reason,
            surfaced_at: $now,
            message: $"Task blocked: ($reason). Add to pending again when resolved.",
        },
        error: null
    }
}

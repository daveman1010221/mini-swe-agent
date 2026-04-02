#!/usr/bin/env nu
# task/advance.nu
#
# Called when a verification gate passes. Moves to the next playbook step.
# Resets step_attempts to 0. If on last step, marks task complete.
# NEVER call this without first calling meta/step-verify to confirm the gate passed.
#
# Usage:
#   nu tools/task/advance.nu --taskfile /workspace/agent-task.json --verification "cargo test: 12 passed, 0 failed"

def main [
    --taskfile: path = "",
    --verification: string   # evidence that the gate passed
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

    if ($verification | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --verification" }
    }

    let tf = (
        try { open $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task to advance" }
    }

    let current_step  = ($current | get step?       | default "unknown")
    let step_index    = ($current | get step_index? | default 0)
    let next_index    = $step_index + 1

    # Known steps for write-tests playbook
    let playbook_steps = ["survey", "orient", "scaffold", "write", "verify", "finalize"]
    let total_steps = ($playbook_steps | length)

    let task_completed = $next_index >= $total_steps
    let next_step = if $task_completed {
        null
    } else {
        $playbook_steps | get $next_index?
    }

    # Update the task file
    let updated_task = if $task_completed {
        # Move to completed
        let completed_entry = {
            crate: ($current | get crate? | default ""),
            op: ($current | get op? | default ""),
            scope: ($current | get scope? | default ""),
            status: "done",
            verification: $verification,
            completed_at: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
        }

        let pending = ($tf | get pending? | default [])
        let next_task = if ($pending | length) > 0 {
            $pending | first
        } else {
            null
        }
        let remaining_pending = if ($pending | length) > 0 {
            $pending | skip 1
        } else {
            []
        }

        $tf
        | upsert completed ($tf | get completed? | default [] | append $completed_entry)
        | upsert current_task $next_task
        | upsert pending $remaining_pending
    } else {
        # Advance to next step
        let updated_current = $current
            | upsert step $next_step
            | upsert step_index $next_index
            | upsert step_attempts 0
            | upsert last_verification $verification
            | upsert last_advanced_at (date now | format date "%Y-%m-%dT%H:%M:%SZ")

        $tf | upsert current_task $updated_current
    }

    # Write back
    try {
        $updated_task | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    let next_pending = ($updated_task | get current_task? | default null)

    {
        ok: true,
        data: {
            advanced: true,
            previous_step: $current_step,
            current_step: $next_step,
            task_completed: $task_completed,
            next_task: (if $task_completed { $next_pending } else { null }),
        },
        error: null
    }
}

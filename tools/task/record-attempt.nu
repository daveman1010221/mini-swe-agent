#!/usr/bin/env nu
# task/record-attempt.nu
#
# Called after every ACT phase. Increments step_attempts.
# Agent calls this BEFORE checking if the attempt succeeded.
# This ensures the loop budget is honest — no cherry-picking.
#
# Usage:
#   nu tools/task/record-attempt.nu \
#     --taskfile /workspace/agent-task.json \
#     --action "compile/check" \
#     --result "2 errors: E0308 type mismatch in tests/unit.rs"

def main [
    --taskfile: path = "",
    --action: string,    # what tool was called
    --result: string     # brief description of outcome
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

    if ($action | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --action" }
    }

    let tf = (
        try { open $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task" }
    }

    let step_attempts = ($current | get step_attempts? | default 0) + 1
    let step_budget   = ($current | get step_budget?   | default 3)
    let budget_remaining = $step_budget - $step_attempts
    let budget_exhausted = $step_attempts >= $step_budget

    # Append attempt to history
    let attempt_entry = {
        action: $action,
        result: $result,
        at: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
    }

    let attempt_history = ($current | get attempt_history? | default []) | append $attempt_entry

    let updated_current = $current
        | upsert step_attempts $step_attempts
        | upsert attempt_history $attempt_history
        | upsert last_action $action
        | upsert last_result $result

    let updated_tf = $tf
        | upsert current_task $updated_current
        | upsert last_updated (date now | format date "%Y-%m-%dT%H:%M:%SZ")

    try {
        $updated_tf | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            step_attempts: $step_attempts,
            step_budget: $step_budget,
            budget_remaining: $budget_remaining,
            budget_exhausted: $budget_exhausted,
            budget_warning: $budget_remaining <= 1,
        },
        error: null
    }
}

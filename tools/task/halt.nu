#!/usr/bin/env nu
# task/halt.nu
#
# Called when the agent cannot proceed.
# Records the reason and OODA phase. Human must intervene before work resumes.
# Different from task/block — halt is unexpected. Block is a known constraint.
#
# Usage:
#   nu tools/task/halt.nu --taskfile /workspace/agent-task.json \
#     --reason "compile/check failed 3 times with same error" \
#     --ooda-phase "act" \
#     --context "attempting to fix E0308 in tests/unit.rs line 42"

def main [
    --taskfile: path = "",
    --reason: string,       # specific, actionable description of why
    --ooda-phase: string,   # "observe" | "orient" | "decide" | "act"
    --context: string = ""  # what the agent was attempting
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

    if ($reason | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --reason" }
    }

    if ($ooda_phase | default "" | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --ooda-phase" }
    }

    let tf = (
        try { open --raw $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    let task_id = if $current != null { $current | get id? | default "" } else { "" }
    let step    = if $current != null { $current | get step? | default "unknown" } else { "unknown" }

    let halt_entry = {
        task_id: $task_id,
        step: $step,
        reason: $reason,
        ooda_phase: $ooda_phase,
        context: $context,
        halted_at: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
    }

    # Mark current task as halted
    let updated_current = if $current != null {
        $current | upsert status "halted" | upsert halted_reason $reason
    } else {
        null
    }

    let updated_tf = $tf
        | upsert current_task $updated_current
        | upsert halted ($tf | get halted? | default [] | append $halt_entry)
        | upsert last_updated (date now | format date "%Y-%m-%dT%H:%M:%SZ")

    try {
        $updated_tf | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            halted: true,
            task_id: $task_id,
            step: $step,
            reason: $reason,
            ooda_phase: $ooda_phase,
            halted_at: ($halt_entry.halted_at),
            message: "Task halted. Human intervention required before work can resume.",
        },
        error: null
    }
}

#!/usr/bin/env nu
# meta/orient-report.nu
#
# Called during ORIENT phase to record the agent's reasoning before acting.
# Documents what was observed, what step the agent is on, and what decision
# was made. Written to the taskfile as last_orient for trajectory analysis
# and supervisor visibility.
#
# Call this AFTER the 3 OBSERVE tools and BEFORE any ACT tool.
# This is the DECIDE record — it proves the agent reasoned before acting.
#
# Usage:
#   nu tools/meta/orient-report.nu \
#     --step "scaffold" \
#     --observed "compile/check clean, no tests dir yet, 9 source files" \
#     --decision "create tests/ dir and unit.rs scaffold, then cargo-test-entry" \
#     --blockers ""

def main [
    --taskfile: path = "",
    --step: string = "",          # current playbook step name
    --observed: string = "",      # what the OBSERVE phase revealed
    --decision: string = "",      # what ACT will do and why
    --blockers: string = "",      # any blocking issues discovered (empty if none)
    --loop-count: int = 0,        # loop iteration within this step
] {
    let tf_path = if ($taskfile | str length) > 0 {
        $taskfile
    } else if ("TASKFILE" in $env) {
        $env.TASKFILE
    } else {
        ""
    }

    if ($tf_path | str length) == 0 {
        return { ok: false, data: null, error: "no taskfile path — provide --taskfile or set $TASKFILE" }
    }

    if ($observed | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --observed" }
    }

    if ($decision | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --decision" }
    }

    let tf = (
        try { open --raw $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task" }
    }

    # Resolve step from taskfile if not provided
    let resolved_step = if ($step | str length) > 0 {
        $step
    } else {
        $current | get step? | default "unknown"
    }

    let step_attempts = ($current | get step_attempts? | default 0)
    let step_budget   = ($current | get step_budget?   | default 3)

    let orient_record = {
        step: $resolved_step,
        loop_count: $loop_count,
        step_attempts: $step_attempts,
        step_budget: $step_budget,
        budget_remaining: ($step_budget - $step_attempts),
        observed: $observed,
        decision: $decision,
        blockers: $blockers,
        recorded_at: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
    }

    let updated_current = $current | upsert last_orient $orient_record
    let updated_tf = $tf
        | upsert current_task $updated_current
        | upsert last_updated (date now | format date "%Y-%m-%dT%H:%M:%SZ")

    try {
        $updated_tf | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    # Surface budget warning prominently
    let budget_remaining = ($step_budget - $step_attempts)
    let budget_warning = if $budget_remaining <= 1 {
        $"⚠️ BUDGET CRITICAL: ($budget_remaining) attempts remaining for step '($resolved_step)'. Complete this step NOW or call task/halt."
    } else if $budget_remaining <= 2 {
        $"⚠️ Budget low: ($budget_remaining) attempts remaining for step '($resolved_step)'."
    } else {
        ""
    }

    {
        ok: true,
        data: {
            recorded: true,
            step: $resolved_step,
            budget_remaining: $budget_remaining,
            budget_warning: $budget_warning,
            decision: $decision,
        },
        error: null
    }
}

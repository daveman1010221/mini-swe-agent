#!/usr/bin/env nu
# task/state.nu
#
# The first call in every OBSERVE phase.
# Returns the complete current task state including playbook position.
# Never proceed without calling this first.
#
# Usage:
#   nu tools/task/state.nu --taskfile /workspace/agent-task.json
#   nu tools/task/state.nu  # uses $TASKFILE env var

def main [
    --taskfile: path = ""   # path to agent-task.json (default: $TASKFILE env var)
] {
    # Resolve taskfile path
    let tf_path = if ($taskfile | str length) > 0 {
        $taskfile
    } else if ("TASKFILE" in $env) {
        $env.TASKFILE
    } else {
        ""
    }

    if ($tf_path | str length) == 0 {
        return {
            ok: false,
            data: null,
            error: "no taskfile path — provide --taskfile or set $TASKFILE"
        }
    }

    if not ($tf_path | path exists) {
        return {
            ok: false,
            data: null,
            error: $"taskfile not found: ($tf_path)"
        }
    }

    # Parse the task file
    let tf = (
        try {
            open $tf_path | from json
        } catch {|err|
            return {
                ok: false,
                data: null,
                error: $"failed to parse taskfile: ($err.msg)"
            }
        }
    )

    let current = ($tf | get current_task? | default null)
    let has_task = $current != null

    let pending_count   = ($tf | get pending?   | default [] | length)
    let completed_count = ($tf | get completed? | default [] | length)
    let blocked_count   = ($tf | get blocked?   | default [] | length)
    let halted_count    = ($tf | get halted?    | default [] | length)

    if not $has_task {
        return {
            ok: true,
            data: {
                has_task: false,
                pending_count: $pending_count,
                completed_count: $completed_count,
                blocked_count: $blocked_count,
                halted_count: $halted_count,
                message: "no current task — run task/next to pop from pending queue",
            },
            error: null
        }
    }

    # Extract task execution state
    let step            = ($current | get step?            | default "unknown")
    let step_index      = ($current | get step_index?      | default 0)
    let step_attempts   = ($current | get step_attempts?   | default 0)
    let step_budget     = ($current | get step_budget?     | default 3)
    let budget_remaining = ($step_budget - $step_attempts)
    let budget_exhausted = $step_attempts >= $step_budget

    {
        ok: true,
        data: {
            has_task: true,
            task_id: ($current | get id? | default ""),
            type: ($current | get op? | default "unknown"),
            crate: ($current | get crate? | default ($current | get crate_field? | default "unknown")),
            crate_path: ($current | get crate_path? | default ""),
            playbook: ($current | get playbook? | default ""),
            step: $step,
            step_index: $step_index,
            step_attempts: $step_attempts,
            step_budget: $step_budget,
            budget_remaining: $budget_remaining,
            budget_exhausted: $budget_exhausted,
            coverage_plan: ($current | get coverage_plan? | default null),
            review: ($current | get review? | default false),
            next_action: ($current | get next_action? | default ""),
            success_condition: ($current | get success_condition? | default ""),
            notes: ($current | get notes? | default ""),
            started_at: ($current | get started_at? | default ""),
            pending_count: $pending_count,
            completed_count: $completed_count,
            blocked_count: $blocked_count,
            halted_count: $halted_count,
            # Urgency flags — agent should check these every OBSERVE
            budget_warning: ($budget_remaining <= 1),
        },
        error: null
    }
}

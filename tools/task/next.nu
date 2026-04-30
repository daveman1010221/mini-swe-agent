#!/usr/bin/env nu
# task/next.nu
#
# Pops the next pending task and sets it as current.
# Looks up the playbook for the task type.
# Halts if no playbook exists for the task type.
# This is what the agent calls at the start of a new session.
#
# Usage:
#   nu tools/task/next.nu --taskfile /workspace/agent-task.json

def main [
    --taskfile: path = ""
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

    let tf = (
        try { open --raw $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    # Check if there's already a current task
    let existing = ($tf | get current_task? | default null)
    if $existing != null and ($existing | get status? | default "") != "halted" {
        return {
            ok: true,
            data: {
                has_task: true,
                task_type: ($existing | get op? | default "unknown"),
                playbook_found: true,
                crate: ($existing | get crate? | default ""),
                first_step: ($existing | get step? | default "survey"),
                message: "existing current task — use task/state to see details",
            },
            error: null
        }
    }

    let pending = ($tf | get pending? | default [])

    if ($pending | length) == 0 {
        return {
            ok: true,
            data: {
                has_task: false,
                task_type: null,
                playbook_found: false,
                crate: null,
                first_step: null,
                message: "no pending tasks — add tasks to pending queue",
            },
            error: null
        }
    }

    # Pop next task
    let next = ($pending | first)
    let remaining = ($pending | skip 1)
    let task_type = ($next | get op? | default "unknown")

    # Known playbooks — this will eventually be a registry lookup
    let known_playbooks = ["write-tests", "fix-clippy", "fix-compile", "fmt"]
    let playbook_found = ($known_playbooks | any {|p| $p == $task_type})

    if not $playbook_found {
        # Record halt — unknown task type
        let halt_entry = {
            task_id: "",
            step: "task/next",
            reason: $"no playbook for task type: ($task_type)",
            ooda_phase: "observe",
            context: $"known types: ($known_playbooks | str join ', ')",
            halted_at: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
        }

        let updated_tf = $tf
            | upsert halted ($tf | get halted? | default [] | append $halt_entry)
            | upsert last_updated (date now | format date "%Y-%m-%dT%H:%M:%SZ")

        try { $updated_tf | to json | save --force $tf_path } catch {}

        return {
            ok: true,
            data: {
                has_task: false,
                task_type: $task_type,
                playbook_found: false,
                crate: ($next | get crate? | default ""),
                first_step: null,
                known_playbook_types: $known_playbooks,
                message: $"HALT — no playbook for task type: ($task_type)",
            },
            error: null
        }
    }

    # Build current task record
    let now = (date now | format date "%Y-%m-%dT%H:%M:%SZ")
    let current_task = $next
        | upsert playbook $task_type
        | upsert step "survey"
        | upsert step_index 0
        | upsert step_attempts 0
        | upsert step_budget 3
        | upsert status "in-progress"
        | upsert started_at $now
        | upsert coverage_plan null

    let updated_tf = $tf
        | upsert current_task $current_task
        | upsert pending $remaining
        | upsert last_updated $now

    try {
        $updated_tf | to json | save --force $tf_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write taskfile: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            has_task: true,
            task_type: $task_type,
            playbook_found: true,
            crate: ($current_task | get crate? | default ""),
            crate_path: ($current_task | get crate_path? | default ""),
            first_step: "survey",
            message: $"Task loaded: ($task_type) on ($current_task | get crate? | default 'unknown')",
        },
        error: null
    }
}

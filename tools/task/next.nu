#!/usr/bin/env nu
# task/next.nu — load next pending task via mswea plugin
#
# Loads the next pending task into the current slot.
# If a non-halted current task already exists, returns it.
# Halts if no pending tasks remain.
# This is what the agent calls at the start of a new session.
#
# Usage:
#   nu tools/task/next.nu

def main [] {
    let result = (mswea rpc task-state)
    if not $result.ok {
        return { ok: false, has_task: false, error: $result.error }
    }
    let d = $result.data
    {
        ok: true,
        has_task: $d.has_task,
        crate_name: $d.crate_name,
        op: $d.op,
        first_step: $d.step,
        playbook_found: $d.has_task,
        error: null
    }
}

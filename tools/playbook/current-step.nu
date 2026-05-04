#!/usr/bin/env nu
# playbook/current-step.nu — get current playbook step via mswea plugin

def main [] {
    let result = (mswea rpc task-state)
    if not $result.ok {
        return { ok: false, data: null, error: $result.error }
    }
    let d = $result.data
    {
        ok: true,
        data: {
            step_name: ($d.step | default ""),
            step_index: ($d.step_index | default 0),
            task_type: ($d.op | default ""),
            description: "",
            has_task: $d.has_task,
        },
        error: null
    }
}

#!/usr/bin/env nu
# meta/step-verify.nu — verify current playbook step via mswea plugin

def main [
    --step: string = "",
] {
    let result = (mswea rpc task-state)
    if not $result.ok {
        return { ok: false, data: null, error: $result.error }
    }
    let d = $result.data
    let current = ($d.step | default "")
    let passed = $current == $step
    {
        ok: true,
        data: {
            step: $step,
            gate: (if $passed { "passed" } else { $"expected ($step), got ($current)" }),
            passed: $passed,
            evidence: $"current step: ($current)",
        },
        error: null
    }
}

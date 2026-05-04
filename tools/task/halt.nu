#!/usr/bin/env nu
# task/halt.nu — halt current task via mswea plugin
#
# Called when the agent cannot proceed.
# Records the reason. Human must intervene before work resumes.
# Different from task/defer — halt is unexpected. Defer is a deliberate decision.
#
# Usage:
#   nu tools/task/halt.nu --reason "compile/check failed 3 times with same error"

def main [
    --reason: string = "no reason provided"
] {
    let result = (mswea rpc halt --reason $reason)
    if not $result.ok {
        return { ok: false, data: null, error: $result.error }
    }
    { ok: true, halted: $result.halted, error: null }
}

#!/usr/bin/env nu
# task/block.nu — defer current task via mswea plugin

# Called when a task cannot be completed due to external dependency,
# missing infrastructure, or explicit out-of-scope constraint.
# Different from task/halt — defer is a deliberate decision. Halt is unexpected.
#
# Usage:
#   nu tools/task/block.nu \
#     --crate-name cassini-types \
#     --reason "crate has compile errors unrelated to test writing — needs fix first"

def main [
    --crate-name: string = "",
    --reason: string = "",
] {
    let result = (mswea rpc halt --reason $"deferred: ($reason)")
    if not $result.ok {
        return { ok: false, deferred: false, error: $result.error }
    }
    { ok: true, deferred: true, error: null }
}

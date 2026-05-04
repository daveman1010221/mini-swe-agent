#!/usr/bin/env nu
# task/advance.nu — advance to next playbook step via mswea plugin
#
# Called when a verification gate passes. Moves to the next playbook step.
# Resets step_attempts to 0. If on last step, marks task complete.
#
# Usage:
#   nu tools/task/advance.nu --verification "cargo test: 12 passed, 0 failed"

def main [
    --verification: string = ""
] {
    let result = (mswea rpc advance --verification $verification)
    if not $result.ok {
        return { ok: false, data: null, error: $result.error }
    }
    { ok: true, data: $result, error: null }
}

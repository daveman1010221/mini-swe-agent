#!/usr/bin/env nu
# task/block.nu
#
# Called when a task cannot be completed due to external dependency,
# missing infrastructure, or explicit out-of-scope constraint.
# Different from task/halt — defer is a deliberate decision. Halt is unexpected.
#
# Usage:
#   nu tools/task/block.nu \
#     --crate-name cassini-types \
#     --reason "crate has compile errors unrelated to test writing — needs fix first"

def main [
    --crate-name: string,   # name of the crate being deferred
    --reason: string,       # specific, named reason for deferral
] {
    if ($crate_name | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --crate-name" }
    }

    if ($reason | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --reason" }
    }

    let base = $env.MSWEA_RPC_BASE? | default "http://127.0.0.1:8000"

    let response = (
        try {
            http post $"($base)/task/defer" {
                crate_name: $crate_name,
                reason: $reason,
            } --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: $"RPC call failed: ($err.msg)" }
        }
    )

    $response
}

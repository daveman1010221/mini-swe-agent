#!/usr/bin/env nu
# task/state.nu
#
# The first call in every OBSERVE phase.
# Returns the complete current task state including playbook position.
# Never proceed without calling this first.
#
# Usage:
#   nu tools/task/state.nu

def main [] {
    let base = if ("MSWEA_RPC_BASE" in $env) { $env.MSWEA_RPC_BASE } else { "http://127.0.0.1:8000" }

    let result = (
        try {
            http post $"($base)/task/state" ({} | to json) --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: ($err | to json) }
        }
    )

    $result
}

#!/usr/bin/env nu
# task/next.nu
#
# Loads the next pending task into the current slot.
# If a non-halted current task already exists, returns it.
# Halts if no pending tasks remain.
# This is what the agent calls at the start of a new session.
#
# Usage:
#   nu tools/task/next.nu

def main [] {
    let base = $env.MSWEA_RPC_BASE? | default "http://127.0.0.1:8000"

    let response = (
        try {
            http post $"($base)/task/load" {} --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: $"RPC call failed: ($err.msg)" }
        }
    )

    $response
}

#!/usr/bin/env nu
# task/record-attempt.nu
#
# Called after every ACT phase. Increments step_attempts.
# Agent calls this BEFORE checking if the attempt succeeded.
# This ensures the loop budget is honest — no cherry-picking.
#
# Usage:
#   nu tools/task/record-attempt.nu \
#     --action "compile/check" \
#     --result "2 errors: E0308 type mismatch in tests/unit.rs"

def main [
    --action: string,    # what tool was called
    --result: string     # brief description of outcome
] {
    if ($action | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --action" }
    }

    let base = $env.MSWEA_RPC_BASE? | default "http://127.0.0.1:8000"

    let response = (
        try {
            http post $"($base)/task/record-attempt" {
                action: $action,
                result: $result,
            } --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: $"RPC call failed: ($err.msg)" }
        }
    )

    $response
}

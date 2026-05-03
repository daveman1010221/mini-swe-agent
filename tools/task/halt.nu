#!/usr/bin/env nu
# task/halt.nu
#
# Called when the agent cannot proceed.
# Records the reason. Human must intervene before work resumes.
# Different from task/defer — halt is unexpected. Defer is a deliberate decision.
#
# Usage:
#   nu tools/task/halt.nu --reason "compile/check failed 3 times with same error"

def main [
    --reason: string,      # specific, actionable description of why
] {
    if ($reason | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --reason" }
    }

    let base = $env.MSWEA_RPC_BASE? | default "http://127.0.0.1:8000"

    let response = (
        try {
            http post $"($base)/task/halt" {
                reason: $reason,
            } --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: $"RPC call failed: ($err.msg)" }
        }
    )

    $response
}

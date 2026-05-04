#!/usr/bin/env nu
# task/record-attempt.nu — record a step attempt via mswea plugin
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
    --action: string = "",
    --result: string = "",
] {
    let res = (mswea rpc record-attempt --action $action --result $result)
    if not $res.ok {
        return { ok: false, step_attempts: 0, budget_remaining: 0, budget_exhausted: false, error: $res.error }
    }
    { ok: true, step_attempts: $res.step_attempts, budget_remaining: $res.budget_remaining, budget_exhausted: $res.budget_exhausted, error: null }
}

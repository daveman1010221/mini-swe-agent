#!/usr/bin/env nu
# meta/orient-report.nu
#
# Called during ORIENT phase to record the agent's reasoning before acting.
# Documents what was observed, what decision was made, and any blockers.
# Written to the taskfile as last_orient for trajectory analysis.
#
# Call this AFTER the 3 OBSERVE tools and BEFORE any ACT tool.
# This is the DECIDE record — it proves the agent reasoned before acting.
#
# Usage:
#   nu tools/meta/orient-report.nu \
#     --observed "compile/check clean, no tests dir yet, 9 source files" \
#     --decision "create tests/ dir and unit.rs scaffold, then cargo-test-entry" \
#     --blockers ""

def main [
    --observed: string = "",   # what the OBSERVE phase revealed
    --decision: string = "",   # what ACT will do and why
    --blockers: string = "",   # any blocking issues discovered (empty if none)
] {
    if ($observed | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --observed" }
    }

    if ($decision | str length) == 0 {
        return { ok: false, data: null, error: "missing required flag: --decision" }
    }

    let base = $env.MSWEA_RPC_BASE? | default "http://127.0.0.1:8000"

    let response = (
        try {
            http post $"($base)/task/record-orient" {
                observed: $observed,
                decision: $decision,
                blockers: (if ($blockers | str length) > 0 { $blockers } else { null }),
            } --content-type application/json
        } catch {|err|
            return { ok: false, data: null, error: $"RPC call failed: ($err.msg)" }
        }
    )

    $response
}

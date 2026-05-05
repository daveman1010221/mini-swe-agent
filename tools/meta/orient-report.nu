#!/usr/bin/env nu
# meta/orient-report.nu — record orient step via mswea plugin
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
    --observed: string = "",
    --decision: string = "",
    --blockers: string = "",
] {
    let result = (mswea rpc record-orient {
        observed: $observed
        decision: $decision
        blockers: (if ($blockers | str length) > 0 { $blockers } else { "" })
    })

    if not $result.ok {
        return { ok: false, recorded: false, step: "", budget_remaining: 0, error: $result.error }
    }

    { ok: true, recorded: $result.recorded, step: $result.step, budget_remaining: $result.budget_remaining, error: null }
}

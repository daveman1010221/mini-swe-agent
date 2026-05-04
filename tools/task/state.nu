#!/usr/bin/env nu
# task/state.nu — query current task state via mswea plugin
#
# The first call in every OBSERVE phase.
# Returns the complete current task state including playbook position.
# Never proceed without calling this first.
#
# Usage:
#   nu tools/task/state.nu

def main [] {
    let result = (mswea rpc task-state)
    if not $result.ok {
        return { ok: false, data: null, error: $result.error }
    }
    { ok: true, data: $result.data, error: null }
}

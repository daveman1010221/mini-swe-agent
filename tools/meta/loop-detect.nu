#!/usr/bin/env nu
# meta/loop-detect.nu
#
# Analyzes the trajectory JSONL to detect reasoning loops.
# A loop is: same tool + same inputs appearing > threshold times
# without an intervening step-advance.
#
# This is the most important safety tool in the toolbox.
# Call this at the start of every OODA OBSERVE phase.
#
# Usage:
#   nu tools/meta/loop-detect.nu --trajectory /tmp/run.jsonl
#   nu tools/meta/loop-detect.nu --trajectory /tmp/run.jsonl --threshold 2 --window 30

def main [
    --trajectory: path,
    --window: int = 20,      # how many recent events to analyze
    --threshold: int = 3     # repeat count that triggers detection
] {
    if ($trajectory | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --trajectory" }
    }

    if not ($trajectory | path exists) {
        # No trajectory yet — no loops possible
        return {
            ok: true,
            data: {
                loop_detected: false,
                loops: [],
                window_analyzed: 0,
                unique_actions: 0,
                message: "no trajectory file yet — session just started"
            },
            error: null
        }
    }

    # Load and parse trajectory events
    let events = (
        try {
            open $trajectory
            | lines
            | where ($it | str length) > 0
            | each {|line| $line | from json}
        } catch {|err|
            return { ok: false, data: null, error: $"Failed to parse trajectory: ($err.msg)" }
        }
    )

    if ($events | is-empty) {
        return {
            ok: true,
            data: {
                loop_detected: false,
                loops: [],
                window_analyzed: 0,
                unique_actions: 0,
            },
            error: null
        }
    }

    # Take the most recent N events
    let windowed = (
        $events
        | last ([$window ($events | length)] | math min)
    )

    # Extract tool call actions from events
    # We look for shell commands and tool calls
    let actions = (
        $windowed
        | where {|e|
            let kind = ($e | get kind.kind? | default "")
            $kind == "shell_command_started" or $kind == "tool_call_emitted"
        }
        | each {|e|
            let kind = ($e | get kind.kind? | default "")
            let action = if $kind == "shell_command_started" {
                $e | get kind.command? | default ""
            } else {
                # tool_call_emitted — get the call summary
                let call = ($e | get kind.call? | default {})
                let call_type = ($call | get type? | default "unknown")
                match $call_type {
                    "shell"       => ($call | get command? | default ""),
                    "nushell_tool" => $"($call | get namespace? | default '')/($call | get tool? | default '')",
                    "read"        => $"read:($call | get path? | default '')",
                    "write"       => $"write:($call | get path? | default '')",
                    "edit"        => $"edit:($call | get path? | default '')",
                    "search"      => $"search:($call | get query? | default '')",
                    _             => $call_type,
                }
            }

            {
                action: $action,
                timestamp: ($e | get timestamp_ms? | default 0),
                kind: $kind,
            }
        }
        | where {|a| ($a.action | str length) > 0}
    )

    # Check for step advances — these reset the loop counter
    let step_advances = (
        $windowed
        | where {|e|
            let kind = ($e | get kind.kind? | default "")
            $kind == "agent_step"
        }
        | length
    )

    # Count action frequencies
    let action_counts = (
        $actions
        | group-by action
        | transpose action events
        | each {|row|
            let count = ($row.events | length)
            let first_ts = ($row.events | first | get timestamp | default 0)
            let last_ts = ($row.events | last | get timestamp | default 0)
            {
                action: $row.action,
                count: $count,
                first_seen: ($first_ts | into datetime | format date "%H:%M:%S"),
                last_seen: ($last_ts | into datetime | format date "%H:%M:%S"),
            }
        }
    )

    # Find loops — actions exceeding threshold
    let loops = (
        $action_counts
        | where count >= $threshold
        | each {|row|
            let recommendation = if $row.count >= ($threshold * 2) {
                "halt"
            } else {
                "try-alternate"
            }
            {
                tool: ($row.action | str substring 0..40),
                command: $row.action,
                count: $row.count,
                first_seen: $row.first_seen,
                last_seen: $row.last_seen,
                recommendation: $recommendation,
            }
        }
    )

    let loop_detected = ($loops | length) > 0

    {
        ok: true,
        data: {
            loop_detected: $loop_detected,
            loops: $loops,
            window_analyzed: ($windowed | length),
            unique_actions: ($action_counts | length),
            step_advances_in_window: $step_advances,
        },
        error: null
    }
}

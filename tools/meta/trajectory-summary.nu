#!/usr/bin/env nu
# meta/trajectory-summary.nu
#
# Returns a structured summary of recent agent activity.
# Used in OBSERVE to understand what has been done this session.
# Call this at the start of every OODA cycle alongside loop-detect.
#
# Usage:
#   nu tools/meta/trajectory-summary.nu --trajectory /tmp/run.jsonl
#   nu tools/meta/trajectory-summary.nu --trajectory /tmp/run.jsonl --last-n 20 --full

def main [
    --trajectory: path,
    --last-n: int = 10,   # most recent N events to return in recent_events
    --full                # return full history (default: false)
] {
    if ($trajectory | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --trajectory" }
    }

    if not ($trajectory | path exists) {
        return {
            ok: true,
            data: {
                total_events: 0,
                total_steps: 0,
                total_tool_calls: 0,
                shell_commands: [],
                files_read: [],
                files_written: [],
                files_edited: [],
                compile_attempts: 0,
                compile_successes: 0,
                test_runs: 0,
                test_successes: 0,
                recent_events: [],
                message: "no trajectory yet — session just started",
            },
            error: null
        }
    }

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

    let total_events = ($events | length)

    # Count agent steps
    let total_steps = (
        $events
        | where {|e| ($e | get kind.kind? | default "") == "agent_step"}
        | length
    )

    # Count tool calls
    let total_tool_calls = (
        $events
        | where {|e| ($e | get kind.kind? | default "") == "tool_call_emitted"}
        | length
    )

    # Shell commands
    let shell_commands = (
        $events
        | where {|e| ($e | get kind.kind? | default "") == "shell_command_started"}
        | each {|e| {
            command: ($e | get kind.command? | default ""),
            timestamp: ($e | get timestamp_ms? | default 0 | into datetime | format date "%H:%M:%S"),
        }}
        | last ([$last_n ($events | length)] | math min)
    )

    # Files read
    let files_read = (
        $events
        | where {|e| ($e | get kind.kind? | default "") == "file_read"}
        | each {|e| $e | get kind.path? | default ""}
        | uniq
    )

    # Files written
    let files_written = (
        $events
        | where {|e| ($e | get kind.kind? | default "") == "file_written"}
        | each {|e| $e | get kind.path? | default ""}
        | uniq
    )

    # Files edited
    let files_edited = (
        $events
        | where {|e| ($e | get kind.kind? | default "") == "file_edited"}
        | each {|e| $e | get kind.path? | default ""}
        | uniq
    )

    # Compile attempts and successes (shell commands containing cargo check)
    let compile_events = (
        $events
        | where {|e|
            let kind = ($e | get kind.kind? | default "")
            $kind == "shell_command_started" and
            (($e | get kind.command? | default "") =~ "cargo check")
        }
    )
    let compile_attempts = ($compile_events | length)

    let compile_successes = (
        $events
        | where {|e|
            let kind = ($e | get kind.kind? | default "")
            $kind == "shell_command_completed" and
            ($e | get kind.exit_code? | default 1) == 0
        }
        | length
    )

    # Test runs
    let test_events = (
        $events
        | where {|e|
            let kind = ($e | get kind.kind? | default "")
            $kind == "shell_command_started" and
            (($e | get kind.command? | default "") =~ "cargo test")
        }
    )
    let test_runs = ($test_events | length)
    let test_successes = 0  # Would need exit code correlation — simplified for now

    # Recent events summary
    let source_events = if $full { $events } else {
        $events | last ([$last_n ($events | length)] | math min)
    }

    let recent_events = (
        $source_events
        | each {|e|
            let kind = ($e | get kind.kind? | default "unknown")
            let summary = match $kind {
                "agent_step"             => $"step ($e | get kind.step? | default '?')",
                "shell_command_started"  => $"shell: ($e | get kind.command? | default '' | str substring 0..60)",
                "tool_call_emitted"      => $"tool: ($e | get kind.call.type? | default 'unknown')",
                "file_read"              => $"read: ($e | get kind.path? | default '')",
                "file_written"           => $"write: ($e | get kind.path? | default '')",
                "file_edited"            => $"edit: ($e | get kind.path? | default '')",
                "model_response_received" => $"model: ($e | get kind.tokens_out? | default 0) tokens",
                "observation_received"   => "observation received",
                _                        => $kind,
            }
            {
                kind: $kind,
                summary: $summary,
                timestamp: ($e | get timestamp_ms? | default 0 | into datetime | format date "%H:%M:%S"),
            }
        }
    )

    {
        ok: true,
        data: {
            total_events: $total_events,
            total_steps: $total_steps,
            total_tool_calls: $total_tool_calls,
            shell_commands: $shell_commands,
            files_read: $files_read,
            files_written: $files_written,
            files_edited: $files_edited,
            compile_attempts: $compile_attempts,
            compile_successes: $compile_successes,
            test_runs: $test_runs,
            test_successes: $test_successes,
            recent_events: $recent_events,
        },
        error: null
    }
}

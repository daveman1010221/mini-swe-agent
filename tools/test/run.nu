#!/usr/bin/env nu
# test/run.nu
#
# Runs tests for a crate and returns structured pass/fail data.
# Agent never reads raw cargo test output — always goes through this tool.
# Zero failures required before advancing any playbook step.
#
# Usage:
#   nu tools/test/run.nu --workspace-root /workspace/src/agents --crate cassini-types
#   nu tools/test/run.nu --workspace-root /workspace/src/agents --crate cassini-types --target unit

def main [
    --workspace-root: path,
    --crate: string,
    --target: string = "all",   # "unit" | "props" | "integration" | "all"
    --filter: string = ""       # optional test name filter
] {
    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }

    if ($crate | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate" }
    }

    # Build cargo test command
    let test_args = match $target {
        "unit"  => $"--package ($crate) --test unit",
        "props" => $"--package ($crate) --test props",
        "all"   => $"--package ($crate)",
        _       => $"--package ($crate)",
    }

    let filter_arg = if ($filter | str length) > 0 { $"-- ($filter)" } else { "" }

    let cmd = $"cargo test ($test_args) --no-fail-fast 2>&1 ($filter_arg)"

    let start_time = (date now)

    let result = (
        try {
            cargo test --package $crate ...(
                if $target != "all" { ["--test" $target] } else { [] }
            ) --no-fail-fast -- --nocapture | complete
        } catch {|err|
            return { ok: false, data: null, error: $"failed to run cargo test: ($err.msg)" }
        }
    )

    let duration_secs = ((date now) - $start_time | into int) / 1_000_000_000.0

    let output = $result.stdout + "\n" + $result.stderr
    let lines = ($output | lines)

    # Parse test results
    # Look for "test result: ok. N passed; M failed; K ignored"
    let result_line = (
        $lines
        | where ($it =~ "test result:")
        | last 1
        | get 0?
        | default ""
    )

    let passed = (
        $result_line
        | parse --regex "(?P<n>\\d+) passed"
        | get n.0?
        | default "0"
        | into int
    )

    let failed = (
        $result_line
        | parse --regex "(?P<n>\\d+) failed"
        | get n.0?
        | default "0"
        | into int
    )

    let ignored = (
        $result_line
        | parse --regex "(?P<n>\\d+) ignored"
        | get n.0?
        | default "0"
        | into int
    )

    let total = $passed + $failed + $ignored
    let success = $failed == 0 and $result.exit_code == 0

    # Extract failure details
    let failures = if $failed > 0 {
        # Find FAILED test names
        let failed_names = (
            $lines
            | where ($it =~ "^FAILED ")
            | each {|l| $l | str replace "FAILED " "" | str trim}
        )

        $failed_names | each {|name|
            # Try to extract panic message for this test
            let panic_msg = (
                $lines
                | window 20
                | each {|window|
                    if ($window | any {|l| $l =~ $name}) {
                        $window
                        | where ($it =~ "thread '.*' panicked|panicked at")
                        | first 1
                        | get 0?
                    } else {
                        null
                    }
                }
                | where ($it != null)
                | first 1
                | get 0?
                | default ""
            )

            {
                name: $name,
                output: "",
                panic_message: $panic_msg,
            }
        }
    } else {
        []
    }

    {
        ok: true,
        data: {
            crate: $crate,
            target: $target,
            passed: $passed,
            failed: $failed,
            ignored: $ignored,
            total: $total,
            success: $success,
            failures: $failures,
            duration_secs: $duration_secs,
            exit_code: $result.exit_code,
        },
        error: null
    }
}

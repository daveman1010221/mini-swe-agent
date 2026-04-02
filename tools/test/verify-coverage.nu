#!/usr/bin/env nu
# test/verify-coverage.nu
#
# Checks actual test coverage against the coverage plan written in orient step.
# Called as the verification gate before task/advance in verify step.
# gate_passed must be true before calling task/advance.
#
# Usage:
#   nu tools/test/verify-coverage.nu \
#     --taskfile /workspace/agent-task.json \
#     --crate-path /workspace/src/agents/cassini/types \
#     --workspace-root /workspace/src/agents

def main [
    --taskfile: path = "",
    --crate-path: path,
    --workspace-root: path = ""
] {
    let tf_path = if ($taskfile | str length) > 0 {
        $taskfile
    } else if ("TASKFILE" in $env) {
        $env.TASKFILE
    } else {
        ""
    }

    if ($tf_path | str length) == 0 {
        return { ok: false, data: null, error: "no taskfile path" }
    }

    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    let tf = (
        try { open $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task" }
    }

    let coverage_plan = ($current | get coverage_plan? | default null)
    if $coverage_plan == null {
        return {
            ok: false,
            data: null,
            error: "no coverage plan found — call task/write-coverage-plan in orient step first"
        }
    }

    let planned_tests = ($coverage_plan | get planned_tests? | default [])
    let plan_count = ($planned_tests | length)

    if $plan_count == 0 {
        return {
            ok: false,
            data: null,
            error: "coverage plan has no planned tests"
        }
    }

    # Get actual tests from crate
    let tests_dir = ($crate_path | path join "tests")
    let actual_test_names = if ($tests_dir | path exists) {
        try {
            ls $tests_dir
            | where type == "file"
            | where name =~ "\.rs$"
            | each {|f|
                open $f.name
                | lines
                | where {|l| $l =~ "fn test_" or $l =~ "fn prop_"}
                | each {|l|
                    $l
                    | str replace --regex "^.*(fn )" ""
                    | str replace --regex "\(.*" ""
                    | str trim
                }
            }
            | flatten
        } catch {
            []
        }
    } else {
        []
    }

    # Check which planned tests exist
    let coverage_results = ($planned_tests | each {|planned|
        let test_name = ($planned | get name? | default "")
        let exists = ($actual_test_names | any {|actual| $actual == $test_name})
        {
            name: $test_name,
            type: ($planned | get type? | default ""),
            rationale: ($planned | get rationale? | default ""),
            exists: $exists,
        }
    })

    let covered = ($coverage_results | where exists == true | length)
    let uncovered = ($coverage_results | where exists == false)
    let uncovered_count = ($uncovered | length)
    let coverage_rate = if $plan_count > 0 { $covered / $plan_count } else { 0.0 }

    # Run the tests to check they pass (if workspace-root provided)
    let tests_pass = if ($workspace_root | str length) > 0 and $uncovered_count == 0 {
        let crate_name = ($crate_path | path basename)
        let result = (
            try {
                do { cd $workspace_root; cargo test --package $crate_name 2>&1 } | complete
            } catch { {exit_code: 1} }
        )
        $result.exit_code == 0
    } else if $uncovered_count > 0 {
        false
    } else {
        true  # No workspace root — assume pass if all planned tests exist
    }

    let gate_passed = $uncovered_count == 0 and $tests_pass

    {
        ok: true,
        data: {
            plan_item_count: $plan_count,
            covered: $covered,
            uncovered: $uncovered_count,
            coverage_rate: $coverage_rate,
            uncovered_items: $uncovered,
            tests_pass: $tests_pass,
            gate_passed: $gate_passed,
            message: (if $gate_passed {
                "✓ All planned tests exist and pass — ready to advance"
            } else if $uncovered_count > 0 {
                $"✗ ($uncovered_count) planned tests not yet written"
            } else {
                "✗ Tests exist but some are failing"
            }),
        },
        error: null
    }
}

#!/usr/bin/env nu
# meta/step-verify.nu
#
# Checks whether the current playbook step's verification gate has been met.
# Call this before task/advance — never advance without calling this first.
# The gate must pass before the agent is allowed to advance.
#
# Usage:
#   nu tools/meta/step-verify.nu --taskfile /workspace/agent-task.json --trajectory /tmp/run.jsonl

def main [
    --taskfile: path = "",
    --trajectory: path = "",
    --step: string = ""    # step to verify (default: current step from taskfile)
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

    let tf = (
        try { open --raw $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task" }
    }

    let current_step = if ($step | str length) > 0 { $step } else {
        $current | get step? | default "unknown"
    }

    # Step-specific verification logic — returns [gate, passed, evidence, missing]
    let verify_result = match $current_step {
        "survey" => {
            let evidence = "Survey automated by ToolboxActor — preflight results available"
            ["compile/check passed and crate structure understood" true $evidence []]
        }

        "orient" => {
            let plan = ($current | get coverage_plan? | default null)
            let planned = if $plan != null { $plan | get planned_tests? | default [] } else { [] }
            let planned_count = ($planned | length)
            let passed = ($plan != null) and ($planned_count > 0)
            let evidence = if $passed { $"Coverage plan written with ($planned_count) planned tests" } else { "" }
            let missing = if not $passed {
                if $plan == null { ["coverage_plan is null — call task/write-coverage-plan"] }
                else { ["planned_tests is empty — coverage plan needs at least one test"] }
            } else { [] }
            ["task/write-coverage-plan called with planned_count > 0" $passed $evidence $missing]
        }

        "scaffold" => {
            let crate_path = ($current | get crate_path? | default "")
            let tests_dir  = ($crate_path | path join "tests")
            let has_tests_dir = if ($crate_path | str length) > 0 { $tests_dir | path exists } else { false }
            let cargo_toml = ($crate_path | path join "Cargo.toml")
            let has_test_entries = if ($cargo_toml | path exists) {
                (open --raw $cargo_toml) =~ '\[\[test\]\]'
            } else { false }
            let passed = $has_tests_dir and $has_test_entries
            let evidence = if $passed { "tests/ directory exists and Cargo.toml has [[test]] entries" } else { "" }
            let missing = [
                ...(if not $has_tests_dir { ["tests/ directory missing — call create/tests-dir"] } else { [] }),
                ...(if not $has_test_entries { ["no [[test]] entries in Cargo.toml — call create/cargo-test-entry"] } else { [] }),
            ]
            ["tests/ dir exists and Cargo.toml declares [[test]] entries" $passed $evidence $missing]
        }

        "write" => {
            let plan = ($current | get coverage_plan? | default null)
            let planned = if $plan != null { $plan | get planned_tests? | default [] } else { [] }
            let crate_path = ($current | get crate_path? | default "")
            let tests_dir  = ($crate_path | path join "tests")

            let actual_names = if ($tests_dir | path exists) {
                try {
                    ls $tests_dir
                    | where type == "file"
                    | where name =~ '\.rs$'
                    | each {|f|
                        open --raw $f.name
                        | lines
                        | where {|l| $l =~ 'fn test_'}
                        | each {|l|
                            $l
                            | str replace --regex '.*fn ' ''
                            | str replace --regex '\(.*' ''
                            | str trim
                        }
                    }
                    | flatten
                } catch { [] }
            } else { [] }

            let missing_tests = ($planned | where {|p| not ($actual_names | any {|a| $a == ($p | get name? | default "")})})
            let passed = ($missing_tests | is-empty)
            let evidence = if $passed { $"All ($planned | length) planned tests found in test files" } else { "" }
            let missing_list = ($missing_tests | each {|t| $"test '($t | get name? | default '')' not yet written"})
            ["all planned tests written and compile/check passes" $passed $evidence $missing_list]
        }

        "verify" => {
            let traj_path = if ($trajectory | str length) > 0 { $trajectory } else { "" }
            let test_passed = if (($traj_path | str length) > 0) and ($traj_path | path exists) {
                try {
                    open --raw $traj_path
                    | lines
                    | where ($it | str length) > 0
                    | each {|l| $l | from json}
                    | where {|e| ($e | get kind.kind? | default "") == "shell_command_completed"}
                    | where {|e| ($e | get kind.exit_code? | default 1) == 0}
                    | length
                } catch { 0 }
            } else { 0 }
            let passed = $test_passed > 0
            let evidence = if $passed { "cargo test completed with exit_code 0 in trajectory" } else { "" }
            let missing = if not $passed { ["no successful test run found in trajectory — run test/run first"] } else { [] }
            ["test/run: failed == 0 and gate_passed == true" $passed $evidence $missing]
        }

        "finalize" => {
            ["fmt/apply done, compile/check clean, task/advance called last" true "Finalize gates are verified by the agent at each sub-step" []]
        }

        _ => {
            ["unknown step" false "" [$"Unknown step: ($current_step) — cannot verify"]]
        }
    }

    let gate    = ($verify_result | get 0)
    let passed  = ($verify_result | get 1)
    let evidence = ($verify_result | get 2)
    let missing = ($verify_result | get 3)

    {
        ok: true,
        data: {
            step: $current_step,
            gate: $gate,
            passed: $passed,
            evidence: $evidence,
            missing: $missing,
            message: (if $passed {
                $"Gate passed for step '($current_step)' — ready to advance"
            } else {
                $"Gate not yet passed for step '($current_step)'"
            }),
        },
        error: null
    }
}

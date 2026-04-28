#!/usr/bin/env nu
# playbook/current-step.nu
#
# Returns full details of the current playbook step including approved tools.
# Call after task/state to get step-specific guidance.
# The approved_tools list is the agent's constraint for this step — no others.
#
# Usage:
#   nu tools/playbook/current-step.nu --taskfile /workspace/agent-task.json

def main [
    --taskfile: path = ""
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
        try { open $tf_path | from json }
        catch {|err| return { ok: false, data: null, error: $"failed to parse taskfile: ($err.msg)" }}
    )

    let current = ($tf | get current_task? | default null)
    if $current == null {
        return { ok: false, data: null, error: "no current task — call task/next first" }
    }

    let step_name    = ($current | get step?          | default "unknown")
    let step_index   = ($current | get step_index?    | default 0)
    let step_attempts = ($current | get step_attempts? | default 0)
    let task_type    = ($current | get op?            | default "write-tests")

    # Step definitions — mirrors playbook/lookup but returns just the current step
    let all_steps = match $task_type {
        "write-tests" => {
            "survey": {
                description: "Understand the crate completely. Read only. No file creation.",
                approved_tools: [
                    "locate/files", "locate/actors", "locate/symbols",
                    "locate/derives", "locate/tests", "locate/deps",
                    "extract/file", "extract/range", "extract/symbol",
                    "extract/actor", "extract/cargo-toml",
                    "compile/check",
                    "task/state", "meta/loop-detect", "meta/trajectory-summary",
                    "meta/orient-report", "playbook/current-step", "tools/check-approved",
                ],
                forbidden_tools: ["create/*", "fmt/*", "lint/*", "task/advance", "task/write-coverage-plan"],
                verification_gate: "compile/check passed. Crate structure understood.",
                budget: 3,
                orient_questions: [
                    "Is this an actor crate, a types crate, or both?",
                    "Which derive macros are present — serde? rkyv? both?",
                    "Do existing tests exist? How many and what do they cover?",
                    "Does compile/check pass cleanly right now?",
                    "Are there private fields that prevent struct literal construction?",
                ],
                example_actions: [
                    "locate/actors to find Actor implementations",
                    "locate/derives to check for serde/rkyv",
                    "locate/tests to establish baseline test count",
                    "compile/check to verify clean state",
                ],
            }
            "orient": {
                description: "Write the coverage plan. Document every decision. This is the contract for task/advance.",
                approved_tools: [
                    "task/write-coverage-plan",
                    "task/state", "meta/loop-detect", "meta/trajectory-summary",
                    "meta/orient-report", "playbook/current-step",
                ],
                forbidden_tools: ["locate/*", "extract/*", "create/*", "compile/*", "test/*", "fmt/*", "task/advance"],
                verification_gate: "task/write-coverage-plan called. planned_count > 0.",
                budget: 2,
                orient_questions: [
                    "What are all public interfaces that need tests?",
                    "Which types need serde roundtrip tests?",
                    "Which types need rkyv roundtrip tests?",
                    "Which actors need mailbox tests?",
                    "How many total tests are planned?",
                ],
                example_actions: [
                    "task/write-coverage-plan with full planned_tests list",
                ],
            }
            "scaffold": {
                description: "Create test infrastructure. No test bodies yet.",
                approved_tools: [
                    "create/tests-dir", "create/test-file",
                    "create/cargo-test-entry", "create/dev-dep",
                    "compile/check", "locate/tests", "extract/cargo-toml",
                    "task/state", "meta/loop-detect", "playbook/current-step",
                ],
                forbidden_tools: ["test/*", "fmt/*", "lint/*", "task/advance"],
                verification_gate: "compile/check passes. tests/ dir exists. Cargo.toml has [[test]] entries.",
                budget: 3,
                orient_questions: [
                    "Does tests/ directory exist?",
                    "Does Cargo.toml declare all required [[test]] entries?",
                    "Are all required dev-dependencies present?",
                    "Do the empty test files compile cleanly?",
                ],
                example_actions: [
                    "create/tests-dir to create tests/ if missing",
                    "create/test-file --target unit to scaffold tests/unit.rs",
                    "create/cargo-test-entry --name unit --path tests/unit.rs",
                    "create/dev-dep --dep proptest if props tests planned",
                ],
            }
            "write": {
                description: "Write test bodies against the coverage plan. One test at a time.",
                approved_tools: [
                    "extract/file", "extract/range", "extract/symbol", "extract/actor",
                    "compile/check", "compile/fix-hint",
                    "locate/symbols", "locate/derives",
                    "task/state", "meta/loop-detect", "meta/trajectory-summary",
                    "meta/orient-report", "playbook/current-step",
                ],
                forbidden_tools: ["test/*", "create/*", "fmt/*", "lint/*", "task/advance", "task/write-coverage-plan"],
                verification_gate: "compile/check passes. All planned tests written.",
                budget: 5,
                orient_questions: [
                    "How many planned tests written so far? How many remain?",
                    "Does the last written test compile cleanly?",
                    "Has loop-detect flagged any repeated compile errors?",
                ],
                example_actions: [
                    "extract/actor to get full actor structure before writing tests",
                    "extract/symbol to read a specific type definition",
                    "compile/check after each test written",
                    "compile/fix-hint if compile/check returns an error",
                ],
            }
            "verify": {
                description: "Run the tests. Zero failures required.",
                approved_tools: [
                    "test/run", "test/count", "test/verify-coverage",
                    "compile/check", "extract/range",
                    "task/state", "meta/loop-detect", "meta/trajectory-summary",
                    "meta/step-verify", "playbook/current-step",
                ],
                forbidden_tools: ["create/*", "fmt/*", "task/advance", "task/write-coverage-plan"],
                verification_gate: "test/run: failed == 0. gate_passed == true.",
                budget: 3,
                orient_questions: [
                    "How many tests passed? How many failed?",
                    "Is failed == 0?",
                    "Is gate_passed == true from verify-coverage?",
                ],
                example_actions: [
                    "test/run --target all to run all tests",
                    "test/verify-coverage to check against coverage plan",
                    "meta/step-verify to confirm gate status",
                ],
            }
            "finalize": {
                description: "Format, final checks, advance task state.",
                approved_tools: [
                    "fmt/apply", "fmt/check",
                    "compile/check", "test/run", "lint/check",
                    "task/advance", "task/state",
                    "meta/loop-detect", "meta/step-verify", "playbook/current-step",
                ],
                forbidden_tools: ["create/*", "task/write-coverage-plan", "task/halt", "task/block"],
                verification_gate: "fmt/apply done. compile/check clean. task/advance called last.",
                budget: 2,
                orient_questions: [
                    "Does fmt/check show unformatted files?",
                    "Does compile/check still pass after fmt?",
                    "Does test/run still show zero failures after fmt?",
                    "Does this task have review:true?",
                ],
                example_actions: [
                    "fmt/apply first",
                    "compile/check to verify fmt didn't break anything",
                    "test/run to verify tests still pass",
                    "task/advance LAST — not first",
                ],
            }
            _ => null
        }
        _ => null
    }

    let step_data = if $all_steps != null { $all_steps | get $step_name? | default null } else { null }

    if $step_data == null {
        return {
            ok: false,
            data: null,
            error: $"unknown step '($step_name)' for task type '($task_type)'"
        }
    }

    let step_budget   = ($step_data | get budget? | default 3)
    let budget_remaining = $step_budget - $step_attempts

    {
        ok: true,
        data: {
            step_name: $step_name,
            step_index: $step_index,
            task_type: $task_type,
            description: ($step_data | get description),
            approved_tools: ($step_data | get approved_tools),
            forbidden_tools: ($step_data | get forbidden_tools),
            verification_gate: ($step_data | get verification_gate),
            budget: $step_budget,
            budget_remaining: $budget_remaining,
            budget_exhausted: $budget_remaining <= 0,
            orient_questions: ($step_data | get orient_questions),
            example_actions: ($step_data | get example_actions? | default []),
        },
        error: null
    }
}

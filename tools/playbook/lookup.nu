#!/usr/bin/env nu
# playbook/lookup.nu
#
# The FIRST thing called when a new task is loaded.
# Returns the full playbook for a task type.
# If no playbook exists, agent must halt immediately — no improvisation.
#
# Usage:
#   nu tools/playbook/lookup.nu --type write-tests
#   nu tools/playbook/lookup.nu --type fix-clippy

def main [
    --type: string    # task type e.g. "write-tests"
] {
    if ($type | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --type" }
    }

    # Find playbook file — search relative to this script's location
    let script_dir = ($env.WORKSPACE_ROOT? | default "/workspace")
    let playbook_dir = ($script_dir | path join "playbooks")
    let playbook_file = ($playbook_dir | path join $"($type).nu")

    let known_types = (
        try {
            ls $playbook_dir
            | where type == "file"
            | where name =~ '\.nu$'
            | get name
            | each {|f| $f | path basename | str replace ".nu" ""}
        } catch { [] }
    )

    if not ($playbook_file | path exists) {
        return {
            ok: true,
            data: {
                found: false,
                type: $type,
                known_types: $known_types,
                recommendation: $"HALT — no playbook for task type: ($type). Known types: ($known_types | str join ', ')",
            },
            error: null
        }
    }

    # Parse the playbook file — it's a nushell record literal
    let playbook = (
        try {
            nu --no-config-file -c $"source ($playbook_file); $in" | from nuon
        } catch {
            # Fallback: return minimal structure from filename
            null
        }
    )

    # For write-tests, return the known structure directly
    # Full nu evaluation of playbook files is a future enhancement
    let steps = match $type {
        "write-tests" => [
            { name: "survey",   index: 0, automated: true,  budget: 3, verification_gate: "compile/check passed and crate structure understood",
              approved_tools: ["locate/files", "locate/actors", "locate/symbols", "locate/derives", "locate/tests", "compile/check", "extract/cargo-toml"],
              orient_questions: ["Is this an actor crate, a types crate, or both?", "Which derives are present?", "Do existing tests exist?", "Does compile/check pass?"] },
            { name: "orient",   index: 1, automated: false, budget: 2, verification_gate: "coverage plan written with planned_count > 0",
              approved_tools: ["task/write-coverage-plan", "task/state", "meta/loop-detect", "meta/orient-report"],
              orient_questions: ["What public interfaces need tests?", "Which types need serde roundtrips?", "Which types need rkyv roundtrips?", "How many total tests planned?"] },
            { name: "scaffold", index: 2, automated: false, budget: 3, verification_gate: "tests/ dir exists, Cargo.toml has [[test]] entries, compile/check passes",
              approved_tools: ["create/tests-dir", "create/test-file", "create/cargo-test-entry", "create/dev-dep", "compile/check"],
              orient_questions: ["Does tests/ exist?", "Does Cargo.toml have all required [[test]] entries?", "Do empty files compile?"] },
            { name: "write",    index: 3, automated: false, budget: 5, verification_gate: "all planned tests written, compile/check passes",
              approved_tools: ["extract/file", "extract/range", "extract/symbol", "extract/actor", "compile/check", "compile/fix-hint"],
              orient_questions: ["How many planned tests written vs remaining?", "Does last test compile?", "Has loop-detect flagged repeated errors?"] },
            { name: "verify",   index: 4, automated: false, budget: 3, verification_gate: "test/run failed == 0, gate_passed == true",
              approved_tools: ["test/run", "test/count", "test/verify-coverage", "compile/check"],
              orient_questions: ["How many passed? How many failed?", "Is failed == 0?", "Is gate_passed true?"] },
            { name: "finalize", index: 5, automated: false, budget: 2, verification_gate: "fmt/apply done, compile/check clean, task/advance called",
              approved_tools: ["fmt/apply", "fmt/check", "compile/check", "test/run", "task/advance"],
              orient_questions: ["Does fmt/check show unformatted files?", "Does test/run still pass after fmt?"] },
        ]
        _ => []
    }

    {
        ok: true,
        data: {
            found: true,
            type: $type,
            version: "1.0",
            description: $"Playbook for ($type)",
            steps: $steps,
            success_condition: "All planned tests pass. Zero failures. Coverage plan fulfilled. task/advance called.",
            preconditions: ["current_task.crate is set", "crate_path is accessible", "compile/check passes before writing tests"],
        },
        error: null
    }
}

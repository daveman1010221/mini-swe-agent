#!/usr/bin/env nu
# locate/tests.nu
#
# Finds existing tests in a crate — what exists, what's missing.
# Always call before writing new tests to avoid duplication.
# Establishes the baseline test count.
#
# Usage:
#   nu tools/locate/tests.nu --crate-path /workspace/src/agents/cassini/types

def main [
    --crate-path: path   # Path to the crate root
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if not ($crate_path | path exists) {
        return { ok: false, data: null, error: $"path does not exist: ($crate_path)" }
    }

    let tests_dir = ($crate_path | path join "tests")
    let cargo_toml = ($crate_path | path join "Cargo.toml")

    let has_tests_dir = ($tests_dir | path exists)

    # Parse [[test]] entries from Cargo.toml
    let cargo_toml_declares = if ($cargo_toml | path exists) {
        try {
            open $cargo_toml
            | get test?
            | default []
            | each {|t| $t.name? | default "unnamed"}
        } catch {
            []
        }
    } else {
        []
    }

    if not $has_tests_dir {
        return {
            ok: true,
            data: {
                crate_path: $crate_path,
                has_tests_dir: false,
                test_files: [],
                total_tests: 0,
                has_unit: false,
                has_props: false,
                cargo_toml_declares: $cargo_toml_declares,
            },
            error: null
        }
    }

    # Find all .rs files in tests/
    let test_files = (
        try {
            ls $tests_dir
            | where type == "file"
            | where name =~ '\.rs$'
            | each {|f|
                let content = (open $f.name)
                let test_names = (
                    $content
                    | lines
                    | where {|l| $l =~ '^(pub |async |pub async )?fn test_'}
                    | each {|l|
                        $l
                        | str replace --regex '^.*(fn test_)' 'test_'
                        | str replace --regex '\(.*' ''
                        | str trim
                    }
                )

                {
                    file: ($f.name | path basename),
                    test_count: ($test_names | length),
                    test_names: $test_names,
                }
            }
        } catch {
            []
        }
    )

    let total_tests = (if ($test_files | is-empty) { 0 } else { $test_files | each {|f| $f.test_count} | math sum })
    let has_unit = ($test_files | any {|f| $f.file == "unit.rs"})
    let has_props = ($test_files | any {|f| $f.file == "props.rs"})

    {
        ok: true,
        data: {
            crate_path: $crate_path,
            has_tests_dir: true,
            test_files: $test_files,
            total_tests: $total_tests,
            has_unit: $has_unit,
            has_props: $has_props,
            cargo_toml_declares: $cargo_toml_declares,
        },
        error: null
    }
}

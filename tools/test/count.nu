#!/usr/bin/env nu
# test/count.nu
#
# Counts existing tests without running them.
# Call before writing new tests to establish baseline.
# Never rely on memory for test counts — always call this first.
#
# Usage:
#   nu tools/test/count.nu --crate-path /workspace/src/agents/cassini/types
#   nu tools/test/count.nu --crate-path /workspace/src/agents/cassini/types --target unit

def main [
    --crate-path: path,
    --target: string = "all"   # "unit" | "props" | "all"
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if not ($crate_path | path exists) {
        return { ok: false, data: null, error: $"path does not exist: ($crate_path)" }
    }

    let tests_dir = ($crate_path | path join "tests")

    if not ($tests_dir | path exists) {
        return {
            ok: true,
            data: {
                crate: ($crate_path | path basename),
                target: $target,
                unit_count: 0,
                props_count: 0,
                total: 0,
                by_file: [],
                message: "no tests/ directory found",
            },
            error: null
        }
    }

    # Find test files based on target filter
    let test_files = match $target {
        "unit"  => ([$tests_dir "unit.rs"  ] | path join | if ($in | path exists) { [$in] } else { [] }),
        "props" => ([$tests_dir "props.rs" ] | path join | if ($in | path exists) { [$in] } else { [] }),
        _       => (ls $tests_dir | where type == "file" | where name =~ '\.rs$' | get name),
    }

    let by_file = ($test_files | each {|file|
        let content = (open --raw $file)

        # Count test functions — handle both regular and async
        let test_names = (
            $content
            | lines
            | where {|l|
                ($l =~ '^(pub |async |pub async )?fn test_') or ($l =~ '#\[test\]') or ($l =~ '#\[tokio::test\]')
            }
            | where {|l| $l =~ '^fn test_|fn test_'}
            | each {|l|
                $l
                | str replace --regex "^.*(fn test_)" "test_"
                | str replace --regex "\(.*" ""
                | str trim
            }
        )

        # Also count proptest! blocks
        let proptest_count = (
            $content
            | lines
            | where ($it =~ "^    fn prop_|^    fn test_")
            | length
        )

        let file_name = ($file | path basename)

        {
            file: $file_name,
            count: ($test_names | length),
            proptest_count: $proptest_count,
            test_names: $test_names,
        }
    })

    let unit_file = ($by_file | where file == "unit.rs" | first 1)
    let props_file = ($by_file | where file == "props.rs" | first 1)

    let unit_count = if ($unit_file | length) > 0 {
        ($unit_file | get 0.count)
    } else { 0 }

    let props_count = if ($props_file | length) > 0 {
        ($props_file | get 0.proptest_count)
    } else { 0 }

    let total = ($by_file | each {|f| $f.count + $f.proptest_count} | math sum | default 0)

    {
        ok: true,
        data: {
            crate: ($crate_path | path basename),
            target: $target,
            unit_count: $unit_count,
            props_count: $props_count,
            total: $total,
            by_file: $by_file,
        },
        error: null
    }
}

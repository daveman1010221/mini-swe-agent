#!/usr/bin/env nu
# create/test-file.nu
#
# Creates a new test file for a crate with correct scaffolding.
# Checks preconditions before creating anything.
# Never overwrites an existing file.
#
# Usage:
#   nu tools/create/test-file.nu --crate-path /workspace/crates/core --target unit --imports '["TaskFile", "CurrentTask"]'
#   nu tools/create/test-file.nu --crate-path /workspace/crates/core --target props --imports '["TaskFile"]'

def main [
    --crate-path: path,
    --target: string,           # "unit" | "props"
    --imports: string = "[]"    # JSON array of items to import from the crate
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if ($target | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --target (unit or props)" }
    }

    if not ($target == "unit" or $target == "props") {
        return { ok: false, data: null, error: $"invalid target: ($target) — must be 'unit' or 'props'" }
    }

    if not ($crate_path | path exists) {
        return { ok: false, data: null, error: $"crate-path not found: ($crate_path)" }
    }

    let crate_name = ($crate_path | path basename)
    let tests_dir  = ($crate_path | path join "tests")
    let file_path  = ($tests_dir | path join $"($target).rs")

    # Precondition 1: tests/ dir must exist
    if not ($tests_dir | path exists) {
        return {
            ok: false,
            data: {
                failed_precondition: "tests/ directory does not exist",
                attempted_fix: false,
                fix_result: "call create/tests-dir first",
            },
            error: "precondition failed: tests/ directory does not exist — call create/tests-dir first"
        }
    }

    # Precondition 2: file must not already exist
    if ($file_path | path exists) {
        return {
            ok: true,
            data: {
                created: false,
                file: $file_path,
                already_existed: true,
                cargo_toml_updated: false,
                scaffold: "",
                message: $"($file_path | path basename) already exists — append to it, do not recreate",
            },
            error: null
        }
    }

    # Parse imports
    let import_list = (try { $imports | from json } catch { [] })

    # Build scaffold content
    let crate_name_snake = ($crate_name | str replace "-" "_")

    let scaffold = if $target == "unit" {
        let import_line = if ($import_list | length) > 0 {
            $"use ($crate_name_snake)::{($import_list | str join ', ')};\n"
        } else {
            $"// use ($crate_name_snake)::YourType;\n"
        }

$"($import_line)
#[cfg(test)]
mod tests {
    use super::*;

    // Add your unit tests here.
    // Run with: cargo test --package ($crate_name) --test unit
}
"
    } else {
        let import_line = if ($import_list | length) > 0 {
            $"use ($crate_name_snake)::{($import_list | str join ', ')};\n"
        } else {
            $"// use ($crate_name_snake)::YourType;\n"
        }

$"use proptest::prelude::*;
($import_line)
proptest! {
    // Add your property tests here.
    // Run with: cargo test --package ($crate_name) --test props
    //
    // Example:
    // #[test]
    // fn prop_example(s in \".*\") {
    //     prop_assert!(my_function(&s).is_ok());
    // }
}
"
    }

    # Write the file
    try {
        $scaffold | save --force $file_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to write ($file_path): ($err.msg)" }
    }

    # Check if Cargo.toml already has the [[test]] entry
    let cargo_toml_path = ($crate_path | path join "Cargo.toml")
    let cargo_toml_updated = false  # create/cargo-test-entry handles this

    {
        ok: true,
        data: {
            created: true,
            file: $file_path,
            already_existed: false,
            cargo_toml_updated: $cargo_toml_updated,
            scaffold: $scaffold,
            message: $"✓ Created ($file_path | path basename) — now call create/cargo-test-entry",
        },
        error: null
    }
}

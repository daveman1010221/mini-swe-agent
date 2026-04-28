#!/usr/bin/env nu
# create/cargo-test-entry.nu
#
# Adds a [[test]] entry to a crate's Cargo.toml.
# Checks for existing entry before adding — idempotent.
# Call after create/test-file.
#
# Usage:
#   nu tools/create/cargo-test-entry.nu --crate-path /workspace/crates/core --name unit --path tests/unit.rs
#   nu tools/create/cargo-test-entry.nu --crate-path /workspace/crates/core --name props --path tests/props.rs

def main [
    --crate-path: path,
    --name: string,    # test binary name e.g. "unit"
    --path: string,    # e.g. "tests/unit.rs"
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if ($name | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --name" }
    }

    if ($path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --path" }
    }

    let cargo_toml_path = ($crate_path | path join "Cargo.toml")

    if not ($cargo_toml_path | path exists) {
        return { ok: false, data: null, error: $"Cargo.toml not found at: ($cargo_toml_path)" }
    }

    let content = (open --raw $cargo_toml_path)

    # Check if entry already exists
    let already_exists = ($content =~ $"\\[\\[test\\]\\][\\s\\S]*?name = \"($name)\"")

    if $already_exists {
        return {
            ok: true,
            data: {
                added: false,
                already_existed: true,
                entry: "",
                message: $"[[test]] entry '($name)' already exists in Cargo.toml",
            },
            error: null
        }
    }

    let entry = $"
[[test]]
name = \"($name)\"
path = \"($path)\"
"

    # Append to Cargo.toml
    try {
        $content + $entry | save --force $cargo_toml_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to update Cargo.toml: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            added: true,
            already_existed: false,
            entry: $entry,
            message: $"✓ Added [[test]] entry '($name)' to Cargo.toml",
        },
        error: null
    }
}

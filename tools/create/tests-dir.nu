#!/usr/bin/env nu
# create/tests-dir.nu
#
# Creates the tests/ directory structure for a crate that has none.
# Safe to call even if tests/ already exists — idempotent.
# Call this before create/test-file.
#
# Usage:
#   nu tools/create/tests-dir.nu --crate-path /workspace/crates/core

def main [
    --crate-path: path
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if not ($crate_path | path exists) {
        return { ok: false, data: null, error: $"crate-path not found: ($crate_path)" }
    }

    let tests_dir = ($crate_path | path join "tests")
    let already_existed = ($tests_dir | path exists)

    if not $already_existed {
        try {
            mkdir $tests_dir
        } catch {|err|
            return { ok: false, data: null, error: $"failed to create tests/: ($err.msg)" }
        }
    }

    {
        ok: true,
        data: {
            created: (not $already_existed),
            already_existed: $already_existed,
            path: $tests_dir,
            message: (if $already_existed { "tests/ already exists" } else { "✓ Created tests/" }),
        },
        error: null
    }
}

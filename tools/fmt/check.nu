#!/usr/bin/env nu
# fmt/check.nu
#
# Checks formatting without modifying files.
# Returns list of files that would be changed by rustfmt.
# Always run this before fmt/apply to see what will change.
#
# Usage:
#   nu tools/fmt/check.nu --workspace-root /workspace --crate mswea-core

def main [
    --workspace-root: path,
    --crate: string = ""    # optional: check specific crate, default all
] {
    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }

    if not ($workspace_root | path exists) {
        return { ok: false, data: null, error: $"workspace-root not found: ($workspace_root)" }
    }

    let check_args = if ($crate | str length) > 0 {
        ["--package" $crate]
    } else {
        ["--all"]
    }

    let result = (
        try {
            do { cd $workspace_root; cargo fmt ...$check_args -- --check 2>&1 } | complete
        } catch {|err|
            return { ok: false, data: null, error: $"failed to run cargo fmt: ($err.msg)" }
        }
    )

    # cargo fmt --check exits 1 and prints filenames of unformatted files
    let unformatted = if $result.exit_code != 0 {
        $result.stdout
        | lines
        | where ($it | str length) > 0
        | where ($it =~ "\.rs$" or $it =~ "Diff")
        | each {|l| $l | str trim}
    } else {
        []
    }

    let clean = $result.exit_code == 0

    {
        ok: true,
        data: {
            crate: (if ($crate | str length) > 0 { $crate } else { "workspace" }),
            clean: $clean,
            unformatted_files: $unformatted,
            message: (if $clean { "✓ All files formatted" } else { $"✗ ($unformatted | length) file(s) need formatting" }),
        },
        error: null
    }
}

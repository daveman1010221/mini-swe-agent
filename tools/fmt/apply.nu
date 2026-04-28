#!/usr/bin/env nu
# fmt/apply.nu
#
# Applies rustfmt to a crate or workspace.
# Returns list of files that were changed.
# Always run compile/check after this — fmt can rarely introduce issues.
#
# Usage:
#   nu tools/fmt/apply.nu --workspace-root /workspace --crate mswea-core

def main [
    --workspace-root: path,
    --crate: string = ""    # optional: format specific crate, default all
] {
    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }

    if not ($workspace_root | path exists) {
        return { ok: false, data: null, error: $"workspace-root not found: ($workspace_root)" }
    }

    # Check which files would change before applying
    let check_args = if ($crate | str length) > 0 {
        ["--package" $crate]
    } else {
        ["--all"]
    }

    let before = (
        try {
            cd $workspace_root
            cargo fmt ...$check_args -- --check | complete
        } catch { {exit_code: 0, stdout: ""} }
    )

    let unformatted_before = if $before.exit_code != 0 {
        $before.stdout | lines | where ($it | str length) > 0 | each {|l| $l | str trim}
    } else {
        []
    }

    # Apply formatting
    let result = (
        try {
            cd $workspace_root
            cargo fmt ...$check_args | complete
        } catch {|err|
            return { ok: false, data: null, error: $"failed to run cargo fmt: ($err.msg)" }
        }
    )

    if $result.exit_code != 0 {
        return {
            ok: false,
            data: null,
            error: $"cargo fmt failed: ($result.stdout)"
        }
    }

    {
        ok: true,
        data: {
            crate: (if ($crate | str length) > 0 { $crate } else { "workspace" }),
            files_changed: $unformatted_before,
            count: ($unformatted_before | length),
            message: (if ($unformatted_before | length) > 0 {
                $"✓ Formatted ($unformatted_before | length) file(s)"
            } else {
                "✓ All files were already formatted"
            }),
        },
        error: null
    }
}

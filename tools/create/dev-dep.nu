#!/usr/bin/env nu
# create/dev-dep.nu
#
# Adds a dev-dependency to a crate's Cargo.toml.
# Checks workspace Cargo.toml first — always prefers workspace = true form.
# Never adds a pinned version if the workspace form is available.
#
# Usage:
#   nu tools/create/dev-dep.nu --crate-path /workspace/crates/core --dep proptest --workspace-root /workspace
#   nu tools/create/dev-dep.nu --crate-path /workspace/crates/core --dep tempfile --workspace-root /workspace

def main [
    --crate-path: path,
    --dep: string,           # dependency name e.g. "proptest"
    --workspace-root: path   # to check workspace Cargo.toml
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if ($dep | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --dep" }
    }

    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }

    let cargo_toml_path = ($crate_path | path join "Cargo.toml")
    let ws_toml_path    = ($workspace_root | path join "Cargo.toml")

    if not ($cargo_toml_path | path exists) {
        return { ok: false, data: null, error: $"Cargo.toml not found: ($cargo_toml_path)" }
    }

    let content = (open --raw $cargo_toml_path)

    # Check if already declared
    let dep_snake = ($dep | str replace "-" "_")
    let already_exists = (
        $content =~ $"\[dev-dependencies\][\\s\\S]*?($dep|$dep_snake)" or
        $content =~ $"($dep|$dep_snake)[\\s\\S]*?\[dev-dependencies\]" |
        # simpler: just check if dep name appears after [dev-dependencies]
        false  # will verify below more carefully
    )

    # Better check: parse toml
    let toml = (open $cargo_toml_path)
    let dev_deps = ($toml | get "dev-dependencies"? | default {})
    let already_exists = ($dev_deps | columns | any {|c| $c == $dep or $c == $dep_snake})

    if $already_exists {
        return {
            ok: true,
            data: {
                added: false,
                already_existed: true,
                form: "already-present",
                entry: "",
                workspace_available: false,
                message: $"($dep) is already in [dev-dependencies]",
            },
            error: null
        }
    }

    # Check workspace availability
    let workspace_available = if ($ws_toml_path | path exists) {
        let ws_toml = (open $ws_toml_path)
        let ws_deps = ($ws_toml | get workspace.dependencies? | default {})
        ($ws_deps | columns | any {|c| $c == $dep or $c == $dep_snake})
    } else {
        false
    }

    let (form, entry) = if $workspace_available {
        let e = $"\n($dep) = { workspace = true }"
        ["workspace" $e]
    } else {
        # Can't add without knowing the version — surface this to agent
        return {
            ok: false,
            data: {
                added: false,
                already_existed: false,
                form: "unknown",
                entry: "",
                workspace_available: false,
                message: $"($dep) is not in workspace Cargo.toml — add it there first with a version, then re-run this tool",
            },
            error: $"($dep) not found in workspace dependencies — add to workspace Cargo.toml first"
        }
    }

    # Append to [dev-dependencies] section or add section
    let has_dev_deps_section = ($content =~ "\[dev-dependencies\]")

    let new_content = if $has_dev_deps_section {
        $content | str replace "[dev-dependencies]" $"[dev-dependencies]\n($dep) = \{ workspace = true \}"
    } else {
        $content + $"\n[dev-dependencies]\n($dep) = \{ workspace = true \}\n"
    }

    try {
        $new_content | save --force $cargo_toml_path
    } catch {|err|
        return { ok: false, data: null, error: $"failed to update Cargo.toml: ($err.msg)" }
    }

    {
        ok: true,
        data: {
            added: true,
            already_existed: false,
            form: $form,
            entry: $entry,
            workspace_available: $workspace_available,
            message: $"✓ Added ($dep) = \{ workspace = true \} to [dev-dependencies]",
        },
        error: null
    }
}

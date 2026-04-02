#!/usr/bin/env nu
# locate/deps.nu
#
# Finds dependencies declared in a crate's Cargo.toml.
# Call before adding dev-dependencies to check workspace vs local.
# Never add a pinned version if workspace form is available.
#
# Usage:
#   nu tools/locate/deps.nu --crate-path /workspace/crates/core
#   nu tools/locate/deps.nu --crate-path /workspace/crates/core --kind dev-deps

def main [
    --crate-path: path,
    --kind: string = "all"   # "all" | "deps" | "dev-deps" | "build-deps"
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    let cargo_toml_path = ($crate_path | path join "Cargo.toml")

    if not ($cargo_toml_path | path exists) {
        return { ok: false, data: null, error: $"Cargo.toml not found at: ($cargo_toml_path)" }
    }

    let toml = (open $cargo_toml_path)

    # Find workspace root by walking up
    let workspace_root = (
        try {
            let output = (do { cd $crate_path; cargo metadata --no-deps --format-version 1 2>/dev/null } | from json)
            $output.workspace_root? | default ""
        } catch {
            ""
        }
    )

    # Get workspace deps if available
    let workspace_deps = if ($workspace_root | str length) > 0 {
        let ws_toml = ($workspace_root | path join "Cargo.toml")
        if ($ws_toml | path exists) {
            try {
                open $ws_toml
                | get workspace.dependencies?
                | default {}
                | columns
            } catch {
                []
            }
        } else {
            []
        }
    } else {
        []
    }

    # Parse deps from toml
    def parse_deps [section: string, dep_kind: string] {
        let raw = ($toml | get $section? | default {})
        $raw | transpose name spec | each {|row|
            let is_workspace = (
                ($row.spec | describe) == "record" and
                (($row.spec | get workspace? | default false) == true)
            )
            let version = if $is_workspace {
                "workspace"
            } else if ($row.spec | describe) == "string" {
                $row.spec
            } else {
                $row.spec | get version? | default "*"
            }
            let features = if ($row.spec | describe) == "record" {
                $row.spec | get features? | default []
            } else {
                []
            }
            {
                name: $row.name,
                kind: $dep_kind,
                version: $version,
                workspace: $is_workspace,
                features: $features,
            }
        }
    }

    let normal_deps    = (parse_deps "dependencies" "normal")
    let dev_deps       = (parse_deps "dev-dependencies" "dev")
    let build_deps     = (parse_deps "build-dependencies" "build")

    let all_deps = match $kind {
        "deps"       => $normal_deps,
        "dev-deps"   => $dev_deps,
        "build-deps" => $build_deps,
        _            => ($normal_deps | append $dev_deps | append $build_deps),
    }

    {
        ok: true,
        data: {
            crate_path: $crate_path,
            workspace_root: $workspace_root,
            kind_filter: $kind,
            deps: $all_deps,
            workspace_deps: $workspace_deps,
        },
        error: null
    }
}

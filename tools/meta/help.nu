#!/usr/bin/env nu
# meta/help.nu
#
# Returns usage documentation for any tool in the toolbox.
# Call this in ORIENT when you need to understand a tool's flags,
# expected arguments, or output shape before calling it.
#
# Usage:
#   nu tools/meta/help.nu --tool locate/files
#   nu tools/meta/help.nu --tool meta/loop-detect

def main [
    --tool: string,   # Tool name in namespace/name format (e.g. locate/files)
] {
    if ($tool | is-empty) {
        return {
            ok: false,
            data: null,
            error: "missing required flag: --tool"
        }
    }

    let parts = ($tool | split row "/")
    if ($parts | length) != 2 {
        return {
            ok: false,
            data: null,
            error: $"invalid tool name: ($tool) — expected namespace/name format"
        }
    }

    let namespace = ($parts | first)
    let name      = ($parts | last)
    let script    = $"/workspace/tools/($namespace)/($name).nu"

    if not ($script | path exists) {
        return {
            ok: false,
            data: null,
            error: $"tool not found: ($tool) — no script at ($script)"
        }
    }

    let result = (do { nu $script --help } | complete)

    {
        ok: true,
        data: {
            tool: $tool,
            script: $script,
            help: $result.stdout,
        },
        error: null
    }
}

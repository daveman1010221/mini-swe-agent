#!/usr/bin/env nu
# locate/files.nu
#
# Lists source files in a crate or directory.
# Starting point when exploring unfamiliar code.
# Always returns structured data — never raw text.
#
# Usage:
#   nu tools/locate/files.nu --crate-path /workspace/src/agents/cassini/types
#   nu tools/locate/files.nu --crate-path /workspace/src/agents/cassini/types --pattern "*.rs"

def main [
    --crate-path: path,      # Path to the crate or directory to list
    --pattern: string = "*.rs"  # Glob pattern (default: *.rs)
] {
    # Validate input
    if ($crate_path | is-empty) {
        return {
            ok: false,
            data: null,
            error: "missing required flag: --crate-path"
        }
    }

    if not ($crate_path | path exists) {
        return {
            ok: false,
            data: null,
            error: $"path does not exist: ($crate_path)"
        }
    }

    # Find files matching pattern
    let files = (
        try {
            fd --type f --glob $pattern $crate_path
            | lines
            | where ($it | str length) > 0
            | each {|f|
                let info = ($f | path parse)
                let stat = (ls $f | first)
                {
                    path: ($f | str replace $"($crate_path)/" ""),
                    full_path: $f,
                    name: $info.stem,
                    extension: $info.extension,
                    size_bytes: ($stat.size | into int),
                    modified: ($stat.modified | format date "%Y-%m-%dT%H:%M:%SZ"),
                }
            }
        } catch {|err|
            return {
                ok: false,
                data: null,
                error: $"Failed to list files: ($err.msg)"
            }
        }
    )

    {
        ok: true,
        data: {
            crate_path: ($crate_path | str replace ($env.HOME? | default "") "~"),
            pattern: $pattern,
            count: ($files | length),
            files: $files,
        },
        error: null
    }
}

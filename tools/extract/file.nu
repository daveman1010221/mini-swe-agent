#!/usr/bin/env nu
# extract/file.nu
#
# Reads a complete file with line numbers.
# Use only for small files or when you need full context.
# Prefer extract/range or extract/symbol for large files (> 200 lines).
#
# Usage:
#   nu tools/extract/file.nu --file /workspace/crates/core/src/config.rs

def main [
    --file: path,
    --workspace-root: path = ""   # for relative path display
] {
    if ($file | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --file" }
    }

    if not ($file | path exists) {
        return { ok: false, data: null, error: $"file not found: ($file)" }
    }

    let content = (open --raw $file)
    let lines = ($content | lines)
    let line_count = ($lines | length)
    let size_bytes = ($file | path expand | ls $in | get size.0 | into int)

    let warning = if $line_count > 200 {
        $"large file ($line_count lines) — consider extract/range or extract/symbol for specific sections"
    } else {
        null
    }

    # Add line numbers
    let numbered = (
        $lines
        | enumerate
        | each {|row| $"($row.index + 1 | fill --alignment right --width 4)  ($row.item)"}
        | str join "\n"
    )

    let display_path = if ($workspace_root | str length) > 0 {
        $file | str replace $"($workspace_root)/" ""
    } else {
        $file
    }

    {
        ok: true,
        data: {
            file: $display_path,
            line_count: $line_count,
            size_bytes: $size_bytes,
            content: $numbered,
            warning: $warning,
        },
        error: null
    }
}

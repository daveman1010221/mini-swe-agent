#!/usr/bin/env nu
# extract/range.nu
#
# Reads a specific line range from a file with context lines.
# Use when locate/ gives you a line number — read context around it.
# Much cheaper than extract/file for large files.
#
# Usage:
#   nu tools/extract/range.nu --file /workspace/crates/core/src/config.rs --start 42 --end 80
#   nu tools/extract/range.nu --file /workspace/crates/core/src/config.rs --start 42 --end 80 --context 10

def main [
    --file: path,
    --start: int,
    --end: int,
    --context: int = 5    # extra lines above/below the requested range
] {
    if ($file | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --file" }
    }

    if not ($file | path exists) {
        return { ok: false, data: null, error: $"file not found: ($file)" }
    }

    let lines = (open --raw $file | lines)
    let total_lines = ($lines | length)

    let actual_start = ([1 ($start - $context)] | math max)
    let actual_end   = ([$total_lines ($end + $context)] | math min)

    let content = (
        $lines
        | enumerate
        | skip ($actual_start - 1)
        | first ($actual_end - $actual_start + 1)
        | each {|row|
            let line_num = $row.index + 1
            let marker = if $line_num >= $start and $line_num <= $end { ">" } else { " " }
            $"($line_num | fill --alignment right --width 4)($marker) ($row.item)"
        }
        | str join "\n"
    )

    {
        ok: true,
        data: {
            file: $file,
            requested_start: $start,
            requested_end: $end,
            actual_start: $actual_start,
            actual_end: $actual_end,
            total_lines: $total_lines,
            content: $content,
        },
        error: null
    }
}

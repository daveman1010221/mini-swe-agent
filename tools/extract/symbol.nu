#!/usr/bin/env nu
# extract/symbol.nu
#
# Extracts the complete definition of a named symbol.
# Handles fn, struct, enum, impl blocks, trait impls.
# Finds matching braces — you get the whole thing.
#
# Usage:
#   nu tools/extract/symbol.nu --file /workspace/crates/core/src/config.rs --symbol TaskFile
#   nu tools/extract/symbol.nu --file /workspace/crates/actors/src/orchestrator.rs --symbol OrchestratorActor --kind impl

def main [
    --file: path,
    --symbol: string,
    --kind: string = "any"   # "fn" | "struct" | "enum" | "impl" | "trait" | "any"
] {
    if ($file | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --file" }
    }

    if ($symbol | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --symbol" }
    }

    if not ($file | path exists) {
        return { ok: false, data: null, error: $"file not found: ($file)" }
    }

    let lines = (open --raw $file | lines)
    let total = ($lines | length)

    # Build search pattern based on kind
    let pattern = match $kind {
        "fn"     => $"(pub |pub\\(crate\\) |pub\\(super\\) )?( async )?fn ($symbol)",
        "struct" => $"(pub |pub\\(crate\\) )?struct ($symbol)",
        "enum"   => $"(pub |pub\\(crate\\) )?enum ($symbol)",
        "impl"   => $"impl(<[^>]*>)? (.*for )?($symbol)",
        "trait"  => $"(pub |pub\\(crate\\) )?trait ($symbol)",
        _        => $"(fn|struct|enum|impl|trait)\\s+($symbol)|impl(<[^>]*>)?\\s+(.*for\\s+)?($symbol)",
    }

    # Find the start line
    let start_line = (
        $lines
        | enumerate
        | where {|row| $row.item =~ $pattern}
        | first 1
        | get 0?
    )

    if $start_line == null {
        return {
            ok: false,
            data: null,
            error: $"symbol '($symbol)' not found in ($file) (kind: ($kind))"
        }
    }

    let start_idx = $start_line.index
    let sym_kind = if ($start_line.item =~ "fn ") {
        "fn"
    } else if ($start_line.item =~ "struct ") {
        "struct"
    } else if ($start_line.item =~ "enum ") {
        "enum"
    } else if ($start_line.item =~ "impl ") {
        "impl"
    } else if ($start_line.item =~ "trait ") {
        "trait"
    } else {
        "unknown"
    }

    # Find end by counting braces
    let end_idx = (
        $lines
        | skip $start_idx
        | enumerate
        | reduce --fold {idx: $start_idx, depth: 0, found: false} {|row, acc|
            if $acc.found { $acc } else {
                let open_count  = ($row.item | split chars | where ($it == "{") | length)
                let close_count = ($row.item | split chars | where ($it == "}") | length)
                let new_depth   = $acc.depth + $open_count - $close_count
                let abs_idx     = $start_idx + $row.index

                if $new_depth <= 0 and $acc.depth > 0 {
                    {idx: $abs_idx, depth: $new_depth, found: true}
                } else if $new_depth > 0 or $open_count > 0 {
                    {idx: $abs_idx, depth: $new_depth, found: false}
                } else {
                    $acc
                }
            }
        }
        | get idx
    )

    let content = (
        $lines
        | enumerate
        | skip $start_idx
        | first ($end_idx - $start_idx + 1)
        | each {|row| $"($start_idx + $row.index + 1 | fill --alignment right --width 4)  ($row.item)"}
        | str join "\n"
    )

    {
        ok: true,
        data: {
            file: $file,
            symbol: $symbol,
            kind: $sym_kind,
            start_line: ($start_idx + 1),
            end_line: ($end_idx + 1),
            content: $content,
        },
        error: null
    }
}

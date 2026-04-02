#!/usr/bin/env nu
# locate/symbols.nu
#
# Finds public symbols (fns, structs, enums, traits) in a crate.
# Use before writing tests to understand the public API surface.
#
# Usage:
#   nu tools/locate/symbols.nu --crate-path /workspace/crates/core/src
#   nu tools/locate/symbols.nu --crate-path /workspace/crates/core/src --kind struct

def main [
    --crate-path: path,
    --kind: string = "all",   # "fn" | "struct" | "enum" | "trait" | "all"
    --pattern: string = ""    # optional grep pattern to narrow results
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if not ($crate_path | path exists) {
        return { ok: false, data: null, error: $"path does not exist: ($crate_path)" }
    }

    let src_path = ($crate_path | path join "src")
    let search_path = if ($src_path | path exists) { $src_path } else { $crate_path }

    # Build rg pattern based on kind
    let rg_pattern = match $kind {
        "fn"     => "^pub (async )?fn ",
        "struct" => "^pub struct ",
        "enum"   => "^pub enum ",
        "trait"  => "^pub trait ",
        _        => "^pub (async fn |fn |struct |enum |trait )",
    }

    let raw_lines = (
        try {
            rg -n $rg_pattern $search_path --type rust
            | lines
            | where ($it | str length) > 0
        } catch {
            []
        }
    )

    let symbols = ($raw_lines | each {|line|
        # Parse: path/to/file.rs:42:pub struct Foo {
        let colon_parts = ($line | split row ":" | first 3)
        let file = ($colon_parts | get 0? | default "unknown")
        let line_num = ($colon_parts | get 1? | default "0" | into int)
        let code = ($colon_parts | get 2? | default "" | str trim)

        # Determine kind
        let sym_kind = if ($code =~ "^pub (async )?fn ") {
            "fn"
        } else if ($code =~ "^pub struct ") {
            "struct"
        } else if ($code =~ "^pub enum ") {
            "enum"
        } else if ($code =~ "^pub trait ") {
            "trait"
        } else {
            "other"
        }

        # Extract name
        let name = (
            $code
            | str replace --regex "^pub (async )?fn " ""
            | str replace --regex "^pub (struct|enum|trait) " ""
            | str replace --regex "[\s<({].*" ""
            | str trim
        )

        # Determine visibility
        let visibility = if ($code =~ "^pub\(crate\)") {
            "pub(crate)"
        } else if ($code =~ "^pub") {
            "pub"
        } else {
            "private"
        }

        {
            name: $name,
            kind: $sym_kind,
            file: ($file | str replace $"($crate_path)/" "" | str replace $"($search_path)/" ""),
            line: $line_num,
            visibility: $visibility,
            signature: ($code | str substring 0..120),
        }
    })

    # Apply optional pattern filter
    let filtered = if ($pattern | str length) > 0 {
        $symbols | where {|s| $s.name =~ $pattern or $s.signature =~ $pattern}
    } else {
        $symbols
    }

    {
        ok: true,
        data: {
            crate_path: $crate_path,
            kind_filter: $kind,
            count: ($filtered | length),
            symbols: $filtered,
        },
        error: null
    }
}

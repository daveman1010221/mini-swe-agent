#!/usr/bin/env nu
# locate/derives.nu
#
# Finds all derive macros used in a crate.
# Critical for deciding what tests to write — serde? rkyv? both?
# Call this during survey before writing any tests.
#
# Usage:
#   nu tools/locate/derives.nu --crate-path /workspace/src/agents/cassini/types

def main [
    --crate-path: path   # Path to the crate root
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    if not ($crate_path | path exists) {
        return { ok: false, data: null, error: $"path does not exist: ($crate_path)" }
    }

    let src_path = ($crate_path | path join "src")
    let search_path = if ($src_path | path exists) { $src_path } else { $crate_path }

    # Find all derive attributes
    let derive_lines = (
        try {
            rg -n '#\[derive\(' $search_path --type rust
            | lines
            | where ($it | str length) > 0
        } catch {
            []
        }
    )

    if ($derive_lines | is-empty) {
        return {
            ok: true,
            data: {
                crate_path: $crate_path,
                has_serde: false,
                has_rkyv: false,
                has_partial_eq: false,
                has_debug: false,
                has_clone: false,
                types_with_serde: [],
                types_with_rkyv: [],
                types_with_both: [],
                all_derives: [],
            },
            error: null
        }
    }

    # Parse each derive line
    let all_derives = ($derive_lines | each {|line|
        # Parse: file.rs:42:    #[derive(Serialize, Deserialize, Clone)]
        let parts = ($line | split column ":" file_part line_num rest)
        let file = ($parts | get file_part.0? | default "unknown")
        let line_num = ($parts | get line_num.0? | default "0" | into int)

        # Extract derive list
        let derives_str = (
            $line
            | parse --regex '#\[derive\((?P<derives>[^)]+)\)'
            | get derives.0?
            | default ""
        )

        let derives = (
            $derives_str
            | split row ","
            | each {|d| $d | str trim}
            | where ($it | str length) > 0
        )

        # Try to find the type name (look for struct/enum above this line)
        {
            file: ($file | str replace $"($crate_path)/" ""),
            line: $line_num,
            derives: $derives,
            type_name: "unknown",  # Simplified — full parse would need multi-line context
        }
    })

    # Aggregate flags
    let all_derive_names = ($all_derives | each {|d| $d.derives} | flatten)

    let has_serde     = ($all_derive_names | any {|d| $d == "Serialize" or $d == "Deserialize"})
    let has_rkyv      = ($all_derive_names | any {|d| $d == "Archive" or ($d =~ 'RkyvSerialize') or ($d =~ 'RkyvDeserialize')})
    let has_partial_eq = ($all_derive_names | any {|d| $d == "PartialEq"})
    let has_debug     = ($all_derive_names | any {|d| $d == "Debug"})
    let has_clone     = ($all_derive_names | any {|d| $d == "Clone"})

    # Files with serde derives
    let types_with_serde = (
        $all_derives
        | where {|d| $d.derives | any {|x| $x == "Serialize" or $x == "Deserialize"}}
        | get file
        | uniq
    )

    # Files with rkyv derives
    let types_with_rkyv = (
        $all_derives
        | where {|d| $d.derives | any {|x| $x == "Archive" or $x =~ "Rkyv"}}
        | get file
        | uniq
    )

    # Files with both
    let types_with_both = (
        $types_with_serde
        | where {|f| $types_with_rkyv | any {|r| $r == $f}}
    )

    {
        ok: true,
        data: {
            crate_path: $crate_path,
            has_serde: $has_serde,
            has_rkyv: $has_rkyv,
            has_partial_eq: $has_partial_eq,
            has_debug: $has_debug,
            has_clone: $has_clone,
            types_with_serde: $types_with_serde,
            types_with_rkyv: $types_with_rkyv,
            types_with_both: $types_with_both,
            all_derives: $all_derives,
        },
        error: null
    }
}

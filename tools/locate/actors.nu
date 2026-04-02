#!/usr/bin/env nu
# locate/actors.nu
#
# Finds all ractor Actor implementations in a crate.
# Returns enough to know what to read next — not the full source.
# Call this first when writing tests for an actor crate.
#
# Usage:
#   nu tools/locate/actors.nu --crate-path /workspace/src/agents/cassini/broker

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

    # Find files containing Actor implementations
    let actor_files = (
        try {
            rg -l "impl Actor for" $search_path --type rust
            | lines
            | where ($it | str length) > 0
        } catch {
            []
        }
    )

    if ($actor_files | is-empty) {
        return {
            ok: true,
            data: {
                crate_path: $crate_path,
                count: 0,
                actors: [],
            },
            error: null
        }
    }

    # For each file, extract actor details
    let actors = ($actor_files | each {|file|
        let content = (open $file)

        # Find Actor impl blocks
        let impl_lines = (
            $content
            | lines
            | enumerate
            | where {|row| $row.item =~ "impl Actor for"}
            | each {|row|
                let actor_name = (
                    $row.item
                    | parse "impl Actor for {name}"
                    | get name.0?
                    | default ($row.item | str replace --all "impl Actor for" "" | str trim | str replace --regex "\\s.*" "")
                )

                # Look for associated types near this line
                let nearby = (
                    $content
                    | lines
                    | skip ($row.index)
                    | first 20
                )

                let msg_type = (
                    $nearby
                    | where ($it =~ "type Msg =")
                    | first 1
                    | each {|l| $l | str replace --regex ".*type Msg = " "" | str replace ";" "" | str trim}
                    | get 0?
                    | default "unknown"
                )

                let state_type = (
                    $nearby
                    | where ($it =~ "type State =")
                    | first 1
                    | each {|l| $l | str replace --regex ".*type State = " "" | str replace ";" "" | str trim}
                    | get 0?
                    | default "unknown"
                )

                let args_type = (
                    $nearby
                    | where ($it =~ "type Arguments =")
                    | first 1
                    | each {|l| $l | str replace --regex ".*type Arguments = " "" | str replace ";" "" | str trim}
                    | get 0?
                    | default "unknown"
                )

                {
                    name: $actor_name,
                    file: ($file | str replace $"($crate_path)/" ""),
                    line: ($row.index + 1),
                    msg_type: $msg_type,
                    state_type: $state_type,
                    args_type: $args_type,
                    has_pre_start: ($nearby | any {|l| $l =~ "fn pre_start"}),
                    has_post_stop: ($nearby | any {|l| $l =~ "fn post_stop"}),
                    has_handle: ($nearby | any {|l| $l =~ "fn handle"}),
                    has_supervisor_evt: ($nearby | any {|l| $l =~ "fn handle_supervisor_evt"}),
                }
            }
        )

        $impl_lines
    } | flatten)

    {
        ok: true,
        data: {
            crate_path: $crate_path,
            count: ($actors | length),
            actors: $actors,
        },
        error: null
    }
}

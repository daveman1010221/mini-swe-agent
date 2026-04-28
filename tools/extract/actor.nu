#!/usr/bin/env nu
# extract/actor.nu
#
# Extracts everything needed to write tests for a ractor actor.
# Combines locate/actors + extract/symbol into one structured result.
# The single best tool to call before writing actor tests.
#
# Usage:
#   nu tools/extract/actor.nu --file /workspace/crates/actors/src/orchestrator.rs --actor OrchestratorActor

def main [
    --file: path,
    --actor: string    # actor struct name e.g. "OrchestratorActor"
] {
    if ($file | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --file" }
    }

    if ($actor | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --actor" }
    }

    if not ($file | path exists) {
        return { ok: false, data: null, error: $"file not found: ($file)" }
    }

    let lines = (open --raw $file | lines)
    let content = ($lines | str join "\n")

    # Find impl Actor for <actor>
    let impl_line = (
        $lines
        | enumerate
        | where {|row| $row.item =~ $"impl Actor for ($actor)"}
        | first 1
        | get 0?
    )

    if $impl_line == null {
        return {
            ok: false,
            data: null,
            error: $"'($actor)' does not implement Actor in ($file)"
        }
    }

    let impl_start = $impl_line.index

    # Extract associated types from impl block (next 30 lines)
    let impl_body = ($lines | skip $impl_start | first 30)

    let msg_type = (
        $impl_body
        | where ($it =~ "type Msg =")
        | first 1
        | get 0?
        | default ""
        | str replace --regex ".*type Msg = " ""
        | str replace ";" ""
        | str trim
    )

    let state_type = (
        $impl_body
        | where ($it =~ "type State =")
        | first 1
        | get 0?
        | default ""
        | str replace --regex ".*type State = " ""
        | str replace ";" ""
        | str trim
    )

    let args_type = (
        $impl_body
        | where ($it =~ "type Arguments =")
        | first 1
        | get 0?
        | default ""
        | str replace --regex ".*type Arguments = " ""
        | str replace ";" ""
        | str trim
    )

    # Extract Msg enum variants
    let msg_enum = if ($msg_type | str length) > 0 {
        let enum_start = (
            $lines
            | enumerate
            | where {|row| $row.item =~ $"pub enum ($msg_type)"}
            | first 1
            | get 0?
        )

        if $enum_start != null {
            let enum_body = (
                $lines
                | skip $enum_start.index
                | first 40
            )

            let variants = (
                $enum_body
                | where {|l| $l =~ "^\\s+[A-Z][a-zA-Z]+"}
                | where {|l| not ($l =~ "^\\s*//")}
                | each {|l|
                    let name = ($l | str replace --regex "\\s*(.*?)[\\s{(,].*" "$1" | str trim)
                    let is_rpc = ($l =~ "RpcReplyPort")
                    let reply_type = if $is_rpc {
                        $l | parse --regex "RpcReplyPort<(?P<t>[^>]+)>" | get t.0? | default ""
                    } else { "" }
                    {
                        name: $name,
                        is_rpc: $is_rpc,
                        reply_type: $reply_type,
                    }
                }
            )

            { name: $msg_type, variants: $variants }
        } else {
            { name: $msg_type, variants: [] }
        }
    } else {
        { name: "unknown", variants: [] }
    }

    # Extract Args struct fields
    let args_struct = if ($args_type | str length) > 0 {
        let struct_start = (
            $lines
            | enumerate
            | where {|row| $row.item =~ $"pub struct ($args_type)"}
            | first 1
            | get 0?
        )

        if $struct_start != null {
            let struct_body = ($lines | skip $struct_start.index | first 20)
            let fields = (
                $struct_body
                | where {|l| $l =~ "pub [a-z_]+:"}
                | each {|l|
                    let parts = ($l | str trim | split row ":")
                    {
                        name: ($parts | get 0? | default "" | str replace "pub " "" | str trim),
                        type: ($parts | get 1? | default "" | str trim | str replace --regex ",$" ""),
                        visibility: "pub",
                    }
                }
            )
            let has_private = ($struct_body | any {|l| $l =~ "^    [a-z_]+:" and not ($l =~ "^    pub")})
            { name: $args_type, fields: $fields, has_private_fields: $has_private }
        } else {
            { name: $args_type, fields: [], has_private_fields: false }
        }
    } else {
        { name: "unknown", fields: [], has_private_fields: false }
    }

    # Find output ports
    let output_ports = (
        $lines
        | where ($it =~ "OutputPort<")
        | each {|l|
            let field = ($l | str replace --regex ".*pub ([a-z_]+):.*" "$1" | str trim)
            let event_type = ($l | parse --regex "OutputPort<(?P<t>[^>]+)>" | get t.0? | default "")
            { field_name: $field, event_type: $event_type }
        }
    )

    # Extract pre_start and handle signatures
    let pre_start_sig = (
        $lines
        | where ($it =~ "async fn pre_start")
        | first 1
        | get 0?
        | default ""
        | str trim
    )

    let handle_sig = (
        $lines
        | where ($it =~ "async fn handle")
        | first 1
        | get 0?
        | default ""
        | str trim
    )

    {
        ok: true,
        data: {
            actor_name: $actor,
            file: $file,
            impl_line: ($impl_start + 1),
            msg_enum: $msg_enum,
            args_struct: $args_struct,
            state_struct: {
                name: $state_type,
                is_private: true,
            },
            output_ports: $output_ports,
            pre_start_signature: $pre_start_sig,
            handle_signature: $handle_sig,
        },
        error: null
    }
}

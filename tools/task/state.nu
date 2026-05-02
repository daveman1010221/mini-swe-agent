#!/usr/bin/env nu
# task/state.nu
#
# The first call in every OBSERVE phase.
# Returns the complete current task state including playbook position.
# Never proceed without calling this first.
#
# Usage:
#   nu tools/task/state.nu

def main [] {
    let base = if ("MSWEA_RPC_BASE" in $env) { $env.MSWEA_RPC_BASE } else { "https://127.0.0.1:8000" }
    let ca   = if ("MSWEA_CA_CERT" in $env)   { $env.MSWEA_CA_CERT }   else { "" }
    let cert = if ("MSWEA_CLIENT_CERT" in $env){ $env.MSWEA_CLIENT_CERT } else { "" }
    let key  = if ("MSWEA_CLIENT_KEY" in $env) { $env.MSWEA_CLIENT_KEY }  else { "" }

    let result = (
        try {
            http post $"($base)/task/state" {}
                --ssl-cert $cert
                --ssl-key $key  
                --ssl-ca $ca
        } catch {|err|
            return { ok: false, data: null, error: $"TaskActor RPC failed: ($err.msg)" }
        }
    )

    $result
}

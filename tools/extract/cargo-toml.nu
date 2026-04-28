#!/usr/bin/env nu
# extract/cargo-toml.nu
#
# Extracts structured data from a crate's Cargo.toml.
# Use before modifying dependencies or adding test targets.
# Always call this before create/dev-dep or create/cargo-test-entry.
#
# Usage:
#   nu tools/extract/cargo-toml.nu --crate-path /workspace/crates/core

def main [
    --crate-path: path
] {
    if ($crate_path | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate-path" }
    }

    let toml_path = ($crate_path | path join "Cargo.toml")

    if not ($toml_path | path exists) {
        return { ok: false, data: null, error: $"Cargo.toml not found at: ($toml_path)" }
    }

    let toml = (open $toml_path)

    let name    = ($toml | get package.name?    | default "")
    let version = ($toml | get package.version? | default "")
    let edition = ($toml | get package.edition? | default "")

    let lib = {
        exists: (($toml | get lib? | default null) != null),
        path: ($toml | get lib.path? | default "src/lib.rs"),
    }

    let bins = ($toml | get bin? | default [] | each {|b| {
        name: ($b | get name? | default ""),
        path: ($b | get path? | default ""),
    }})

    let tests = ($toml | get test? | default [] | each {|t| {
        name: ($t | get name? | default ""),
        path: ($t | get path? | default ""),
    }})

    let deps     = ($toml | get dependencies?     | default {})
    let dev_deps = ($toml | get "dev-dependencies"? | default {})
    let features = ($toml | get features?         | default {})

    {
        ok: true,
        data: {
            name: $name,
            version: $version,
            edition: $edition,
            lib: $lib,
            bins: $bins,
            tests: $tests,
            deps: $deps,
            dev_deps: $dev_deps,
            features: $features,
        },
        error: null
    }
}

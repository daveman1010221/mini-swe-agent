#!/usr/bin/env nu
# compile/check.nu — run cargo check via mswea plugin

def main [
    --workspace-root: path,
    --crate: string,
    --tests,
] {
    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }
    if ($crate | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate" }
    }

    let result = if $tests {
        mswea cargo check --workspace-root $workspace_root --crate $crate --tests
    } else {
        mswea cargo check --workspace-root $workspace_root --crate $crate
    }

    $result
}

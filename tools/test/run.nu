#!/usr/bin/env nu
# test/run.nu — run cargo test via mswea plugin

def main [
    --workspace-root: path,
    --crate: string,
    --target: string = "all",
    --filter: string = ""
] {
    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }
    if ($crate | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate" }
    }

    mswea cargo test --workspace-root $workspace_root --crate $crate --target $target --filter $filter
}

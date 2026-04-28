#!/usr/bin/env nu
# lint/check.nu
#
# Runs clippy on a crate and returns structured warning data.
# Agent never reads raw clippy output — always goes through this tool.
# clean must be true before advancing the finalize step.
#
# Usage:
#   nu tools/lint/check.nu --workspace-root /workspace --crate mswea-core
#   nu tools/lint/check.nu --workspace-root /workspace --crate mswea-core --deny-warnings false

def main [
    --workspace-root: path,
    --crate: string,
    --deny-warnings: bool = true   # default: true (matches CI behavior)
] {
    if ($workspace_root | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --workspace-root" }
    }

    if ($crate | is-empty) {
        return { ok: false, data: null, error: "missing required flag: --crate" }
    }

    if not ($workspace_root | path exists) {
        return { ok: false, data: null, error: $"workspace-root not found: ($workspace_root)" }
    }

    let deny_arg = if $deny_warnings { ["--" "-D" "warnings"] } else { [] }

    let result = (
        try {
            do {
                cd $workspace_root
                cargo clippy --package $crate --message-format json 2>&1
            } | complete
        } catch {|err|
            return { ok: false, data: null, error: $"failed to run clippy: ($err.msg)" }
        }
    )

    # Parse JSON messages
    let messages = (
        $result.stdout
        | lines
        | where ($it | str length) > 0
        | each {|line| try { $line | from json } catch { null }}
        | where ($it != null)
        | where {|m| ($m | get reason? | default "") == "compiler-message"}
    )

    let warnings = (
        $messages
        | where {|m|
            let level = ($m | get message.level? | default "")
            $level == "warning" and
            ($m | get message.code? | default null) != null
        }
        | each {|m|
            let msg = ($m | get message)
            let spans = ($msg | get spans? | default [])
            let primary = ($spans | where {|s| $s.is_primary? | default false} | first 1)

            {
                file: (if ($primary | length) > 0 { $primary | get 0.file_name? | default "" } else { "" }),
                line: (if ($primary | length) > 0 { $primary | get 0.line_start? | default 0 } else { 0 }),
                lint: ($msg | get code.code? | default ""),
                message: ($msg | get message? | default ""),
                suggestion: ($msg | get children? | default []
                    | where {|c| ($c.level? | default "") == "help"}
                    | each {|c| $c.message? | default ""}
                    | str join " "),
                machine_applicable: ($msg | get children? | default []
                    | any {|c| ($c.spans? | default [] | any {|s| ($s.suggestion_applicability? | default "") == "MachineApplicable"})}),
            }
        }
    )

    let clean = ($warnings | is-empty) and $result.exit_code == 0

    {
        ok: true,
        data: {
            crate: $crate,
            clean: $clean,
            warning_count: ($warnings | length),
            warnings: $warnings,
            message: (if $clean { "✓ No clippy warnings" } else { $"✗ ($warnings | length) warning(s)" }),
        },
        error: null
    }
}

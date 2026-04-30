#!/usr/bin/env nu
# compile/check.nu
#
# Runs cargo check on a crate and returns structured error/warning data.
# The agent never runs raw cargo check — always goes through this tool.
# Errors are structured records, not raw compiler output.
#
# Usage:
#   nu tools/compile/check.nu --workspace-root /workspace --crate mswea-core
#   nu tools/compile/check.nu --workspace-root /workspace --crate mswea-core --tests

def main [
    --workspace-root: path,  # path to cargo workspace root
    --crate: string,         # crate name e.g. "cassini-broker"
    --tests,                 # also check test binaries (cargo check --tests)
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

    # Run cargo check with JSON message format for structured output
    let result = (
        try {
            cargo check --package $crate ...(if $tests { ["--tests"] } else { [] }) --message-format json | complete
        } catch {|err|
            return { ok: false, data: null, error: $"failed to run cargo check: ($err.msg)" }
        }
    )

    # cargo itself failed (e.g. unknown package) — surface the error
    if $result.exit_code == 101 and ($result.stdout | str length) == 0 {
        return { ok: false, data: null, error: $"cargo check failed: ($result.stderr | str trim)" }
    }

    let output_lines = ($result.stdout | lines | where ($it | str length) > 0)

    # Parse JSON messages from cargo
    let messages = (
        $output_lines
        | each {|line|
            try { $line | from json } catch { null }
        }
        | where ($it != null)
        | where {|m| ($m | get reason? | default "") == "compiler-message"}
    )

    # Extract errors
    let errors = (
        $messages
        | where {|m| ($m | get message.level? | default "") == "error"}
        | each {|m|
            let msg = ($m | get message)
            let spans = ($msg | get spans? | default [])
            let primary_span = ($spans | where {|s| $s.is_primary? | default false} | first 1)

            {
                file: (if ($primary_span | length) > 0 { $primary_span | get 0.file_name? | default "" } else { "" }),
                line: (if ($primary_span | length) > 0 { $primary_span | get 0.line_start? | default 0 } else { 0 }),
                col: (if ($primary_span | length) > 0 { $primary_span | get 0.column_start? | default 0 } else { 0 }),
                code: ($msg | get code.code? | default ""),
                message: ($msg | get message? | default ""),
                hint: ($msg | get children? | default [] | where {|c| ($c.level? | default "") == "help"} | each {|c| $c.message? | default ""} | str join " "),
                context: ($spans | each {|s| $s.text? | default [] | each {|t| $t.text? | default ""} | str join " "} | str join "\n"),
            }
        }
    )

    # Extract warnings
    let warnings = (
        $messages
        | where {|m| ($m | get message.level? | default "") == "warning"}
        | each {|m|
            let msg = ($m | get message)
            let spans = ($msg | get spans? | default [])
            let primary_span = ($spans | where {|s| $s.is_primary? | default false} | first 1)

            {
                file: (if ($primary_span | length) > 0 { $primary_span | get 0.file_name? | default "" } else { "" }),
                line: (if ($primary_span | length) > 0 { $primary_span | get 0.line_start? | default 0 } else { 0 }),
                message: ($msg | get message? | default ""),
                lint: ($msg | get code.code? | default ""),
            }
        }
    )

    let clean = ($result.exit_code == 0) and ($errors | is-empty)

    {
        ok: true,
        data: {
            crate: $crate,
            clean: $clean,
            exit_code: $result.exit_code,
            error_count: ($errors | length),
            warning_count: ($warnings | length),
            errors: $errors,
            warnings: $warnings,
        },
        error: null
    }
}

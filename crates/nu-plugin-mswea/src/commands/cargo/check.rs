//! `mswea cargo check` — run cargo check with structured diagnostic output.
//!
//! Replaces raw `cargo check` invocations. Captures stdout+stderr together,
//! parses --message-format json output, and returns structured diagnostics
//! the agent can reason about directly — file, line, column, message, hint.
//!
//! Every invocation is logged to the trajectory via TaskActor and checked
//! against the constraint policy. This is the only approved path to cargo check.
//!
//! Returns:
//!   { ok: bool, data: { crate, clean, exit_code, error_count, warning_count,
//!     errors: [{file, line, col, code, message, hint}],
//!     warnings: [{file, line, message, lint}] } }

use std::process::Command;

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};

use crate::plugin::MsweaPlugin;

pub struct MsweaCargoCheck;

impl SimplePluginCommand for MsweaCargoCheck {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea cargo check" }

    fn description(&self) -> &str {
        "Run cargo check and return structured diagnostics."
    }

    fn signature(&self) -> Signature {
        Signature::build("mswea cargo check")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("workspace-root", SyntaxShape::String,
                "Cargo workspace root path", None)
            .named("crate", SyntaxShape::String,
                "Crate name to check", None)
            .switch("tests", "Include test targets (--tests)", None)
            .category(Category::Custom("mswea".into()))
    }

    fn run(
        &self,
        _plugin: &MsweaPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let span = call.head;

        let workspace_root: String = call.get_flag("workspace-root")?
            .ok_or_else(|| LabeledError::new("Missing required flag")
                .with_label("--workspace-root is required", span))?;

        let crate_name: String = call.get_flag("crate")?
            .ok_or_else(|| LabeledError::new("Missing required flag")
                .with_label("--crate is required", span))?;

        let tests = call.has_flag("tests")?;

        // Build cargo check command
        let mut cmd = Command::new("cargo");
        cmd.arg("check")
            .arg("--package").arg(&crate_name)
            .arg("--message-format").arg("json");

        if tests {
            cmd.arg("--tests");
        }

        cmd.current_dir(&workspace_root);

        // Capture both stdout and stderr
        let output = cmd.output().map_err(|e| {
            LabeledError::new("Failed to run cargo check")
                .with_label(format!("cargo invocation failed: {e}"), span)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        // If cargo itself failed with no JSON output, surface the real error
        if exit_code == 101 && stdout.trim().is_empty() {
            return Ok(Value::record(record! {
                "ok"    => Value::bool(false, span),
                "data"  => Value::nothing(span),
                "error" => Value::string(
                    format!("cargo check failed: {}", stderr.trim()),
                    span
                ),
            }, span));
        }

        // Parse JSON diagnostic messages
        let mut errors: Vec<Value> = Vec::new();
        let mut warnings: Vec<Value> = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };

            if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
                continue;
            }

            let Some(message) = msg.get("message") else { continue; };
            let level = message.get("level").and_then(|l| l.as_str()).unwrap_or("");
            let text = message.get("message").and_then(|m| m.as_str()).unwrap_or("");

            let spans = message.get("spans")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();

            let primary = spans.iter()
                .find(|s| s.get("is_primary").and_then(|p| p.as_bool()).unwrap_or(false));

            let file = primary.and_then(|s| s.get("file_name"))
                .and_then(|f| f.as_str()).unwrap_or("").to_string();
            let line_num = primary.and_then(|s| s.get("line_start"))
                .and_then(|l| l.as_i64()).unwrap_or(0);
            let col = primary.and_then(|s| s.get("column_start"))
                .and_then(|c| c.as_i64()).unwrap_or(0);

            let code = message.get("code").and_then(|c| c.get("code"))
                .and_then(|c| c.as_str()).unwrap_or("").to_string();

            let hint = message.get("children")
                .and_then(|c| c.as_array())
                .map(|children| {
                    children.iter()
                        .filter(|c| c.get("level")
                            .and_then(|l| l.as_str()) == Some("help"))
                        .filter_map(|c| c.get("message").and_then(|m| m.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();

            let diag = Value::record(record! {
                "file"    => Value::string(file, span),
                "line"    => Value::int(line_num, span),
                "col"     => Value::int(col, span),
                "code"    => Value::string(code, span),
                "message" => Value::string(text.to_string(), span),
                "hint"    => Value::string(hint, span),
            }, span);

            match level {
                "error" => errors.push(diag),
                "warning" => warnings.push(diag),
                _ => {}
            }
        }

        let clean = exit_code == 0 && errors.is_empty();

        Ok(Value::record(record! {
            "ok"   => Value::bool(true, span),
            "data" => Value::record(record! {
                "crate"         => Value::string(crate_name, span),
                "clean"         => Value::bool(clean, span),
                "exit_code"     => Value::int(exit_code as i64, span),
                "error_count"   => Value::int(errors.len() as i64, span),
                "warning_count" => Value::int(warnings.len() as i64, span),
                "errors"        => Value::list(errors, span),
                "warnings"      => Value::list(warnings, span),
            }, span),
            "error" => Value::nothing(span),
        }, span))
    }
}

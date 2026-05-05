//! `mswea cargo test` — run cargo test with structured result output.
//!
//! Captures stdout+stderr together, parses test result lines, and returns
//! structured pass/fail data including failure details with panic messages.
//!
//! Every invocation goes through the policy layer. This is the only
//! approved path to cargo test.

use std::process::Command;

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};

use crate::plugin::MsweaPlugin;

pub struct MsweaCargoTest;

impl SimplePluginCommand for MsweaCargoTest {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea cargo test" }

    fn description(&self) -> &str {
        "Run cargo test and return structured results."
    }

    fn signature(&self) -> Signature {
        Signature::build("mswea cargo test")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("workspace-root", SyntaxShape::String,
                "Cargo workspace root path", None)
            .named("crate", SyntaxShape::String,
                "Crate name to test", None)
            .named("target", SyntaxShape::String,
                "Test target: unit | props | integration | all", None)
            .named("filter", SyntaxShape::String,
                "Optional test name filter", None)
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

        let target: String = call.get_flag("target")?
            .unwrap_or_else(|| "all".to_string());

        let filter: String = call.get_flag("filter")?
            .unwrap_or_default();

        // Build cargo test command
        let mut cmd = Command::new("cargo");
        cmd.arg("test")
            .arg("--package").arg(&crate_name)
            .arg("--no-fail-fast");

        if target != "all" {
            cmd.arg("--test").arg(&target);
        }

        cmd.arg("--").arg("--nocapture");

        if !filter.is_empty() {
            cmd.arg(&filter);
        }

        cmd.current_dir(&workspace_root);

        // Capture stdout+stderr together
        let output = cmd.output().map_err(|e| {
            LabeledError::new("Failed to run cargo test")
                .with_label(format!("cargo invocation failed: {e}"), span)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!("{stdout}\n{stderr}");
        let exit_code = output.status.code().unwrap_or(-1);
        let lines: Vec<&str> = combined.lines().collect();

        // Find the test result summary line
        let result_line = lines.iter()
            .filter(|l| l.contains("test result:"))
            .last()
            .copied()
            .unwrap_or("");

        let passed = extract_count(result_line, "passed");
        let failed = extract_count(result_line, "failed");
        let ignored = extract_count(result_line, "ignored");
        let total = passed + failed + ignored;
        let success = failed == 0 && exit_code == 0;

        // Extract failure details
        let failures: Vec<Value> = if failed > 0 {
            lines.iter()
                .filter(|l| l.starts_with("FAILED "))
                .map(|l| {
                    let name = l.trim_start_matches("FAILED ").trim().to_string();
                    // Find panic message near this test name
                    let panic_msg = lines.windows(20)
                        .find(|w| w.iter().any(|l| l.contains(&name)))
                        .and_then(|w| w.iter()
                            .find(|l| l.contains("panicked at") || l.contains("thread '") && l.contains("panicked"))
                            .map(|l| l.to_string()))
                        .unwrap_or_default();

                    Value::record(record! {
                        "name"          => Value::string(name, span),
                        "panic_message" => Value::string(panic_msg, span),
                    }, span)
                })
                .collect()
        } else {
            vec![]
        };

        Ok(Value::record(record! {
            "ok"   => Value::bool(true, span),
            "data" => Value::record(record! {
                "crate"          => Value::string(crate_name, span),
                "target"         => Value::string(target, span),
                "passed"         => Value::int(passed as i64, span),
                "failed"         => Value::int(failed as i64, span),
                "ignored"        => Value::int(ignored as i64, span),
                "total"          => Value::int(total as i64, span),
                "success"        => Value::bool(success, span),
                "failures"       => Value::list(failures, span),
                "exit_code"      => Value::int(exit_code as i64, span),
            }, span),
            "error" => Value::nothing(span),
        }, span))
    }
}

fn extract_count(line: &str, label: &str) -> u32 {
    line.split_whitespace()
        .zip(line.split_whitespace().skip(1))
        .find(|(_, b)| b.trim_end_matches(';').trim_end_matches('.') == label)
        .and_then(|(a, _)| a.parse().ok())
        .unwrap_or(0)
}

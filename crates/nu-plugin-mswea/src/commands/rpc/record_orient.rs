//! `mswea rpc record-orient` — record an orient step observation.
//!
//! Replaces:
//!   http post $"($base)/task/record-orient" ($body | to json)
//!
//! With:
//!   mswea rpc record-orient {
//!       observed: $observed
//!       decision: $decision
//!       blockers: $blockers
//!   }
//!
//! The record fields map directly to the orient report stored in TaskActor.
//! Returns { ok: bool, recorded: bool, step: string, budget_remaining: int, error: string? }

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcRecordOrient;

impl SimplePluginCommand for MsweaRpcRecordOrient {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str {
        "mswea rpc record-orient"
    }

    fn description(&self) -> &str {
        "Record an orient step observation and decision to the task actor."
    }

    fn extra_description(&self) -> &str {
        r#"Sends the orient report to TaskActor via the ractor cluster.
The report is recorded against the current playbook step and the
step budget is decremented.

The input record must contain:
  observed  — what the agent observed during this orient cycle
  decision  — what the agent decided to do next
  blockers  — (optional) list of blocking issues identified"#
    }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc record-orient")
            .input_output_type(Type::Record(vec![].into()), Type::Record(vec![].into()))
            .required(
                "report",
                SyntaxShape::Record(vec![]),
                "Orient report record with observed, decision, and optional blockers fields.",
            )
            .category(Category::Custom("mswea".into()))
    }

    fn run(
        &self,
        _plugin: &MsweaPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let report: Value = call.req(0)?;
        let span = call.head;

        // Extract fields from the record
        let observed = report
            .get_data_by_key("observed")
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .ok_or_else(|| {
                LabeledError::new("Missing required field")
                    .with_label("record must contain 'observed' string field", span)
            })?;

        let decision = report
            .get_data_by_key("decision")
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .ok_or_else(|| {
                LabeledError::new("Missing required field")
                    .with_label("record must contain 'decision' string field", span)
            })?;

        let blockers = report
            .get_data_by_key("blockers")
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .unwrap_or_default();

        // TODO: send RecordOrient message to TaskActor via ractor cluster ActorRef
        // For now, return a stub response so the plugin compiles and tools can be
        // written against the correct return shape.
        let _ = (observed, decision, blockers);

        Ok(Value::record(
            record! {
                "ok" => Value::bool(false, span),
                "recorded" => Value::bool(false, span),
                "step" => Value::string("", span),
                "budget_remaining" => Value::int(0, span),
                "error" => Value::string("ractor cluster not yet connected", span),
            },
            span,
        ))
    }
}

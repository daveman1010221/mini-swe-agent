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

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use tokio::time::Duration;

use mswea_core::task::TaskMsg;
use mswea_core::task::{RecordOrientRequest, RecordOrientResponse};

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcRecordOrient;

impl SimplePluginCommand for MsweaRpcRecordOrient {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea rpc record-orient" }

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
  blockers  — (optional) blocking issues identified"#
    }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc record-orient")
            .input_output_type(Type::Record(vec![].into()), Type::Record(vec![].into()))
            .required(
                "report",
                SyntaxShape::Record(vec![]),
                "Orient report with observed, decision, and optional blockers fields.",
            )
            .category(Category::Custom("mswea".into()))
    }

    fn run(
        &self,
        plugin: &MsweaPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let span = call.head;
        let report: Value = call.req(0)?;

        let observed = report
            .get_data_by_key("observed")
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .ok_or_else(|| LabeledError::new("Missing required field")
                .with_label("record must contain 'observed' string field", span))?;

        let decision = report
            .get_data_by_key("decision")
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .ok_or_else(|| LabeledError::new("Missing required field")
                .with_label("record must contain 'decision' string field", span))?;

        let blockers = report
            .get_data_by_key("blockers")
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .filter(|s| !s.is_empty());

        let task_actor = plugin.task_actor.as_ref().ok_or_else(|| {
            LabeledError::new("Plugin not connected")
                .with_label("task-actor ActorRef not resolved — is the mswea runtime running?", span)
        })?;

        let req = RecordOrientRequest { observed, decision, blockers };

        // Bridge sync plugin command into async actor call via stored runtime handle
        let response: RecordOrientResponse = match plugin.rt.block_on(async {
            task_actor.call(
                |reply| TaskMsg::RecordOrient { req, reply },
                Some(Duration::from_secs(5)),
            )
            .await
        }) {
            Err(e) => return Err(LabeledError::new("Actor messaging error")
                .with_label(format!("Failed to send to task-actor: {e}"), span)),
            Ok(ractor::rpc::CallResult::Success(v)) => v,
            Ok(ractor::rpc::CallResult::Timeout) => return Err(LabeledError::new("Actor call timed out")
                .with_label("task-actor did not reply within 5 seconds", span)),
            Ok(ractor::rpc::CallResult::SenderError) => return Err(LabeledError::new("Actor send error")
                .with_label("task-actor reply channel closed", span)),
        };

        Ok(Value::record(record! {
            "ok"               => Value::bool(response.ok, span),
            "recorded"         => Value::bool(response.recorded, span),
            "step"             => Value::string(response.step, span),
            "budget_remaining" => Value::int(response.budget_remaining as i64, span),
            "error"            => match response.error {
                Some(e) => Value::string(e, span),
                None    => Value::nothing(span),
            },
        }, span))
    }
}

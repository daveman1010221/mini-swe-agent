use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use tokio::time::Duration;

use actors::task_actor::TaskMsg;
use mswea_core::task::{RecordAttemptRequest, RecordAttemptResponse};

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcRecordAttempt;

impl SimplePluginCommand for MsweaRpcRecordAttempt {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea rpc record-attempt" }
    fn description(&self) -> &str { "Record a step attempt against the current task budget." }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc record-attempt")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("action", SyntaxShape::String, "Action taken", None)
            .named("result", SyntaxShape::String, "Result of the action", None)
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
        let action: String = call.get_flag("action")?.unwrap_or_default();
        let result: String = call.get_flag("result")?.unwrap_or_default();

        let task_actor = plugin.task_actor.as_ref().ok_or_else(|| {
            LabeledError::new("Plugin not connected")
                .with_label("task-actor ActorRef not resolved", span)
        })?;

        let req = RecordAttemptRequest { action, result };

        let response: RecordAttemptResponse = match plugin.rt.block_on(async {
            task_actor.call(
                |reply| TaskMsg::RecordAttempt { req, reply },
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
            "step_attempts"    => Value::int(response.step_attempts as i64, span),
            "budget_remaining" => Value::int(response.budget_remaining as i64, span),
            "budget_exhausted" => Value::bool(response.budget_exhausted, span),
            "error"            => opt_string(response.error, span),
        }, span))
    }
}

fn opt_string(v: Option<String>, span: nu_protocol::Span) -> Value {
    match v { Some(s) => Value::string(s, span), None => Value::nothing(span) }
}

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use tokio::time::Duration;

use mswea_core::task::TaskMsg;
use mswea_core::task::{AdvanceRequest, AdvanceResponse};

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcAdvance;

impl SimplePluginCommand for MsweaRpcAdvance {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea rpc advance" }
    fn description(&self) -> &str { "Advance to the next playbook step." }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc advance")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("verification", SyntaxShape::String,
                "Verification note confirming the current step is complete", None)
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
        let verification: String = call.get_flag("verification")?
            .unwrap_or_default();

        let task_actor = plugin.task_actor.as_ref().ok_or_else(|| {
            LabeledError::new("Plugin not connected")
                .with_label("task-actor ActorRef not resolved", span)
        })?;

        let req = AdvanceRequest { verification };

        let response: AdvanceResponse = match plugin.rt.block_on(async {
            task_actor.call(
                |reply| TaskMsg::Advance { req, reply },
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
            "ok"             => Value::bool(response.ok, span),
            "advanced"       => Value::bool(response.advanced, span),
            "previous_step"  => opt_string(response.previous_step, span),
            "current_step"   => opt_string(response.current_step, span),
            "task_completed" => Value::bool(response.task_completed, span),
            "error"          => opt_string(response.error, span),
        }, span))
    }
}

fn opt_string(v: Option<String>, span: nu_protocol::Span) -> Value {
    match v { Some(s) => Value::string(s, span), None => Value::nothing(span) }
}

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use tokio::time::Duration;

use mswea_core::task::TaskMsg;
use mswea_core::task::{HaltRequest, HaltResponse};

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcHalt;

impl SimplePluginCommand for MsweaRpcHalt {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea rpc halt" }
    fn description(&self) -> &str { "Halt the current task with a reason." }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc halt")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("reason", SyntaxShape::String, "Reason for halting", None)
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
        let reason: String = call.get_flag("reason")?
            .unwrap_or_else(|| "no reason provided".to_string());

        let task_actor = plugin.task_actor.as_ref().ok_or_else(|| {
            LabeledError::new("Plugin not connected")
                .with_label("task-actor ActorRef not resolved", span)
        })?;

        let req = HaltRequest { reason };

        let response: HaltResponse = match plugin.rt.block_on(async {
            task_actor.call(
                |reply| TaskMsg::Halt { req, reply },
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
            "ok"     => Value::bool(response.ok, span),
            "halted" => Value::bool(response.halted, span),
            "error"  => opt_string(response.error, span),
        }, span))
    }
}

fn opt_string(v: Option<String>, span: nu_protocol::Span) -> Value {
    match v { Some(s) => Value::string(s, span), None => Value::nothing(span) }
}

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, Type, Value, record};
use tokio::time::Duration;

use mswea_core::task::TaskMsg;
use mswea_core::task::TaskStateResponse;

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcTaskState;

impl SimplePluginCommand for MsweaRpcTaskState {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea rpc task-state" }
    fn description(&self) -> &str { "Query the current task state from the task actor." }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc task-state")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
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

        let task_actor = plugin.task_actor.as_ref().ok_or_else(|| {
            LabeledError::new("Plugin not connected")
                .with_label("task-actor ActorRef not resolved", span)
        })?;

        let response: TaskStateResponse = match plugin.rt.block_on(async {
            task_actor.call(
                |reply| TaskMsg::GetState { reply },
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

        let data_value = match response.data {
            None => Value::nothing(span),
            Some(d) => Value::record(record! {
                "has_task"        => Value::bool(d.has_task, span),
                "crate_name"      => opt_string(d.crate_name, span),
                "crate_path"      => opt_string(d.crate_path, span),
                "op"              => opt_string(d.op, span),
                "step"            => opt_string(d.step, span),
                "step_index"      => opt_int(d.step_index.map(|v| v as i64), span),
                "step_attempts"   => opt_int(d.step_attempts.map(|v| v as i64), span),
                "step_budget"     => opt_int(d.step_budget.map(|v| v as i64), span),
                "budget_remaining" => opt_int(d.budget_remaining.map(|v| v as i64), span),
                "budget_exhausted" => opt_bool(d.budget_exhausted, span),
                "pending_count"   => Value::int(d.pending_count as i64, span),
                "completed_count" => Value::int(d.completed_count as i64, span),
                "halted_count"    => Value::int(d.halted_count as i64, span),
            }, span),
        };

        Ok(Value::record(record! {
            "ok"    => Value::bool(response.ok, span),
            "data"  => data_value,
            "error" => opt_string(response.error, span),
        }, span))
    }
}

fn opt_string(v: Option<String>, span: nu_protocol::Span) -> Value {
    match v {
        Some(s) => Value::string(s, span),
        None => Value::nothing(span),
    }
}

fn opt_int(v: Option<i64>, span: nu_protocol::Span) -> Value {
    match v {
        Some(i) => Value::int(i, span),
        None => Value::nothing(span),
    }
}

fn opt_bool(v: Option<bool>, span: nu_protocol::Span) -> Value {
    match v {
        Some(b) => Value::bool(b, span),
        None => Value::nothing(span),
    }
}

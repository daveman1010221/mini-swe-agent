use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, Type, Value, record};
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
    fn run(&self, _plugin: &MsweaPlugin, _engine: &EngineInterface, call: &EvaluatedCall, _input: &Value) -> Result<Value, LabeledError> {
        let span = call.head;
        Ok(Value::record(record! { "ok" => Value::bool(false, span), "error" => Value::string("not yet implemented", span) }, span))
    }
}

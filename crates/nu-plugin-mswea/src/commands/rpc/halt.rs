use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
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
    fn run(&self, _plugin: &MsweaPlugin, _engine: &EngineInterface, call: &EvaluatedCall, _input: &Value) -> Result<Value, LabeledError> {
        let span = call.head;
        Ok(Value::record(record! { "ok" => Value::bool(false, span), "error" => Value::string("not yet implemented", span) }, span))
    }
}

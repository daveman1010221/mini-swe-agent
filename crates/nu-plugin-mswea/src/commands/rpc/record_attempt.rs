use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
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
    fn run(&self, _plugin: &MsweaPlugin, _engine: &EngineInterface, call: &EvaluatedCall, _input: &Value) -> Result<Value, LabeledError> {
        let span = call.head;
        Ok(Value::record(record! { "ok" => Value::bool(false, span), "error" => Value::string("not yet implemented", span) }, span))
    }
}

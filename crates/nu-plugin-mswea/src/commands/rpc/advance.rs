use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use crate::plugin::MsweaPlugin;

pub struct MsweaRpcAdvance;

impl SimplePluginCommand for MsweaRpcAdvance {
    type Plugin = MsweaPlugin;
    fn name(&self) -> &str { "mswea rpc advance" }
    fn description(&self) -> &str { "Advance to the next playbook step." }
    fn signature(&self) -> Signature {
        Signature::build("mswea rpc advance")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("verification", SyntaxShape::String, "Verification note for the completed step", None)
            .category(Category::Custom("mswea".into()))
    }
    fn run(&self, _plugin: &MsweaPlugin, _engine: &EngineInterface, call: &EvaluatedCall, _input: &Value) -> Result<Value, LabeledError> {
        let span = call.head;
        Ok(Value::record(record! { "ok" => Value::bool(false, span), "error" => Value::string("not yet implemented", span) }, span))
    }
}

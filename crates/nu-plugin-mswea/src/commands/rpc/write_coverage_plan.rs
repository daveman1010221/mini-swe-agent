use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use crate::plugin::MsweaPlugin;

pub struct MsweaRpcWriteCoveragePlan;

impl SimplePluginCommand for MsweaRpcWriteCoveragePlan {
    type Plugin = MsweaPlugin;
    fn name(&self) -> &str { "mswea rpc write-coverage-plan" }
    fn description(&self) -> &str { "Record the test coverage plan for the current task." }
    fn signature(&self) -> Signature {
        Signature::build("mswea rpc write-coverage-plan")
            .input_output_type(Type::Record(vec![].into()), Type::Record(vec![].into()))
            .required("plan", SyntaxShape::Record(vec![]), "Coverage plan record", )
            .category(Category::Custom("mswea".into()))
    }
    fn run(&self, _plugin: &MsweaPlugin, _engine: &EngineInterface, call: &EvaluatedCall, _input: &Value) -> Result<Value, LabeledError> {
        let span = call.head;
        Ok(Value::record(record! { "ok" => Value::bool(false, span), "error" => Value::string("not yet implemented", span) }, span))
    }
}

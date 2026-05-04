//! `mswea cargo test` — run cargo test, return structured results.

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};

use crate::plugin::MsweaPlugin;

pub struct MsweaCargoTest;

impl SimplePluginCommand for MsweaCargoTest {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea cargo test" }

    fn description(&self) -> &str {
        "Run cargo test and return structured results."
    }

    fn signature(&self) -> Signature {
        Signature::build("mswea cargo test")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("workspace-root", SyntaxShape::String, "Workspace root path", None)
            .named("crate", SyntaxShape::String, "Crate name to test", None)
            .named("target", SyntaxShape::String, "Test target (unit, integration, all)", None)
            .category(Category::Custom("mswea".into()))
    }

    fn run(
        &self,
        _plugin: &MsweaPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let span = call.head;
        // TODO: invoke cargo test via policy-gated execution
        Ok(Value::record(record! {
            "ok" => Value::bool(false, span),
            "error" => Value::string("not yet implemented", span),
        }, span))
    }
}

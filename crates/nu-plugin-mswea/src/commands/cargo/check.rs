//! `mswea cargo check` — run cargo check, return structured diagnostics.
//!
//! Replaces raw `cargo check` invocations in compile/check.nu.
//! Captures stdout+stderr together, parses --message-format json output,
//! and returns a structured record the agent can reason about directly.
//!
//! Returns:
//!   { ok: bool, clean: bool, error_count: int, warning_count: int,
//!     errors: list<record>, warnings: list<record> }

use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};

use crate::plugin::MsweaPlugin;

pub struct MsweaCargoCheck;

impl SimplePluginCommand for MsweaCargoCheck {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea cargo check" }

    fn description(&self) -> &str {
        "Run cargo check and return structured diagnostics."
    }

    fn signature(&self) -> Signature {
        Signature::build("mswea cargo check")
            .input_output_type(Type::Nothing, Type::Record(vec![].into()))
            .named("workspace-root", SyntaxShape::String, "Workspace root path", None)
            .named("crate", SyntaxShape::String, "Crate name to check", None)
            .switch("tests", "Include test targets", None)
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
        // TODO: invoke cargo check via policy-gated execution
        Ok(Value::record(record! {
            "ok" => Value::bool(false, span),
            "error" => Value::string("not yet implemented", span),
        }, span))
    }
}

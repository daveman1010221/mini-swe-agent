use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Signature, SyntaxShape, Type, Value, record};
use tokio::time::Duration;

use mswea_core::task::TaskMsg;
use mswea_core::task::{WriteCoveragePlanRequest, WriteCoveragePlanResponse, PlannedTest};

use crate::plugin::MsweaPlugin;

pub struct MsweaRpcWriteCoveragePlan;

impl SimplePluginCommand for MsweaRpcWriteCoveragePlan {
    type Plugin = MsweaPlugin;

    fn name(&self) -> &str { "mswea rpc write-coverage-plan" }
    fn description(&self) -> &str { "Record the test coverage plan for the current task." }

    fn signature(&self) -> Signature {
        Signature::build("mswea rpc write-coverage-plan")
            .input_output_type(Type::Record(vec![].into()), Type::Record(vec![].into()))
            .required("plan", SyntaxShape::Record(vec![]),
                "Coverage plan record with planned_tests, serde_required, rkyv_required fields")
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
        let plan: Value = call.req(0)?;

        let serde_required = plan.get_data_by_key("serde_required")
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(false);

        let rkyv_required = plan.get_data_by_key("rkyv_required")
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(false);

        let existing_tests = plan.get_data_by_key("existing_tests")
            .and_then(|v| v.as_int().ok())
            .unwrap_or(0) as u32;

        // Parse planned_tests list of records
        let planned_tests: Vec<PlannedTest> = plan.get_data_by_key("planned_tests")
            .and_then(|v| v.as_list().ok().map(|l| l.to_vec()))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let name = item.get_data_by_key("name")
                    .and_then(|v| v.as_str().ok().map(|s| s.to_string()))?;
                let test_type = item.get_data_by_key("type")
                    .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
                    .unwrap_or_else(|| "unit".to_string());
                let rationale = item.get_data_by_key("rationale")
                    .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
                    .unwrap_or_default();
                Some(PlannedTest { name, test_type, rationale })
            })
            .collect();

        let public_interfaces: Vec<String> = plan.get_data_by_key("public_interfaces")
            .and_then(|v| v.as_list().ok().map(|l| l.to_vec()))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().ok().map(|s| s.to_string()))
            .collect();

        let failure_modes: Vec<String> = plan.get_data_by_key("failure_modes")
            .and_then(|v| v.as_list().ok().map(|l| l.to_vec()))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().ok().map(|s| s.to_string()))
            .collect();

        let boundary_conditions: Vec<String> = plan.get_data_by_key("boundary_conditions")
            .and_then(|v| v.as_list().ok().map(|l| l.to_vec()))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().ok().map(|s| s.to_string()))
            .collect();

        let task_actor = plugin.task_actor.as_ref().ok_or_else(|| {
            LabeledError::new("Plugin not connected")
                .with_label("task-actor ActorRef not resolved", span)
        })?;

        let req = WriteCoveragePlanRequest {
            public_interfaces,
            failure_modes,
            boundary_conditions,
            serde_required,
            rkyv_required,
            existing_tests,
            planned_tests,
        };

        let response: WriteCoveragePlanResponse = match plugin.rt.block_on(async {
            task_actor.call(
                |reply| TaskMsg::WriteCoveragePlan { req, reply },
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
            "ok"            => Value::bool(response.ok, span),
            "plan_recorded" => Value::bool(response.plan_recorded, span),
            "planned_count" => Value::int(response.planned_count as i64, span),
            "error"         => opt_string(response.error, span),
        }, span))
    }
}

fn opt_string(v: Option<String>, span: nu_protocol::Span) -> Value {
    match v { Some(s) => Value::string(s, span), None => Value::nothing(span) }
}

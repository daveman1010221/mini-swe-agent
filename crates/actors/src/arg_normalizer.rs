//! ArgNormalizerActor — first stage of the policy pipeline.
//!
//! Receives a raw ToolCall from the model, normalizes argument types,
//! coerces common mistakes, and returns a NormalizedToolCall with
//! FeedbackNotes describing every correction made.
//!
//! Corrections applied:
//!   - Boolean flags passed as strings ("true"/"false") → coerced to bool
//!   - Flag names with underscores → converted to kebab-case
//!   - (future) numeric strings → coerced to int/float where expected
//!
//! This actor is stateless — it consults the ToolRegistry for flag type
//! information but holds no mutable state of its own.

use std::sync::Arc;
use tokio::sync::RwLock;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use mswea_core::{
    policy::{FeedbackNote, NormalizedToolCall},
    toolbox::ToolRegistry,
    ToolCall,
};

use crate::policy_messages::NormalizeRequest;

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct ArgNormalizerActor;

pub struct ArgNormalizerArgs {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
}

pub struct ArgNormalizerState {
    tool_registry: Arc<RwLock<ToolRegistry>>,
}

impl Actor for ArgNormalizerActor {
    type Msg = NormalizeRequest;
    type State = ArgNormalizerState;
    type Arguments = ArgNormalizerArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("ArgNormalizerActor starting");
        Ok(ArgNormalizerState {
            tool_registry: args.tool_registry,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        req: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let result = normalize(req.call, &state.tool_registry).await;
        let _ = req.reply.send(result);
        Ok(())
    }
}

// ── Normalization logic ───────────────────────────────────────────────────────

async fn normalize(call: ToolCall, registry: &Arc<RwLock<ToolRegistry>>) -> NormalizedToolCall {
    match &call {
        ToolCall::NushellTool { namespace, tool, args } => {
            let full_name = format!("{namespace}/{tool}");
            let flags = {
                let reg = registry.read().await;
                reg.get(&full_name).map(|e| e.flags.clone())
            };

            let Some(tool_flags) = flags else {
                // Unknown tool — pass through unchanged, ConstraintChecker will reject
                return NormalizedToolCall::unchanged(call);
            };

            let args_val: serde_json::Value = serde_json::from_str(args)
                .unwrap_or_default();

            let Some(obj) = args_val.as_object() else {
                return NormalizedToolCall::unchanged(call);
            };

            let mut feedback = Vec::new();
            let mut normalized_args = obj.clone();

            for (key, val) in obj {
                let flag_name = key.replace('_', "-");

                // Normalize underscore flag names to kebab-case
                if flag_name != *key {
                    normalized_args.remove(key);
                    normalized_args.insert(flag_name.clone(), val.clone());
                    feedback.push(FeedbackNote::info(
                        "ArgNormalizer",
                        format!(
                            "--{key} normalized to --{flag_name} (use kebab-case for flag names)"
                        ),
                    ));
                }

                // Coerce string "true"/"false" to boolean for bool flags
                if let Some(flag) = tool_flags.iter().find(|f| f.name == flag_name) {
                    if flag.flag_type == "bool" {
                        if let serde_json::Value::String(s) = val {
                            match s.as_str() {
                                "true" => {
                                    normalized_args.insert(
                                        flag_name.clone(),
                                        serde_json::Value::Bool(true),
                                    );
                                    feedback.push(FeedbackNote::info(
                                        "ArgNormalizer",
                                        format!(
                                            "--{flag_name} was string \"{s}\" — \
                                             auto-converted to boolean. \
                                             Pass boolean flags as true, not \"true\"."
                                        ),
                                    ));
                                }
                                "false" => {
                                    normalized_args.insert(
                                        flag_name.clone(),
                                        serde_json::Value::Bool(false),
                                    );
                                    feedback.push(FeedbackNote::info(
                                        "ArgNormalizer",
                                        format!(
                                            "--{flag_name} was string \"{s}\" — \
                                             auto-converted to boolean. \
                                             Pass boolean flags as false, not \"false\"."
                                        ),
                                    ));
                                }
                                _ => {
                                    // Not a valid bool string — leave for ConstraintChecker
                                }
                            }
                        }
                    }
                }
            }

            let normalized_call = ToolCall::NushellTool {
                namespace: namespace.clone(),
                tool: tool.clone(),
                args: serde_json::Value::Object(normalized_args).to_string(),
            };

            NormalizedToolCall::with_feedback(normalized_call, feedback)
        }

        // All other call types pass through unchanged
        _ => NormalizedToolCall::unchanged(call),
    }
}

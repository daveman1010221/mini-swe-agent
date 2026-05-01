//! ConstraintCheckerActor — second stage of the policy pipeline.
//!
//! Receives a NormalizedToolCall and fans out to domain policy actors
//! in parallel. Reduces their verdicts into a single PipelineResult:
//!
//!   - Any Rejected → Block with aggregated feedback
//!   - Any Modified → Execute modified call with feedback
//!   - All Approved → Execute original call with any info feedback
//!
//! Current domain checks (inline, will migrate to sub-actors):
//!   - ApprovedToolsPolicy   — is this tool allowed in the current playbook step?
//!   - ForbiddenToolsPolicy  — is this tool explicitly forbidden?
//!   - WriteConstraintPolicy — compile/check required after test write?
//!   - OneTestPerWritePolicy — only one #[test] per write to a test file?

use std::sync::Arc;
use tokio::sync::RwLock;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use mswea_core::{
    policy::{
        FeedbackNote, PipelineResult, PolicyContext, PolicyVerdict,
        LastTestWrite, LastCompileCheck
    },
    toolbox::ToolRegistry,
    ToolCall,
};
use crate::policy_messages::{ConstraintRequest, PolicyContextUpdate, ToolCallCompleted};

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct ConstraintCheckerActor;

pub struct ConstraintCheckerArgs {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
}

pub struct ConstraintCheckerState {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    context: PolicyContext,
}

pub enum ConstraintCheckerMsg {
    Check(ConstraintRequest),
    UpdateContext(PolicyContextUpdate),
    ToolCallCompleted(ToolCallCompleted),
}

impl Actor for ConstraintCheckerActor {
    type Msg = ConstraintCheckerMsg;
    type State = ConstraintCheckerState;
    type Arguments = ConstraintCheckerArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("ConstraintCheckerActor starting");
        Ok(ConstraintCheckerState {
            tool_registry: args.tool_registry,
            context: PolicyContext::initial(),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            ConstraintCheckerMsg::Check(req) => {
                let result = check(&req.normalized.call, &req.context, &req.normalized.feedback, &state.tool_registry).await;
                let _ = req.reply.send(result);
            }
            ConstraintCheckerMsg::UpdateContext(update) => {
                state.context = update.context;
                tracing::debug!(
                    step = %state.context.playbook_step,
                    "ConstraintCheckerActor: context updated"
                );
                let _ = update.reply.send(());
            }
            ConstraintCheckerMsg::ToolCallCompleted(completed) => {
                state.context.last_tool_call = Some(completed.call_summary);
                state.context.last_tool_step = Some(completed.step);

                if let Some(path) = completed.path {
                    if path.contains("/tests/") && path.ends_with(".rs") {
                        state.context.last_test_write = Some(LastTestWrite {
                            step: completed.step,
                            path,
                        });
                    }
                }

                if completed.was_compile_check {
                    state.context.last_compile_check = Some(LastCompileCheck {
                        step: completed.step,
                        clean: completed.compile_clean.unwrap_or(false),
                        error_count: 0, // will refine later
                    });
                }
            }
        }
        Ok(())
    }
}

// ── Constraint checks ─────────────────────────────────────────────────────────

async fn check(
    call: &ToolCall,
    ctx: &PolicyContext,
    prior_feedback: &[FeedbackNote],
    _registry: &Arc<RwLock<ToolRegistry>>,
) -> PipelineResult {
    let mut feedback: Vec<FeedbackNote> = prior_feedback.to_vec();
    let mut verdicts: Vec<PolicyVerdict> = Vec::new();

    // Run all constraint checks
    verdicts.push(check_approved_tools(call, ctx));
    verdicts.push(check_forbidden_tools(call, ctx));
    verdicts.push(check_one_test_per_write(call, ctx));
    verdicts.push(check_compile_after_test_write(call, ctx));

    // Reduce verdicts
    let mut rejections: Vec<FeedbackNote> = Vec::new();

    for verdict in verdicts {
        match verdict {
            PolicyVerdict::Approved => {}
            PolicyVerdict::Rejected { reason, feedback: vf } => {
                rejections.push(FeedbackNote::required("ConstraintChecker", reason));
                feedback.extend(vf);
            }
            PolicyVerdict::Modified { call: _, feedback: vf } => {
                // Modified verdicts from sub-checks not yet supported —
                // normalization handles modifications upstream
                feedback.extend(vf);
            }
        }
    }

    if !rejections.is_empty() {
        let reason = rejections.iter()
            .map(|n| n.message.clone())
            .collect::<Vec<_>>()
            .join(" | ");
        feedback.extend(rejections);
        return PipelineResult::Block { reason, feedback };
    }

    PipelineResult::Execute {
        call: call.clone(),
        feedback,
    }
}

// ── Individual constraint implementations ─────────────────────────────────────

fn check_approved_tools(call: &ToolCall, ctx: &PolicyContext) -> PolicyVerdict {
    if ctx.approved_tools.is_empty() {
        return PolicyVerdict::Approved;
    }

    // Extract tool name for nushell tools
    let tool_name = match call {
        ToolCall::NushellTool { namespace, tool, .. } => {
            format!("{namespace}/{tool}")
        }
        ToolCall::Shell { .. } => "shell".to_string(),
        ToolCall::Read { .. } => "read".to_string(),
        ToolCall::Write { .. } => "write".to_string(),
        ToolCall::Edit { .. } => "edit".to_string(),
        ToolCall::Search { .. } => "search".to_string(),
        ToolCall::Submit { .. } => return PolicyVerdict::Approved,
    };

    // Primitive tools (read, write, edit, search) are always permitted —
    // they're infrastructure, not playbook tools
    if matches!(call, ToolCall::Read { .. } | ToolCall::Write { .. }
        | ToolCall::Edit { .. } | ToolCall::Search { .. }) {
        return PolicyVerdict::Approved;
    }

    if ctx.approved_tools.iter().any(|t| t == &tool_name) {
        PolicyVerdict::Approved
    } else {
        PolicyVerdict::Rejected {
            reason: format!(
                "{tool_name} is not in the approved tools list for the \
                 '{step}' playbook step. Approved: {approved}",
                step = ctx.playbook_step,
                approved = ctx.approved_tools.join(", "),
            ),
            feedback: vec![FeedbackNote::required(
                "ApprovedToolsPolicy",
                format!(
                    "Use only approved tools for the '{step}' step. \
                     Call playbook/current-step to see what's allowed.",
                    step = ctx.playbook_step,
                ),
            )],
        }
    }
}

fn check_forbidden_tools(call: &ToolCall, ctx: &PolicyContext) -> PolicyVerdict {
    if ctx.forbidden_tools.is_empty() {
        return PolicyVerdict::Approved;
    }

    let tool_name = match call {
        ToolCall::NushellTool { namespace, tool, .. } => format!("{namespace}/{tool}"),
        _ => return PolicyVerdict::Approved,
    };

    // Check exact match and wildcard prefix (e.g. "create/*")
    let is_forbidden = ctx.forbidden_tools.iter().any(|pattern| {
        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 2];
            tool_name.starts_with(prefix)
        } else {
            pattern == &tool_name
        }
    });

    if is_forbidden {
        PolicyVerdict::Rejected {
            reason: format!(
                "{tool_name} is forbidden in the '{step}' playbook step.",
                step = ctx.playbook_step,
            ),
            feedback: vec![FeedbackNote::required(
                "ForbiddenToolsPolicy",
                format!(
                    "{tool_name} is not permitted during '{step}'. \
                     Check playbook/current-step for what's allowed.",
                    step = ctx.playbook_step,
                ),
            )],
        }
    } else {
        PolicyVerdict::Approved
    }
}

fn check_one_test_per_write(call: &ToolCall, _ctx: &PolicyContext) -> PolicyVerdict {
    let ToolCall::Write { path, content } = call else {
        return PolicyVerdict::Approved;
    };

    if !path.contains("/tests/") || !path.ends_with(".rs") {
        return PolicyVerdict::Approved;
    }

    let test_count = content.matches("#[test]").count();
    if test_count <= 1 {
        return PolicyVerdict::Approved;
    }

    PolicyVerdict::Rejected {
        reason: format!(
            "ONE TEST PER WRITE violation: content contains {test_count} #[test] \
             functions. Write exactly one #[test] per write call."
        ),
        feedback: vec![FeedbackNote::required(
            "OneTestPerWritePolicy",
            format!(
                "Write one #[test] function, then compile/check --tests, \
                 fix any errors, then write the next test. \
                 Found {test_count} #[test] functions — write was rejected."
            ),
        )],
    }
}

fn check_compile_after_test_write(call: &ToolCall, ctx: &PolicyContext) -> PolicyVerdict {
    // Only enforce in the write playbook step
    if ctx.playbook_step != "write" {
        return PolicyVerdict::Approved;
    }

    let ToolCall::Write { path, .. } = call else {
        return PolicyVerdict::Approved;
    };

    if !path.contains("/tests/") || !path.ends_with(".rs") {
        return PolicyVerdict::Approved;
    }

    let Some(last_write) = &ctx.last_test_write else {
        return PolicyVerdict::Approved;
    };

    let last_compile_step = ctx.last_compile_check
        .as_ref()
        .map(|c| c.step)
        .unwrap_or(0);

    if last_compile_step <= last_write.step {
        PolicyVerdict::Rejected {
            reason: format!(
                "OODA violation: test written at step {} but compile/check \
                 not called since then (last check: step {}). \
                 Call compile/check with tests:true before writing another test.",
                last_write.step, last_compile_step,
            ),
            feedback: vec![FeedbackNote::required(
                "WriteConstraintPolicy",
                "After every test write, call compile/check with tests:true \
                 before writing the next test. This is the OODA loop — \
                 ACT then OBSERVE before acting again."
                    .to_string(),
            )],
        }
    } else {
        PolicyVerdict::Approved
    }
}

//! ConstraintCheckerActor — second stage of the policy pipeline.
//!
//! Receives a NormalizedToolCall and fans out to domain policy checks,
//! then reduces their verdicts into a single PipelineResult:
//!
//!   - Any Rejected → Block with aggregated feedback
//!   - Any Modified → Execute modified call with feedback
//!   - All Approved → Execute original call with any info feedback
//!
//! # Current policy checks (inline)
//!
//!   - LoopEnforcementPolicy   — consecutive rejection and sequence detection
//!   - ApprovedToolsPolicy     — is this tool allowed in the current playbook step?
//!   - ForbiddenToolsPolicy    — is this tool explicitly forbidden?
//!   - WriteConstraintPolicy   — compile/check required after test write?
//!   - OneTestPerWritePolicy   — only one #[test] per write to a test file?
//!
//! # Loop detection architecture
//!
//! ConstraintCheckerActor is the authoritative enforcer for loop detection
//! because it sees every tool call before and after execution. Two modes:
//!
//! ## Mode 1: Consecutive rejection
//! Same tool summary rejected N times in a row. Simple per-tool counter,
//! resets on any successful execution of that tool. Enforced immediately
//! without trajectory analysis.
//!
//! ## Mode 2: Sequence repetition (future)
//! A pattern of M calls repeating K times, e.g.:
//!   [orient-report, write, compile-check] × 3
//! Catches multi-tool loops that single-tool detection misses. Relevant
//! even in constrained playbook steps where the agent cycles through a
//! small approved set without making progress.
//! The call_window VecDeque is already populated for this purpose.
//!
//! # Future architecture
//!
//! As constraint complexity grows, split into sub-actors:
//!   - StaticPolicyActor    — stateless checks (approved/forbidden/one-test-per-write)
//!   - LoopEnforcementActor — stateful rejection and sequence tracking
//!   - ConstraintCheckerActor — coordinator, fan-out, reduce verdicts
//!
//! For now all checks live here to keep the pipeline simple and the
//! actor count low. The seam is clear: stateless checks take only the
//! current call and context; stateful checks also read actor state.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use mswea_core::{
    policy::{
        FeedbackNote, PipelineResult, PolicyContext, PolicyVerdict,
        LastTestWrite, LastCompileCheck,
    },
    toolbox::ToolRegistry,
    ToolCall,
};
use crate::policy_messages::{
    ConstraintRequest, PolicyContextUpdate, ToolCallCompleted, ToolCallRejected,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Number of consecutive rejections of the same tool before hard-blocking
/// with a halt directive. Chosen to allow one genuine retry (e.g. fixing args)
/// while preventing runaway loops.
const CONSECUTIVE_REJECTION_WARN_THRESHOLD: u32 = 2;
const CONSECUTIVE_REJECTION_HALT_THRESHOLD: u32 = 3;

/// Sliding window size for sequence repetition detection (future).
const CALL_WINDOW_SIZE: usize = 30;

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct ConstraintCheckerActor;

pub struct ConstraintCheckerArgs {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
}

pub struct ConstraintCheckerState {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    context: PolicyContext,

    // ── Loop detection state ──────────────────────────────────────────────────
    /// Consecutive rejection count per tool summary.
    /// Reset to 0 when any call to that tool succeeds.
    consecutive_rejections: HashMap<String, u32>,

    /// Sliding window of recent call summaries for sequence detection.
    /// Prefixed with "REJECTED:" for blocked calls so patterns that include
    /// rejections are distinguishable from successful sequences.
    /// Future: implement Lempel-Ziv or simple subsequence matching here.
    call_window: VecDeque<String>,
}

pub enum ConstraintCheckerMsg {
    Check(ConstraintRequest),
    UpdateContext(PolicyContextUpdate),
    ToolCallCompleted(ToolCallCompleted),
    ToolCallRejected(ToolCallRejected),
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
            consecutive_rejections: HashMap::new(),
            call_window: VecDeque::with_capacity(CALL_WINDOW_SIZE),
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
                let result = check(
                    &req.normalized.call,
                    &state.context,
                    &req.normalized.feedback,
                    &state.tool_registry,
                    &state.consecutive_rejections,
                    &state.call_window,
                ).await;
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
                // Successful execution resets the consecutive rejection counter
                // for this tool — the agent found a valid approach.
                if state.consecutive_rejections.remove(&completed.call_summary).is_some() {
                    tracing::debug!(
                        tool = %completed.call_summary,
                        "ConstraintCheckerActor: rejection counter reset after success"
                    );
                }

                // Update sliding window for future sequence detection
                push_window(&mut state.call_window, completed.call_summary.clone());

                // Update PolicyContext tracking fields
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
                        error_count: 0,
                    });
                }
            }

            ConstraintCheckerMsg::ToolCallRejected(rejected) => {
                let count = state.consecutive_rejections
                    .entry(rejected.call_summary.clone())
                    .or_insert(0);
                *count += 1;

                // Push to window with rejection marker for sequence detection
                push_window(
                    &mut state.call_window,
                    format!("REJECTED:{}", rejected.call_summary),
                );

                tracing::warn!(
                    tool = %rejected.call_summary,
                    consecutive = *count,
                    reason = %rejected.reason,
                    "ConstraintCheckerActor: consecutive rejection recorded"
                );
            }
        }
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn push_window(window: &mut VecDeque<String>, entry: String) {
    if window.len() >= CALL_WINDOW_SIZE {
        window.pop_front();
    }
    window.push_back(entry);
}

// ── Constraint checks ─────────────────────────────────────────────────────────

async fn check(
    call: &ToolCall,
    ctx: &PolicyContext,
    prior_feedback: &[FeedbackNote],
    _registry: &Arc<RwLock<ToolRegistry>>,
    consecutive_rejections: &HashMap<String, u32>,
    call_window: &VecDeque<String>,
) -> PipelineResult {
    let mut feedback: Vec<FeedbackNote> = prior_feedback.to_vec();
    let mut verdicts: Vec<PolicyVerdict> = Vec::new();

    // Loop enforcement runs first — if the agent is stuck, don't bother
    // running other checks, just tell it to change approach or halt.
    verdicts.push(check_consecutive_rejections(call, consecutive_rejections));
    verdicts.push(check_sequence_repetition(call, call_window));

    // Stateless policy checks
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

    PipelineResult::Execute { call: call.clone(), feedback }
}

// ── Individual constraint implementations ─────────────────────────────────────

/// Loop enforcement — consecutive rejection check.
///
/// Stateful: reads consecutive_rejections map built from ToolCallRejected messages.
/// Escalates through warning → hard block with halt directive.
///
/// Note: This is Mode 1 of loop detection. Mode 2 (sequence repetition) will
/// read the call_window VecDeque maintained in ConstraintCheckerState and detect
/// repeating subsequences of M calls. Implemented when Mode 1 proves insufficient.
fn check_consecutive_rejections(
    call: &ToolCall,
    consecutive_rejections: &HashMap<String, u32>,
) -> PolicyVerdict {
    let summary = call.summary();
    let count = consecutive_rejections.get(&summary).copied().unwrap_or(0);

    if count >= CONSECUTIVE_REJECTION_HALT_THRESHOLD {
        PolicyVerdict::Rejected {
            reason: format!(
                "LOOP DETECTED: '{summary}' has been rejected {count} times in a row. \
                 You are stuck in a loop. You MUST either: \
                 (1) call task/halt with a specific explanation of why you are blocked, or \
                 (2) call meta/orient-report to reassess your approach before trying again. \
                 Do NOT repeat this call."
            ),
            feedback: vec![FeedbackNote::required(
                "LoopEnforcement",
                format!(
                    "Consecutive rejection limit ({CONSECUTIVE_REJECTION_HALT_THRESHOLD}) reached \
                     for '{summary}'. Change approach or halt."
                ),
            )],
        }
    } else if count >= CONSECUTIVE_REJECTION_WARN_THRESHOLD {
        // Warning — still allowed but model is put on notice
        PolicyVerdict::Approved // feedback attached via FeedbackNote below
        // TODO: return a Modified verdict with warning feedback attached
        // For now just log — the hard block at threshold+1 is the enforcement
    } else {
        PolicyVerdict::Approved
    }
}

/// Loop enforcement — sequence repetition check (Mode 2).
///
/// Stateful: reads call_window built from both ToolCallCompleted and
/// ToolCallRejected messages. Detects when the same tool appears too
/// frequently in the recent window, regardless of whether it was rejected.
///
/// This catches loops that Mode 1 (consecutive rejection) misses — specifically
/// tools that succeed but produce no progress, like orient-report called
/// repeatedly without advancing. A successful call that loops is still a loop.
///
/// Future enhancement: detect repeating *subsequences* of M calls using
/// Lempel-Ziv or simple sliding window pattern matching, e.g.:
///   [orient-report, write, compile-check] repeating 3x
/// The call_window contains the full sequence needed for this analysis.
fn check_sequence_repetition(
    call: &ToolCall,
    call_window: &VecDeque<String>,
) -> PolicyVerdict {
    // Only check nushell tools — primitives (read/write/edit) are expected
    // to appear frequently and are controlled by other policies.
    let ToolCall::NushellTool { .. } = call else {
        return PolicyVerdict::Approved;
    };

    let summary = call.summary();

    // Count appearances in the recent window, excluding rejection markers
    let recent_count = call_window
        .iter()
        .filter(|s| s.as_str() == summary)
        .count();

    // Threshold: 5 appearances in the last CALL_WINDOW_SIZE (30) calls
    // is a strong signal of looping. Tuned conservatively to avoid
    // false positives on legitimately frequent tools like task/state.
    const SEQUENCE_REPEAT_THRESHOLD: usize = 5;

    if recent_count >= SEQUENCE_REPEAT_THRESHOLD {
        PolicyVerdict::Rejected {
            reason: format!(
                "SEQUENCE LOOP DETECTED: '{summary}' has been called {recent_count} times \
                 in the last {} steps without meaningful progress. \
                 You are stuck in a loop. You MUST either: \
                 (1) call task/halt with a specific explanation, or \
                 (2) call a different tool to make actual progress. \
                 Do NOT call '{summary}' again until you have made progress.",
                call_window.len(),
            ),
            feedback: vec![FeedbackNote::required(
                "LoopEnforcement",
                format!(
                    "Sequence repetition limit ({SEQUENCE_REPEAT_THRESHOLD}) reached \
                     for '{summary}'. This tool has appeared {recent_count} times recently. \
                     Change strategy or call task/halt."
                ),
            )],
        }
    } else {
        PolicyVerdict::Approved
    }
}

fn check_approved_tools(call: &ToolCall, ctx: &PolicyContext) -> PolicyVerdict {
    if ctx.approved_tools.is_empty() {
        return PolicyVerdict::Approved;
    }

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

    // Primitive tools are always permitted — they're infrastructure
    if matches!(call, ToolCall::Read { .. } | ToolCall::Write { .. }
        | ToolCall::Edit { .. } | ToolCall::Search { .. }) {
        return PolicyVerdict::Approved;
    }

    if ctx.approved_tools.iter().any(|t| t == &tool_name)
        || ctx.global_approved_tools.iter().any(|t| t == &tool_name)
    {
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
                 fix any errors, then use the edit tool to append the next test. \
                 Found {test_count} #[test] functions — write was rejected."
            ),
        )],
    }
}

fn check_compile_after_test_write(call: &ToolCall, ctx: &PolicyContext) -> PolicyVerdict {
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

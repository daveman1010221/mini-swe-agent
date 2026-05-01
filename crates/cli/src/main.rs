//! `mswea` — mini-swe-agent entry point.

mod args;
mod config_loader;
mod exit;
mod logging;
mod wiring;

use std::sync::Arc;
use actors::policy_messages::{NormalizeRequest, ConstraintRequest};

use anyhow::{bail, Result};
use clap::Parser;
use models::ModelRequest;
use mswea_core::{
    config::TaskFile,
    error::ExitStatus,
    event::{Event, EventKind},
    message::Message,
    observation::Observation,
    ToolCall,
};
use ractor::{call_t, port::OutputPort};
use tracing::{error, info, warn};

use actors::tool_router::RouteRequest;
use actors::ConstraintCheckerMsg;
use actors::policy_messages::ToolCallCompleted;
use args::CliArgs;
use config_loader::resolve_config;
use wiring::{boot_actor_system, shutdown_actor_system, ActorSystem};

/// RPC timeout for a single tool call in milliseconds.
const TOOL_CALL_TIMEOUT_MS: u64 = 30_000;

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();
    logging::init(args.verbose, args.json_logs);

    match run(args).await {
        Ok(status) => {
            let code = exit::exit_code(&status);
            info!(exit_status = %status, code, "Agent finished");
            std::process::exit(code);
        }
        Err(e) => {
            error!(error = %e, "Fatal error");
            std::process::exit(1);
        }
    }
}

fn attach_feedback(obs: Observation, feedback: Vec<mswea_core::policy::FeedbackNote>) -> Observation {
    let note = feedback.iter().map(|n| n.render()).collect::<Vec<_>>().join("\n");
    match obs {
        Observation::Structured { value, exit_code, feedback: existing } => {
            Observation::Structured {
                value,
                exit_code,
                feedback: Some(match existing {
                    Some(e) => format!("{e}\n{note}"),
                    None => note,
                }),
            }
        }
        Observation::FileWritten { path, lines_changed, feedback: existing } => {
            Observation::FileWritten {
                path,
                lines_changed,
                feedback: Some(match existing {
                    Some(e) => format!("{e}\n{note}"),
                    None => note,
                }),
            }
        }
        other => other,
    }
}

async fn run(args: CliArgs) -> Result<ExitStatus> {
    match dotenv::dotenv() {
        Ok(path) => info!(path = %path.display(), "Loaded .env"),
        Err(_) => {}
    }

    // Validate: --task and --task-file are mutually exclusive
    if args.task.is_some() && args.task_file.is_some() {
        bail!("--task and --task-file are mutually exclusive — use one or the other");
    }

    let mut config = resolve_config(&args)?;

    // ── Task file loading ────────────────────────────────────────────────────
    let (task_file, current_task) = if let Some(ref task_file_path) = args.task_file {
        let tf = TaskFile::load(task_file_path)?;
        info!(path = %task_file_path.display(), "Loaded task file");

        let ct = tf.current_task.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Task file has no current_task — add a task to pending and run `just agent-task` first"
            )
        })?;

        info!(
            crate_name = ct.crate_name().unwrap_or("unknown"),
            op = ct.op.as_deref().unwrap_or("unknown"),
            "Current task loaded"
        );

        (Some(tf), Some(ct))
    } else {
        (None, None)
    };

    // ── Task resolution ──────────────────────────────────────────────────────
    let (task, instance_prompt) = if let Some(ref ct) = current_task {
        let briefing = ct.to_mission_briefing();
        (briefing.clone(), briefing)
    } else {
        if config.run.task.is_none() {
            let task = read_task_from_stdin()?;
            config.run.task = Some(task);
        }
        let task = config.run.task.clone().unwrap();
        (task.clone(), task)
    };

    info!(task = %truncate(&task, 120), "Task loaded");

    // ── Rules and skills for system prompt ───────────────────────────────────
    let rules_section = task_file
        .as_ref()
        .map(|tf| tf.rules_section())
        .unwrap_or_default();

    let skills_section = load_skills();

    let step_limit = config.agent.step_limit;
    let cost_limit = config.agent.cost_limit;
    let model_name = config.model.model_name.clone();

    let mswea_root = std::env::current_dir().unwrap_or_default();
    let system = boot_actor_system(
        config,
        rules_section,
        skills_section,
        current_task.clone(),
        mswea_root,
    ).await?;

    let bus = Arc::clone(&system.event_bus);

    emit(&bus, EventKind::AgentStarted { task: task.clone(), model: model_name });

    let started = std::time::Instant::now();
    let (status, total_steps, total_cost) =
        agent_loop(&system, &bus, instance_prompt, step_limit, cost_limit).await?;

    // ── Review gate ──────────────────────────────────────────────────────────
    if let Some(ct) = &current_task {
        if ct.review && matches!(status, ExitStatus::Submitted) {
            info!("Task has review:true — stopping for human review before advancing");
        }
    }

    emit(&bus, EventKind::AgentFinished {
        exit_status: status.clone(),
        submission: String::new(),
        total_cost,
        total_steps,
        duration_ms: started.elapsed().as_millis() as u64,
    });

    shutdown_actor_system(system).await;
    Ok(status)
}

async fn agent_loop(
    system: &ActorSystem,
    bus: &Arc<OutputPort<Event>>,
    instance_prompt: String,
    step_limit: u32,
    cost_limit: f64,
) -> Result<(ExitStatus, u32, f64)> {
    let system_prompt = system.system_prompt.read().unwrap().clone();

    let mut messages: Vec<Message> = vec![
        Message::system(&system_prompt),
        Message::user(&instance_prompt),
    ];

    let mut step = 0u32;
    let mut total_cost = 0.0f64;

    loop {
        step += 1;
        info!(step, total_cost, "Agent step");
        emit(bus, EventKind::AgentStep { step, cost_so_far: total_cost });

        if step > step_limit {
            warn!(step, step_limit, "Step limit exceeded");
            return Ok((ExitStatus::LimitsExceeded, step, total_cost));
        }
        if total_cost > cost_limit {
            warn!(total_cost, cost_limit, "Cost limit exceeded");
            return Ok((ExitStatus::LimitsExceeded, step, total_cost));
        }

        emit(bus, EventKind::ModelRequestStarted {
            model: "model".into(),
            message_count: messages.len(),
        });

        let req = ModelRequest::new(messages.clone());
        let reply = match system.model.handle(req).await {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, "Model failed");
                emit(bus, EventKind::ModelFailed { error: e.to_string(), attempts: 1 });
                return Ok((ExitStatus::ModelError, step, total_cost));
            }
        };

        total_cost += reply.response.cost_usd;
        emit(bus, EventKind::ModelResponseReceived {
            tokens_in: reply.response.tokens_in,
            tokens_out: reply.response.tokens_out,
            cost_usd: reply.response.cost_usd,
            latency_ms: reply.response.latency_ms,
        });

        messages.push(Message::assistant(&reply.response.raw_text));
        let tool_call = reply.response.tool_call;
        info!(tool_call = %tool_call.summary(), step, "Tool call");
        emit(bus, EventKind::ToolCallEmitted { call: tool_call.clone(), step });

        if let ToolCall::Submit { ref output } = tool_call {
            info!(output = %truncate(output, 200), "Agent submitted");
            return Ok((ExitStatus::Submitted, step, total_cost));
        }

        // ── Policy pipeline ───────────────────────────────────────────────────
        // Stage 1: Normalize args (bool coercion, kebab-case, etc.)
        let normalized = call_t!(
            system.arg_normalizer,
            |reply| NormalizeRequest {
                call: tool_call.clone(),
                context: mswea_core::policy::PolicyContext::initial(),
                step,
                reply,
            },
            TOOL_CALL_TIMEOUT_MS
        )
        .unwrap_or_else(|_| mswea_core::policy::NormalizedToolCall::unchanged(tool_call.clone()));

        // Stage 2: Check constraints
        let pipeline_result = call_t!(
            system.constraint_checker,
            |reply| ConstraintCheckerMsg::Check(ConstraintRequest {
                normalized: normalized.clone(),
                step,
                reply,
            }),
            TOOL_CALL_TIMEOUT_MS
        )
        .unwrap_or_else(|e| mswea_core::policy::PipelineResult::Block {
            reason: format!("ConstraintChecker RPC failed: {e}"),
            feedback: vec![],
        });

        // Stage 3: Execute or block
        let observation = match pipeline_result {
            mswea_core::policy::PipelineResult::Execute { call, feedback } => {
                let mut obs = call_t!(
                    system.tool_router,
                    |reply| RouteRequest { call: call.clone(), step, reply },
                    TOOL_CALL_TIMEOUT_MS
                )
                .unwrap_or_else(|e| Observation::Error {
                    message: format!("ToolRouter RPC failed: {e}"),
                    exit_code: Some(1),
                    tool_call_summary: call.summary(),
                });
                if !feedback.is_empty() {
                    obs = attach_feedback(obs, feedback);
                }
                obs
            }
            mswea_core::policy::PipelineResult::Block { reason, feedback } => {
                let notes = feedback.iter()
                    .map(|n| n.render())
                    .collect::<Vec<_>>()
                    .join("\n");
                Observation::Error {
                    message: if notes.is_empty() {
                        reason
                    } else {
                        format!("{reason}\n\n{notes}")
                    },
                    exit_code: Some(1),
                    tool_call_summary: tool_call.summary(),
                }
            }
        };

        // Notify ConstraintCheckerActor of what just happened
        let was_compile_check = matches!(&tool_call, 
            ToolCall::NushellTool { namespace, tool, .. } 
            if namespace == "compile" && tool == "check"
        );

        let write_path = match &tool_call {
            ToolCall::Write { path, .. } => Some(path.clone()),
            _ => None,
        };

        let compile_clean = if was_compile_check {
            // Extract clean field from observation if it's a structured result
            if let Observation::Structured { .. } = observation {
                // we'll refine this — for now just mark as checked
                Some(true)
            } else {
                Some(false)
            }
        } else {
            None
        };

        let _ = system.constraint_checker.cast(
            ConstraintCheckerMsg::ToolCallCompleted(ToolCallCompleted {
                call_summary: tool_call.summary(),
                step,
                path: write_path,
                was_compile_check,
                compile_clean,
            })
        );

        let obs_json = observation.to_llm_content();
        info!(observation = %truncate(&obs_json.to_string(), 120), "Observation");
        emit(bus, EventKind::ObservationReceived {
            observation: observation.to_archive(),
            duration_ms: 0,
        });

        messages.push(Message::user(format!("Tool result:\n{obs_json}")));
    }
}

fn emit(bus: &Arc<OutputPort<Event>>, kind: EventKind) {
    bus.send(Event::new("agent", kind));
}

fn read_task_from_stdin() -> Result<String> {
    use std::io::Read;
    warn!("No task provided — reading from stdin (Ctrl-D to finish)");
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| anyhow::anyhow!("Failed to read task from stdin: {e}"))?;
    let task = buf.trim().to_string();
    if task.is_empty() {
        bail!("Task is empty — provide via --task, --task-file, $MSWEA_TASK, config file, or stdin");
    }
    Ok(task)
}

/// Load stack-specific skills from the mswea skills directory.
/// Looks in order:
///   1. $MSWEA_SKILLS_DIR
///   2. ~/.config/mswea/skills/
///   3. ./skills/ (relative to cwd)
fn load_skills() -> String {
    let candidates = [
        std::env::var("MSWEA_SKILLS_DIR").ok().map(std::path::PathBuf::from),
        dirs::config_dir().map(|d| d.join("mswea").join("skills")),
        Some(std::path::PathBuf::from("skills")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.is_dir() {
            let mut skills = String::new();
            let mut entries: Vec<_> = match std::fs::read_dir(&candidate) {
                Ok(e) => e.flatten().collect(),
                Err(_) => continue,
            };
            entries.sort_by_key(|e| e.file_name());

            for entry in entries {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        info!(path = %path.display(), "Loaded skill");
                        skills.push_str(&content);
                        skills.push('\n');
                    }
                }
            }

            if !skills.is_empty() {
                return skills;
            }
        }
    }

    String::new()
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

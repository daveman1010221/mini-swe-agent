//! `OrchestratorActor` — maintains the live `CapabilityMap` and system prompt.
//!
//! # Responsibilities
//!
//! - Receives `Capability` announcements (one per tool actor at boot)
//! - Merges them into a `CapabilityMap`
//! - Receives `UpdateToolbox` from `ToolboxActor` — updates tool registry,
//!   playbook registry, skills, preflight result, and current step
//! - Renders the system prompt via minijinja templates
//! - Writes the rendered prompt to a shared `Arc<RwLock<String>>`
//! - Emits `CapabilityMapUpdated` and `SystemPromptRegenerated` events

use std::sync::{Arc, RwLock};

use minijinja::{context, Environment};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::info;

use mswea_core::{
    capability::{builtins, Capability, CapabilityMap},
    event::{Event, EventKind},
    toolbox::{PlaybookStep, PreflightResult, ToolboxUpdate},
};

use crate::event_bus::EventBus;
use crate::constraint_checker::ConstraintCheckerMsg;
use crate::policy_messages::PolicyContextUpdate;
use mswea_core::policy::PolicyContext;

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum OrchestratorMsg {
    /// A tool actor is announcing its capabilities.
    RegisterCapability(Capability),
    /// ToolboxActor is pushing updated toolbox state.
    UpdateToolbox(ToolboxUpdate),
}

// ── Arguments ─────────────────────────────────────────────────────────────────

pub struct OrchestratorArgs {
    pub event_bus: EventBus,
    pub system_prompt: Arc<RwLock<String>>,
    pub cwd: String,
    pub output_path: String,
    pub rules_section: String,
    pub skills_section: String,
    pub constraint_checker: Option<ActorRef<ConstraintCheckerMsg>>,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OrchestratorState {
    capability_map: CapabilityMap,
    event_bus: EventBus,
    system_prompt: Arc<RwLock<String>>,
    cwd: String,
    output_path: String,
    rules_section: String,
    skills_section: String,
    toolbox_section: String,
    ooda_section: String,
    env: Environment<'static>,
    shell_policy_section: String,
    constraint_checker: Option<ActorRef<ConstraintCheckerMsg>>,
}

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct OrchestratorActor;

impl Actor for OrchestratorActor {
    type Msg = OrchestratorMsg;
    type State = OrchestratorState;
    type Arguments = OrchestratorArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!("OrchestratorActor starting");

        let mut env = Environment::new();
        env.add_template_owned("system.j2", SYSTEM_TEMPLATE.to_string())
            .map_err(|e| {
                ActorProcessingErr::from(format!("Failed to load system template: {e}"))
            })?;

        Ok(OrchestratorState {
            capability_map: CapabilityMap::default(),
            event_bus: args.event_bus,
            system_prompt: args.system_prompt,
            cwd: args.cwd,
            output_path: args.output_path,
            rules_section: args.rules_section,
            skills_section: args.skills_section,
            toolbox_section: String::new(),
            ooda_section: String::new(),
            shell_policy_section: String::new(),
            constraint_checker: args.constraint_checker,
            env,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            OrchestratorMsg::RegisterCapability(cap) => {
                let actor_id = cap.actor_id.clone();
                state.capability_map.update(cap);

                info!(
                    actor_id,
                    total_commands = state.capability_map.total_commands(),
                    "CapabilityMap updated"
                );

                state.event_bus.send(Event::new(
                    "orchestrator",
                    EventKind::CapabilityMapUpdated {
                        total_commands: state.capability_map.total_commands(),
                        actor_count:    state.capability_map.actor_count(),
                    },
                ));

                regenerate_prompt(state);
            }

            OrchestratorMsg::UpdateToolbox(update) => {
                info!(
                    tools     = update.tool_registry.count(),
                    playbooks = update.playbook_registry.count(),
                    has_preflight = update.preflight.is_some(),
                    "OrchestratorActor: toolbox updated"
                );

                // Update skills from ToolboxActor (authoritative source)
                state.skills_section = update.skills;

                // Render toolbox section from tool registry
                state.toolbox_section = update.tool_registry.render_prompt_section();

                // Render OODA section from preflight + current step
                state.ooda_section = render_ooda_section(
                    update.preflight.as_ref(),
                    update.current_step.as_ref(),
                );

                state.shell_policy_section = update.shell_policy.render_prompt_section();

                if let Some(ref cc) = state.constraint_checker {
                    if let Some(ref step) = update.current_step {
                        let ctx = PolicyContext {
                            step: 0,
                            playbook_step: step.name.clone(),
                            playbook_index: step.index as u32,
                            approved_tools: step.approved_tools.clone(),
                            forbidden_tools: step.forbidden_tools.clone(),
                            last_tool_call: None,
                            last_tool_step: None,
                            last_compile_check: None,
                            last_test_write: None,
                        };
                        let _ = ractor::call!(
                            cc,
                            |reply| ConstraintCheckerMsg::UpdateContext(PolicyContextUpdate { context: ctx, reply })
                        );
                    }
                }

                regenerate_prompt(state);
            }
        }
        Ok(())
    }
}

// ── Prompt regeneration ───────────────────────────────────────────────────────

fn regenerate_prompt(state: &mut OrchestratorState) {
    let prompt = render_system_prompt(
        &state.env,
        &state.capability_map,
        &state.cwd,
        &state.output_path,
        &state.rules_section,
        &state.skills_section,
        &state.toolbox_section,
        &state.ooda_section,
    );

    let prompt_len = prompt.len();
    *state.system_prompt.write().unwrap() = prompt;

    state.event_bus.send(Event::new(
        "orchestrator",
        EventKind::SystemPromptRegenerated { prompt_len },
    ));

    info!(prompt_len, "System prompt regenerated");
}

// ── OODA section rendering ────────────────────────────────────────────────────

fn render_ooda_section(
    preflight: Option<&PreflightResult>,
    step: Option<&PlaybookStep>,
) -> String {
    match (preflight, step) {
        (Some(pf), Some(s)) => {
            // Collect names of automated steps completed
            let automated: Vec<String> = vec!["survey".to_string()]; // survey is always automated

            pf.render_ooda_section(
                "write-tests", // TODO: pass task_type through
                &s.name,
                s.index,
                6, // TODO: pass total_steps through
                &s.orient_questions,
                &s.approved_tools,
                &automated,
            )
        }
        (None, Some(s)) => {
            // No preflight yet but we have a step — render basic OODA guidance
            let mut out = String::new();
            out.push_str(&format!("## Current Step: {}\n\n", s.name));
            if !s.orient_questions.is_empty() {
                out.push_str("**Orient questions:**\n");
                for q in &s.orient_questions {
                    out.push_str(&format!("- {q}\n"));
                }
                out.push('\n');
            }
            if !s.approved_tools.is_empty() {
                out.push_str("**Approved tools:** ");
                out.push_str(&s.approved_tools.join(", "));
                out.push('\n');
            }
            out
        }
        _ => {
            // No task loaded — render the base OODA standing orders
            String::new()
        }
    }
}

// ── System prompt rendering ───────────────────────────────────────────────────

const SYSTEM_TEMPLATE: &str = include_str!("templates/system.j2");

fn render_system_prompt(
    env: &Environment<'_>,
    map: &CapabilityMap,
    cwd: &str,
    output_path: &str,
    rules_section: &str,
    skills_section: &str,
    toolbox_section: &str,
    ooda_section: &str,
) -> String {
    let tools_section = map.render_system_prompt_section();

    let tmpl = match env.get_template("system.j2") {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get system template — using fallback");
            return render_fallback(map, cwd);
        }
    };

    match tmpl.render(context! {
        tools_section   => tools_section,
        toolbox_section => toolbox_section,
        cwd             => cwd,
        output_path     => output_path,
        rules_section   => rules_section,
        skills_section  => skills_section,
        ooda_section    => ooda_section,
    }) {
        Ok(rendered) => rendered,
        Err(e) => {
            tracing::error!(error = %e, "Failed to render system template — using fallback");
            render_fallback(map, cwd)
        }
    }
}

fn render_fallback(map: &CapabilityMap, cwd: &str) -> String {
    let tools_section = map.render_system_prompt_section();
    format!(
        "You are an autonomous coding agent. \
        On every turn you must respond with exactly one JSON tool call and nothing else.\n\
        \n\
        {tools_section}\
        \n\
        Working directory: {cwd}\n\
        Shell: nushell 0.111\n\
        Submit: {{\"type\":\"submit\",\"output\":\"your answer\"}}\n"
    )
}

// ── Builtin registration ──────────────────────────────────────────────────────

pub fn register_builtins(
    orch: &ActorRef<OrchestratorMsg>,
) -> Result<(), ractor::MessagingErr<OrchestratorMsg>> {
    orch.cast(OrchestratorMsg::RegisterCapability(
        builtins::shell_capabilities("shell-actor"),
    ))?;
    orch.cast(OrchestratorMsg::RegisterCapability(
        builtins::file_capabilities("file-actor"),
    ))?;
    orch.cast(OrchestratorMsg::RegisterCapability(
        builtins::search_capabilities("search-actor"),
    ))?;
    Ok(())
}

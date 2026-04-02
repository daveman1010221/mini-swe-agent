//! `OrchestratorActor` — maintains the live `CapabilityMap` and system prompt.
//!
//! # Responsibilities
//!
//! - Receives `Capability` announcements (one per tool actor at boot)
//! - Merges them into a `CapabilityMap`
//! - Renders the system prompt via minijinja templates
//! - Writes the rendered prompt to a shared `Arc<RwLock<String>>`
//! - Emits `CapabilityMapUpdated` and `SystemPromptRegenerated` events
//!
//! # Template Variables
//!
//! The system prompt template (`system.j2`) receives:
//! - `tools_section`   — rendered from `CapabilityMap`
//! - `cwd`             — working directory
//! - `rules_section`   — from task file rules (never/always)
//! - `ooda_section`    — OODA standing orders (always present)
//! - `skills_section`  — stack knowledge injected at boot
//!
//! # Usage
//!
//! ```rust
//! let prompt = Arc::new(RwLock::new(String::new()));
//! let (orch, _) = Actor::spawn(None, OrchestratorActor, OrchestratorArgs {
//!     event_bus: bus.clone(),
//!     system_prompt: Arc::clone(&prompt),
//!     cwd: "/workspace".into(),
//!     rules_section: String::new(),
//!     skills_section: String::new(),
//! }).await?;
//! ```

use std::sync::{Arc, RwLock};

use minijinja::{context, Environment};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::info;

use mswea_core::{
    capability::{builtins, Capability, CapabilityMap},
    event::{Event, EventKind},
};

use crate::event_bus::EventBus;

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum OrchestratorMsg {
    /// A tool actor is announcing its capabilities.
    RegisterCapability(Capability),
}

// ── Arguments ─────────────────────────────────────────────────────────────────

pub struct OrchestratorArgs {
    pub event_bus: EventBus,
    /// Shared prompt string written here on every capability map update.
    /// The agent loop reads this directly — no RPC needed.
    pub system_prompt: Arc<RwLock<String>>,
    /// Working directory — included in the system prompt.
    pub cwd: String,
    /// Rules section from agent-task.json (never/always rules).
    /// Empty string if no task file is provided.
    pub rules_section: String,
    /// Stack knowledge injected into the prompt (rust-actor-tests, proptests, etc.)
    /// Empty string if no skills are loaded.
    pub skills_section: String,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OrchestratorState {
    capability_map: CapabilityMap,
    event_bus: EventBus,
    system_prompt: Arc<RwLock<String>>,
    cwd: String,
    rules_section: String,
    skills_section: String,
    env: Environment<'static>,
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
            rules_section: args.rules_section,
            skills_section: args.skills_section,
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
                        actor_count: state.capability_map.actor_count(),
                    },
                ));

                let prompt = render_system_prompt(
                    &state.env,
                    &state.capability_map,
                    &state.cwd,
                    &state.rules_section,
                    &state.skills_section,
                );

                let prompt_len = prompt.len();
                *state.system_prompt.write().unwrap() = prompt;

                state.event_bus.send(Event::new(
                    "orchestrator",
                    EventKind::SystemPromptRegenerated { prompt_len },
                ));

                info!(prompt_len, "System prompt regenerated");
            }
        }
        Ok(())
    }
}

// ── System prompt rendering ───────────────────────────────────────────────────

/// The default system prompt template — embedded at compile time.
const SYSTEM_TEMPLATE: &str = include_str!("templates/system.j2");

fn render_system_prompt(
    env: &Environment<'_>,
    map: &CapabilityMap,
    cwd: &str,
    rules_section: &str,
    skills_section: &str,
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
        tools_section => tools_section,
        cwd => cwd,
        rules_section => rules_section,
        skills_section => skills_section,
    }) {
        Ok(rendered) => rendered,
        Err(e) => {
            tracing::error!(error = %e, "Failed to render system template — using fallback");
            render_fallback(map, cwd)
        }
    }
}

/// Fallback prompt if minijinja rendering fails.
fn render_fallback(map: &CapabilityMap, cwd: &str) -> String {
    let tools_section = map.render_system_prompt_section();
    format!(
        "You are an autonomous coding agent. \
        On every turn you must respond with exactly one JSON tool call and nothing else — \
        no explanations, no markdown, no extra text. \
        The JSON must be a single object on one line.\n\
        \n\
        Tool calls use this structure: {{\"type\": \"<tool>\", <args>}}\n\
        \n\
        {tools_section}\
        \n\
        Environment:\n\
        - Shell: nushell 0.111 (NOT bash — use nu syntax)\n\
        - Working directory: {cwd}\n\
        - ls returns structured records; use `ls /path | get name` to list filenames\n\
        - Avoid POSIX flags like -1, -la; use nu flags or pipelines\n\
        - Once you have the information needed, call submit immediately\n\
        - Do not keep looping after the task is done\n\
        \n\
        IMPORTANT: The submit tool requires exactly this field name:\n\
        {{\"type\":\"submit\",\"output\":\"your answer here\"}}\n\
        The field is \"output\", not \"result\" or \"answer\".\n"
    )
}

// ── Builtin registration ──────────────────────────────────────────────────────

/// Register all builtin tool capabilities with the orchestrator at boot.
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

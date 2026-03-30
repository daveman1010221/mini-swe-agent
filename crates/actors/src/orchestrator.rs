//! `OrchestratorActor` — maintains the live `CapabilityMap` and system prompt.
//!
//! # Responsibilities
//!
//! - Receives `Capability` announcements (one per tool actor at boot)
//! - Merges them into a `CapabilityMap`
//! - Regenerates the system prompt and writes it to a shared `Arc<RwLock<String>>`
//! - Emits `CapabilityMapUpdated` and `SystemPromptRegenerated` events
//!
//! # Usage
//!
//! ```rust
//! let prompt = Arc::new(RwLock::new(String::new()));
//! let (orch, _) = Actor::spawn(None, OrchestratorActor, OrchestratorArgs {
//!     event_bus: bus.clone(),
//!     system_prompt: Arc::clone(&prompt),
//! }).await?;
//!
//! // Register builtin capabilities at boot:
//! orch.cast(OrchestratorMsg::RegisterCapability(shell_capabilities("shell-actor")))?;
//! orch.cast(OrchestratorMsg::RegisterCapability(file_capabilities("file-actor")))?;
//! orch.cast(OrchestratorMsg::RegisterCapability(search_capabilities("search-actor")))?;
//!
//! // Agent loop reads prompt cheaply, no RPC needed:
//! let prompt_text = prompt.read().unwrap().clone();
//! ```

use std::sync::{Arc, RwLock};

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
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OrchestratorState {
    capability_map: CapabilityMap,
    event_bus: EventBus,
    system_prompt: Arc<RwLock<String>>,
    cwd: String,
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
        Ok(OrchestratorState {
            capability_map: CapabilityMap::default(),
            event_bus: args.event_bus,
            system_prompt: args.system_prompt,
            cwd: args.cwd,
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

                // Regenerate and publish the system prompt.
                let prompt = render_system_prompt(&state.capability_map, &state.cwd);
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

/// Build the full system prompt from the capability map.
///
/// Structure:
///   1. Role and instructions
///   2. Tool call format rules
///   3. Generated tool descriptions from CapabilityMap
///   4. Environment context
fn render_system_prompt(map: &CapabilityMap, cwd: &str) -> String {
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

/// Register all builtin tool capabilities with the orchestrator at boot.
///
/// Call this from `wiring.rs` after spawning the orchestrator.
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

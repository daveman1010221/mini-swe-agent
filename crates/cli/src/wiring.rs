//! Actor wiring.
//!
//! All actors are now real:
//!   ✅ EventLoggerActor  — OutputPort subscription, JSONL trajectory
//!   ✅ OrchestratorActor — CapabilityMap, system prompt via Arc<RwLock<String>>
//!   ✅ ToolRouterActor   — dispatches ToolCall, emits events
//!   ✅ ModelActor        — LitellmClient, retry
//!   ✅ ShellWorker       — embedded nushell session

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use actors::{
    new_event_bus, register_builtins, EventBus, EventLoggerActor, EventLoggerArgs,
    OrchestratorActor, OrchestratorArgs, ToolRouterActor, ToolRouterArgs,
};
use environments::ShellWorker;
use models::{LitellmClient, ModelActor};
use mswea_core::config::RunConfig;
use ractor::{Actor, ActorRef};
use tracing::{info, warn};

use actors::tool_router::RouteRequest;

/// Live handles to the running actor system.
pub struct ActorSystem {
    pub config: RunConfig,
    pub model: ModelActor,
    pub tool_router: ActorRef<RouteRequest>,
    pub event_bus: EventBus,
    pub system_prompt: Arc<RwLock<String>>,
    pub event_logger: Option<ActorRef<actors::EventLoggerMsg>>,
}

pub async fn boot_actor_system(
    config: RunConfig,
    rules_section: String,
    skills_section: String,
) -> Result<ActorSystem> {
    info!("Booting actor system");

    // ── Event bus ────────────────────────────────────────────────────────────
    let event_bus = new_event_bus();

    // ── EventLoggerActor — subscribe BEFORE anyone emits ─────────────────────
    let event_logger = if let Some(ref output_path) = config.agent.output_path {
        let path = PathBuf::from(output_path).with_extension("jsonl");
        info!(path = %path.display(), "EventLogger: starting");
        let (logger_ref, _handle) = Actor::spawn(
            Some("event-logger".into()),
            EventLoggerActor,
            EventLoggerArgs {
                event_bus: Arc::clone(&event_bus),
                output_path: path,
            },
        )
        .await
        .context("Spawning EventLoggerActor")?;
        Some(logger_ref)
    } else {
        warn!("EventLogger: no --output configured — trajectory will not be persisted");
        None
    };

    // ── OrchestratorActor ────────────────────────────────────────────────────
    let system_prompt = Arc::new(RwLock::new(String::new()));
    let (orch_ref, _orch_handle) = Actor::spawn(
        Some("orchestrator".into()),
        OrchestratorActor,
        OrchestratorArgs {
            event_bus: Arc::clone(&event_bus),
            system_prompt: Arc::clone(&system_prompt),
            cwd: config.shell.cwd.clone(),
            rules_section,
            skills_section,
        },
    )
    .await
    .context("Spawning OrchestratorActor")?;

    register_builtins(&orch_ref).context("Registering builtin capabilities")?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    info!(prompt_len = system_prompt.read().unwrap().len(), "OrchestratorActor: ready");

    // ── ShellWorker ──────────────────────────────────────────────────────────
    let shell = ShellWorker::spawn(&config.shell.cwd)
        .context("Spawning ShellWorker")?;
    info!(cwd = %config.shell.cwd, "ShellWorker: ready");

    // ── ToolRouterActor ──────────────────────────────────────────────────────
    let (tool_router, _router_handle) = Actor::spawn(
        Some("tool-router".into()),
        ToolRouterActor,
        ToolRouterArgs {
            shell,
            event_bus: Arc::clone(&event_bus),
            cwd: config.shell.cwd.clone(),
        },
    )
    .await
    .context("Spawning ToolRouterActor")?;
    info!("ToolRouterActor: ready");

    // ── ModelActor ───────────────────────────────────────────────────────────
    let client = LitellmClient::from_env(&config.model.model_name)
        .context("Building LitellmClient")?;
    let model = ModelActor::new(client, "model-actor");
    info!(model = %config.model.model_name, "ModelActor: ready");

    Ok(ActorSystem {
        config,
        model,
        tool_router,
        event_bus,
        system_prompt,
        event_logger,
    })
}

pub async fn shutdown_actor_system(system: ActorSystem) {
    info!("Shutting down actor system");
    system.tool_router.stop(None);
    if let Some(ref logger) = system.event_logger {
        logger.stop(None);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let _ = system;
}

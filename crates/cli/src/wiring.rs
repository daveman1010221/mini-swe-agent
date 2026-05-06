//! Actor wiring.
//!
//! Boot order:
//!   1. EventLoggerActor  — subscribe before anyone emits
//!   2. OrchestratorActor — capability map + system prompt
//!   3. ToolboxActor      — tool registry, playbooks, skills, preflight
//!   4. ShellWorker       — embedded nushell session
//!   5. ToolRouterActor   — dispatches ToolCall
//!   6. ModelActor        — LiteLLM client

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use environments::ShellWorker;
use models::{LitellmClient, ModelActor};
use mswea_core::{
    config::{CurrentTask, RunConfig},
    toolbox::{PlaybookRegistry, ToolRegistry, ShellPolicy},
};
use ractor::{Actor, ActorRef};
use ractor_cluster::{NodeServer, node::NodeConnectionMode};
use std::net::IpAddr;
use std::net::Ipv4Addr;
use tokio::sync::RwLock as AsyncRwLock;
use tracing::{info, warn};

use actors::{
    new_event_bus, register_builtins, EventBus, EventLoggerActor, EventLoggerArgs,
    OrchestratorActor, OrchestratorArgs, ToolboxActor, ToolboxArgs, ToolboxMsg,
    ToolRouterActor, ToolRouterArgs, ConstraintCheckerActor, ConstraintCheckerArgs,
    ArgNormalizerActor, ArgNormalizerArgs, TaskActor, TaskActorArgs, TaskMsg,
};
use actors::constraint_checker::ConstraintCheckerMsg;
use actors::policy_messages::NormalizeRequest;
use actors::tool_router::RouteRequest;

/// Live handles to the running actor system.
pub struct ActorSystem {
    pub model: ModelActor,
    pub tool_router: ActorRef<RouteRequest>,
    pub arg_normalizer: ActorRef<NormalizeRequest>,
    pub constraint_checker: ActorRef<ConstraintCheckerMsg>,
    pub task_actor: ActorRef<TaskMsg>,
    pub event_bus: EventBus,
    pub system_prompt: Arc<RwLock<String>>,
    pub event_logger: Option<ActorRef<actors::EventLoggerMsg>>,
    pub toolbox: ActorRef<ToolboxMsg>,
    pub cluster_node: ActorRef<ractor_cluster::NodeServerMessage>,
    pub step_banner_text: Arc<RwLock<String>>,
    pub step_banner: bool,
}

pub async fn boot_actor_system(
    config: RunConfig,
    rules_section: String,
    skills_section: String,
    current_task: Option<CurrentTask>,
    mswea_root: PathBuf,
) -> Result<ActorSystem> {
    info!("Booting actor system");

    // ── Cluster node ─────────────────────────────────────────────────────────
    // Start the mswea-core ractor cluster node. The nu-plugin-mswea process
    // will connect to this node to resolve ActorRefs to TaskActor and
    // ConstraintCheckerActor without HTTP.
    //
    // Port 9000 is the cluster port — distinct from the legacy RPC port 8000.
    // Cookie is a shared secret between the mswea-core and mswea-plugin nodes.
    // We use Isolated mode — the plugin connects to us, we don't discover peers.
    let cluster_cookie = "mswea-cluster-cookie".to_string();
    let cluster_port: u16 = 9000;
    let (cluster_node, _cluster_handle) = Actor::spawn(
        Some("mswea-core-node".into()),
        NodeServer::new(
            cluster_port,
            cluster_cookie.clone(),
            "mswea-core".to_string(),
            "localhost".to_string(),
            None, // IncomingEncryptionMode::Raw — localhost only
            Some(NodeConnectionMode::Isolated),
        ).with_listen_addr(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        (),
    )
    .await
    .context("Spawning cluster NodeServer")?;
    info!(port = cluster_port, "Cluster node started: mswea-core");

    // ── Event bus ────────────────────────────────────────────────────────────
    let event_bus = new_event_bus();

    // ── EventLoggerActor ─────────────────────────────────────────────────────
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

    // ── Shared state ─────────────────────────────────────────────────────────
    // One registry shared across all actors — ToolboxActor writes, others read.
    let tool_registry = Arc::new(AsyncRwLock::new(ToolRegistry::default()));
    let playbook_registry = Arc::new(AsyncRwLock::new(PlaybookRegistry::default()));
    let shell_policy = Arc::new(AsyncRwLock::new(ShellPolicy::default()));
    let step_banner_text = Arc::new(RwLock::new(String::new()));

    // ── ConstraintCheckerActor ────────────────────────────────────────────────
    let (constraint_checker_ref, _cc_handle) = Actor::spawn(
        Some("constraint-checker".into()),
        ConstraintCheckerActor,
        ConstraintCheckerArgs {
            tool_registry: Arc::clone(&tool_registry),
        },
    )
    .await
    .context("Spawning ConstraintCheckerActor")?;

    ractor::pg::join("mswea-constraint-checkers".to_string(), vec![constraint_checker_ref.get_cell()]);

    // ── ArgNormalizerActor ────────────────────────────────────────────────────
    let (arg_normalizer_ref, _norm_handle) = Actor::spawn(
        Some("arg-normalizer".into()),
        ArgNormalizerActor,
        ArgNormalizerArgs {
            tool_registry: Arc::clone(&tool_registry),
        },
    )
    .await
    .context("Spawning ArgNormalizerActor")?;
    info!("ArgNormalizerActor: ready");

    // ── OrchestratorActor ────────────────────────────────────────────────────
    let system_prompt = Arc::new(RwLock::new(String::new()));

    let output_path = config.agent.output_path
        .clone()
        .unwrap_or_default();

    let (orch_ref, _orch_handle) = Actor::spawn(
        Some("orchestrator".into()),
        OrchestratorActor,
        OrchestratorArgs {
            event_bus:          Arc::clone(&event_bus),
            system_prompt:      Arc::clone(&system_prompt),
            step_banner_text:   Arc::clone(&step_banner_text),
            cwd:                config.shell.cwd.clone(),
            output_path:        format!("{output_path}.jsonl"),
            rules_section,
            skills_section,
            constraint_checker: Some(constraint_checker_ref.clone()),
        },
    )
    .await
    .context("Spawning OrchestratorActor")?;

    register_builtins(&orch_ref).context("Registering builtin capabilities")?;

    let mut shell_env = config.shell.env.clone();
    shell_env.insert("WORKSPACE_ROOT".into(), config.shell.cwd.clone());
    shell_env.insert("MSWEA_CLUSTER_ADDR".into(), format!("127.0.0.1:{cluster_port}"));
    shell_env.insert("MSWEA_CLUSTER_COOKIE".into(), cluster_cookie.clone());
    if let Some(ref tf) = config.agent.task_file {
        shell_env.insert("TASKFILE".into(), tf.display().to_string());
        tracing::info!(taskfile = %tf.display(), "Injecting TASKFILE into shell env");
    }

    // ── TaskActor ─────────────────────────────────────────────────────────────────
    let task_file_path = config.agent.task_file
        .clone()
        .ok_or_else(|| anyhow::anyhow!("TaskActor requires --task-file to be set"))?;

    let (task_actor_ref, _task_handle) = Actor::spawn(
        Some("task-actor".into()),
        TaskActor,
        TaskActorArgs {
            taskfile_path: task_file_path,
            constraint_checker: constraint_checker_ref.clone(),
            orchestrator: orch_ref.clone(),
            event_bus: Arc::clone(&event_bus),
            playbook_registry: Arc::clone(&playbook_registry),
        },
    )
    .await
    .context("Spawning TaskActor")?;
    info!("TaskActor: ready, RPC on :8000");

    info!("TaskActor supports_remoting: {}", task_actor_ref.get_cell().supports_remoting());

    ractor::pg::join("mswea-task-actors".to_string(), vec![task_actor_ref.get_cell()]);

    // ── ToolboxActor ─────────────────────────────────────────────────────────
    let shell_for_toolbox = Arc::new(AsyncRwLock::new(
        ShellWorker::spawn(&config.shell.cwd, &shell_env).context("Spawning ShellWorker for ToolboxActor")?
    ));

    let (toolbox_ref, _toolbox_handle) = Actor::spawn(
        Some("toolbox".into()),
        ToolboxActor,
        ToolboxArgs {
            event_bus:         Arc::clone(&event_bus),
            orchestrator:      orch_ref.clone(),
            mswea_root,
            shell:             Arc::clone(&shell_for_toolbox),
            tool_registry:     Arc::clone(&tool_registry),
            shell_policy:      Arc::clone(&shell_policy),
            playbook_registry: Arc::clone(&playbook_registry),
        },
    )
    .await
    .context("Spawning ToolboxActor")?;

    // If a task is already loaded, trigger preflight immediately
    if let Some(task) = current_task {
        toolbox_ref
            .cast(ToolboxMsg::TaskLoaded(task))
            .context("Sending TaskLoaded to ToolboxActor")?;
    }

    // Give ToolboxActor time to scan and push first update to Orchestrator
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    info!(prompt_len = system_prompt.read().unwrap().len(), "OrchestratorActor: ready");

    // ── ShellWorker (agent's shell) ──────────────────────────────────────────
    let shell = ShellWorker::spawn(&config.shell.cwd, &shell_env)
        .context("Spawning ShellWorker")?;

    // Register nu_plugin_mswea so tool scripts can call mswea commands
    let plugin_binary = std::env::current_exe()
        .context("Finding current exe")?
        .parent()
        .context("Finding exe directory")?
        .join("nu_plugin_mswea");

    shell.register_mswea_plugin(&plugin_binary)
        .await
        .context("Registering nu_plugin_mswea")?;

    info!(cwd = %config.shell.cwd, "ShellWorker: ready");

    // ── ToolRouterActor ──────────────────────────────────────────────────────
    let (tool_router, _router_handle) = Actor::spawn(
        Some("tool-router".into()),
        ToolRouterActor,
        ToolRouterArgs {
            shell,
            event_bus: Arc::clone(&event_bus),
            cwd: config.shell.cwd.clone(),
            tool_registry: Arc::clone(&tool_registry),
            shell_policy: Arc::clone(&shell_policy),
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
        model,
        tool_router,
        arg_normalizer: arg_normalizer_ref,
        constraint_checker: constraint_checker_ref,
        task_actor: task_actor_ref,
        event_bus,
        system_prompt,
        event_logger,
        toolbox: toolbox_ref,
        cluster_node,
        step_banner_text,
        step_banner: config.agent.step_banner,
    })
}

pub async fn shutdown_actor_system(system: ActorSystem) {
    info!("Shutting down actor system");
    system.tool_router.stop(None);
    system.task_actor.stop(None);
    system.constraint_checker.stop(None);
    system.arg_normalizer.stop(None);
    system.toolbox.stop(None);
    if let Some(ref logger) = system.event_logger {
        logger.stop(None);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    system.cluster_node.stop(None);
    let _ = system;
}

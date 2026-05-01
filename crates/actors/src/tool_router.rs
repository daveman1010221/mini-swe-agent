//! `ToolRouterActor` — receives `ToolCall` messages and dispatches to the
//! appropriate handler, returning an `Observation`.
//!
//! Dispatch table:
//!   - `Shell`       → `ShellWorker`
//!   - `Read`        → `file_ops::read_file`
//!   - `Write`       → `file_ops::write_file`
//!   - `Edit`        → `file_ops::edit_file`
//!   - `Search`      → `file_ops::search`
//!   - `NushellTool` → look up script in `ToolRegistry`, exec via `ShellWorker`
//!   - `Submit`      → immediate `Observation::Submitted`

use std::sync::Arc;

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use tokio::sync::RwLock;
use tracing::instrument;

use environments::{edit_file, read_file, search, write_file, ShellWorker};
use mswea_core::{
    event::{Event, EventKind},
    observation::Observation,
    toolbox::ToolRegistry,
    ShellPolicy, ToolCall,
};

use crate::event_bus::EventBus;

// ── Messages ──────────────────────────────────────────────────────────────────

pub struct RouteRequest {
    pub call: ToolCall,
    pub step: u32,
    pub reply: RpcReplyPort<Observation>,
}

impl std::fmt::Debug for RouteRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteRequest")
            .field("call", &self.call.summary())
            .field("step", &self.step)
            .finish()
    }
}

// ── Arguments ─────────────────────────────────────────────────────────────────

pub struct ToolRouterArgs {
    pub shell: ShellWorker,
    pub event_bus: EventBus,
    pub cwd: String,
    /// Shared tool registry — updated by ToolboxActor.
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Shared shell policy — updated by ToolboxActor.
    pub shell_policy: Arc<RwLock<ShellPolicy>>,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct ToolRouterState {
    shell: ShellWorker,
    event_bus: EventBus,
    cwd: String,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    shell_policy: Arc<RwLock<ShellPolicy>>,
}

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct ToolRouterActor;

impl Actor for ToolRouterActor {
    type Msg = RouteRequest;
    type State = ToolRouterState;
    type Arguments = ToolRouterArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("ToolRouterActor starting");
        Ok(ToolRouterState {
            shell: args.shell,
            event_bus: args.event_bus,
            cwd: args.cwd,
            tool_registry: args.tool_registry,
            shell_policy: args.shell_policy,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        req: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let obs = dispatch(&req.call, req.step, state).await;
        let _ = req.reply.send(obs);
        Ok(())
    }
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

#[instrument(skip(state), fields(tool = %call.summary()))]
async fn dispatch(call: &ToolCall, step: u32, state: &ToolRouterState) -> Observation {
    match call {
        ToolCall::Shell { command } => {
            state.event_bus.send(Event::new(
                "tool-router",
                EventKind::ShellCommandStarted {
                    command: command.clone(),
                    cwd: state.cwd.clone(),
                },
            ));

            // Policy check — read lock is non-blocking in the happy path
            let policy = state.shell_policy.read().await;
            if let Err(msg) = policy.check(command) {
                return Observation::Error {
                    message: msg,
                    exit_code: Some(126), // POSIX: command not executable
                    tool_call_summary: format!("shell[blocked]: {}", truncate(command, 60)),
                };
            }
            drop(policy);

            match state.shell.exec(command).await {
                Ok(o) => {
                    if let Observation::Structured { exit_code, .. } = &o {
                        state.event_bus.send(Event::new(
                            "tool-router",
                            EventKind::ShellCommandCompleted {
                                exit_code: *exit_code,
                                duration_ms: 0,
                                structured: true,
                            },
                        ));
                    }
                    o
                }
                Err(e) => {
                    state.event_bus.send(Event::new(
                        "tool-router",
                        EventKind::ShellCommandFailed {
                            error: e.to_string(),
                            exit_code: Some(1),
                        },
                    ));
                    Observation::Error {
                        message: e.to_string(),
                        exit_code: Some(1),
                        tool_call_summary: format!("shell: {}", truncate(command, 60)),
                    }
                }
            }
        }

        ToolCall::Read { path } => {
            match read_file(path) {
                Ok(obs) => {
                    if let Observation::FileContent { size_bytes, .. } = &obs {
                        state.event_bus.send(Event::new(
                            "tool-router",
                            EventKind::FileRead { path: path.clone(), size_bytes: *size_bytes },
                        ));
                    }
                    obs
                }
                Err(e) => Observation::Error {
                    message: e.to_string(),
                    exit_code: None,
                    tool_call_summary: format!("read: {path}"),
                },
            }
        }

        ToolCall::Write { path, content } => {
            match write_file(path, content) {
                Ok(obs) => {
                    if let Observation::FileWritten { lines_changed, .. } = &obs {
                        state.event_bus.send(Event::new(
                            "tool-router",
                            EventKind::FileWritten { path: path.clone(), lines_changed: *lines_changed },
                        ));
                    }
                    obs
                }
                Err(e) => Observation::Error {
                    message: e.to_string(),
                    exit_code: None,
                    tool_call_summary: format!("write: {path}"),
                },
            }
        }

        ToolCall::Edit { path, old, new } => {
            match edit_file(path, old, new) {
                Ok(obs) => {
                    state.event_bus.send(Event::new(
                        "tool-router",
                        EventKind::FileEdited {
                            path: path.clone(),
                            old_len: old.len(),
                            new_len: new.len(),
                        },
                    ));
                    obs
                }
                Err(e) => Observation::Error {
                    message: e.to_string(),
                    exit_code: None,
                    tool_call_summary: format!("edit: {path}"),
                },
            }
        }

        ToolCall::Search { query, path, regex } => {
            match search(query, path.as_deref(), *regex) {
                Ok(obs) => obs,
                Err(e) => Observation::Error {
                    message: e.to_string(),
                    exit_code: None,
                    tool_call_summary: format!("search: {}", truncate(query, 40)),
                },
            }
        }

        ToolCall::NushellTool { namespace, tool, args } => {
            let args_val: serde_json::Value = serde_json::from_str(args)
                .unwrap_or_default();
            dispatch_nushell_tool(namespace, tool, &args_val, state).await
        }

        ToolCall::Submit { .. } => Observation::Submitted,
    }
}

// ── NushellTool dispatch ──────────────────────────────────────────────────────

async fn dispatch_nushell_tool(
    namespace: &str,
    tool: &str,
    args: &serde_json::Value,
    state: &ToolRouterState,
) -> Observation {
    let full_name = format!("{namespace}/{tool}");

    // Look up the script path and flags from the tool registry
    let (script_path, tool_flags) = {
        let registry = state.tool_registry.read().await;
        tracing::debug!(count = registry.count(), "ToolRegistry lookup");
        match registry.get(&full_name) {
            Some(e) => (e.script_path.clone(), e.flags.clone()),
            None => {
                return Observation::Error {
                    message: format!(
                        "Unknown nushell tool: {full_name} — run tools/discovery/list.nu to see available tools"
                    ),
                    exit_code: Some(1),
                    tool_call_summary: full_name,
                };
            }
        }
    };

    // Validate args against known flags before invoking nushell
    if let Some(obj) = args.as_object() {
        for (key, _val) in obj {
            let flag_name = key.replace('_', "-");
            match tool_flags.iter().find(|f| f.name == flag_name) {
                Some(_) => {
                    // Valid flag — ArgNormalizerActor handles type coercion upstream
                }
                None => {
                    let usage = tool_flags.iter()
                        .map(|f| f.render_signature())
                        .collect::<Vec<_>>()
                        .join(" ");
                    return Observation::Error {
                        message: format!(
                            "Unknown flag --{flag_name} for {full_name}. Usage: {full_name} {usage}"
                        ),
                        exit_code: Some(1),
                        tool_call_summary: full_name,
                    };
                }
            }
        }
    }

    // Build flags string
    let mut flags = args_to_flags(args);

    // Inject --taskfile for task/* tools if not already provided
    if namespace == "task" && !flags.contains("--taskfile") {
        if let Ok(tf) = std::env::var("TASKFILE") {
            flags = format!("--taskfile {tf} {flags}").trim().to_string();
        }
    }

    let command_summary = format!("tool: {} {}", script_path.display(), flags);

    tracing::info!(
        tool = %full_name,
        script = %script_path.display(),
        "Dispatching nushell tool"
    );

    state.event_bus.send(Event::new(
        "tool-router",
        EventKind::ShellCommandStarted {
            command: command_summary.clone(),
            cwd: state.cwd.clone(),
        },
    ));

    match state.shell.call_tool(&script_path, &flags).await {
        Ok(obs) => {
            if let Observation::Structured { exit_code, .. } = &obs {
                state.event_bus.send(Event::new(
                    "tool-router",
                    EventKind::ShellCommandCompleted {
                        exit_code: *exit_code,
                        duration_ms: 0,
                        structured: true,
                    },
                ));
            }
            obs
        }
        Err(e) => {
            state.event_bus.send(Event::new(
                "tool-router",
                EventKind::ShellCommandFailed {
                    error: e.to_string(),
                    exit_code: Some(1),
                },
            ));
            Observation::Error {
                message: e.to_string(),
                exit_code: Some(1),
                tool_call_summary: format!("{namespace}/{tool}"),
            }
        }
    }
}

/// Convert a JSON args object into nushell flag syntax.
/// `{"taskfile": "/foo", "window": 20}` → `--taskfile /foo --window 20`
fn args_to_flags(args: &serde_json::Value) -> String {
    let obj = match args.as_object() {
        Some(o) => o,
        None => return String::new(),
    };
    obj.iter()
        .map(|(k, v)| {
            let flag = k.replace('_', "-");
            let val = match v {
                serde_json::Value::String(s) => {
                    if s.contains(' ') || s.starts_with('[') || s.starts_with('{') {
                        // Double-quote and escape inner quotes so nushell receives
                        // the value as a proper string, not a single-quoted literal.
                        // Single quotes in nushell prevent all interpolation and break
                        // from json parsing when the value is a JSON array/object.
                        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                        format!("\"{escaped}\"")
                    } else {
                        s.clone()
                    }
                }
                serde_json::Value::Bool(b) if *b => format!("--{flag}"),
                serde_json::Value::Bool(_)       => return String::new(),
                serde_json::Value::Null          => return String::new(),
                other                            => other.to_string(),
            };
            if val.starts_with("--") {
                val
            } else {
                format!("--{flag} {val}")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

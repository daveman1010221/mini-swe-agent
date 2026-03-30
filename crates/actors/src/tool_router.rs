//! `ToolRouterActor` — receives `ToolCall` messages and dispatches to the
//! appropriate handler, returning an `Observation`.
//!
//! # Design
//!
//! The agent loop sends a `RouteRequest` (containing a `ToolCall` and an
//! `RpcReplyPort<Observation>`) to this actor. The actor pattern-matches on
//! the `ToolCall` variant and dispatches:
//!
//!   - `Shell`  → `ShellWorker` (dedicated nu thread, async)
//!   - `Read`   → `file_ops::read_file` (sync, cheap)
//!   - `Write`  → `file_ops::write_file` (sync)
//!   - `Edit`   → `file_ops::edit_file` (sync)
//!   - `Search` → `file_ops::search` (spawns `rg` subprocess)
//!   - `Submit` → immediate `Observation::Submitted`
//!
//! # Future
//!
//! When ShellActor, FileActor, and SearchActor become real ractor actors,
//! replace the direct calls here with RPC calls to their `ActorRef`s.
//! The agent loop and message types stay unchanged.

use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use tracing::{debug, instrument};

use environments::{edit_file, read_file, search, write_file, ShellWorker};
use mswea_core::{
    event::{Event, EventKind},
    observation::Observation,
    ToolCall,
};

use crate::event_bus::EventBus;

// ── Messages ──────────────────────────────────────────────────────────────────

/// Request sent by the agent loop to route a tool call.
pub struct RouteRequest {
    pub call: ToolCall,
    pub step: u32,
    pub reply: RpcReplyPort<Observation>,
}

// ractor requires messages to be Debug — implement manually since
// RpcReplyPort doesn't implement Debug.
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
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct ToolRouterState {
    shell: ShellWorker,
    event_bus: EventBus,
    cwd: String,
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
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        req: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let obs = dispatch(&req.call, req.step, state).await;
        // Reply to the caller — ignore error if they dropped the port.
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

            let obs = match state.shell.exec(command).await {
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
            };
            obs
        }

        ToolCall::Read { path } => {
            match read_file(path) {
                Ok(obs) => {
                    if let Observation::FileContent { size_bytes, .. } = &obs {
                        state.event_bus.send(Event::new(
                            "tool-router",
                            EventKind::FileRead {
                                path: path.clone(),
                                size_bytes: *size_bytes,
                            },
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
                            EventKind::FileWritten {
                                path: path.clone(),
                                lines_changed: *lines_changed,
                            },
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

        ToolCall::Submit { .. } => Observation::Submitted,
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

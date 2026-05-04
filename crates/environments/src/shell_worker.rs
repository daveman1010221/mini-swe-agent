//! `ShellWorker` — dedicated thread owning the `NushellSession`.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use anyhow::{anyhow, Context, Result};
use mswea_core::observation::Observation;
use tracing::{debug, error, info};

use crate::session::NushellSession;
use crate::value_to_observation::value_to_observation;

enum ShellRequest {
    Exec {
        command: String,
        reply: mpsc::SyncSender<Result<Observation>>,
    },
    CallTool {
        script_path: PathBuf,
        flags: String,
        reply: mpsc::SyncSender<Result<Observation>>,
    },
    RegisterPlugin {
        plugin_binary: PathBuf,
        reply: mpsc::SyncSender<Result<()>>,
    },
}

#[derive(Clone)]
pub struct ShellWorker {
    tx: mpsc::SyncSender<ShellRequest>,
}

impl ShellWorker {
    pub fn spawn(cwd: impl Into<String>, env: &std::collections::HashMap<String, String>) -> Result<Self> {
        let cwd = cwd.into();
        let (tx, rx) = mpsc::sync_channel::<ShellRequest>(32);

        let thread_cwd = cwd.clone();
        let thread_env = env.clone();
        thread::Builder::new()
            .name("nu-session".into())
            .spawn(move || session_thread(rx, &thread_cwd, &thread_env))
            .context("Spawning nu-session thread")?;

        info!(cwd, "ShellWorker started");
        Ok(Self { tx })
    }

    pub async fn exec(&self, command: impl Into<String>) -> Result<Observation> {
        let command = command.into();
        let tx = self.tx.clone();

        tokio::task::spawn_blocking(move || {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            tx.send(ShellRequest::Exec {
                command,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Shell worker thread has exited"))?;

            reply_rx
                .recv()
                .map_err(|_| anyhow!("Shell worker reply channel closed"))?
        })
        .await
        .context("spawn_blocking panicked")?
    }

    pub async fn call_tool(&self, script_path: &std::path::Path, flags: &str) -> Result<Observation> {
        let script_path = script_path.to_path_buf();
        let flags = flags.to_string();
        let tx = self.tx.clone();

        tokio::task::spawn_blocking(move || {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            tx.send(ShellRequest::CallTool {
                script_path,
                flags,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Shell worker thread has exited"))?;

            reply_rx
                .recv()
                .map_err(|_| anyhow!("Shell worker reply channel closed"))?
        })
        .await
        .context("spawn_blocking panicked")?
    }

    pub async fn register_mswea_plugin(&self, plugin_binary: &std::path::Path) -> Result<()> {
        let plugin_binary = plugin_binary.to_path_buf();
        let tx = self.tx.clone();

        tokio::task::spawn_blocking(move || {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            tx.send(ShellRequest::RegisterPlugin {
                plugin_binary,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Shell worker thread has exited"))?;

            reply_rx
                .recv()
                .map_err(|_| anyhow!("Shell worker reply channel closed"))?
        })
        .await
        .context("spawn_blocking panicked")?
    }
}

fn session_thread(rx: mpsc::Receiver<ShellRequest>, cwd: &str, env: &std::collections::HashMap<String, String>) {
    let mut session = match NushellSession::new(cwd, env) {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "Failed to create NushellSession — shell thread exiting");
            return;
        }
    };

    info!("Nu session thread ready");

    for req in rx {
        match req {
            ShellRequest::Exec { command, reply } => {
                debug!(command = %command, "Shell exec");
                let result = run_command(&mut session, &command);
                let _ = reply.send(result);
            }
            ShellRequest::CallTool { script_path, flags, reply } => {
                debug!(script = %script_path.display(), flags = %flags, "Tool call");
                let result = run_tool(&mut session, &script_path, &flags);
                let _ = reply.send(result);
            }
            ShellRequest::RegisterPlugin { plugin_binary, reply } => {
                let result = session.register_mswea_plugin(&plugin_binary);
                let _ = reply.send(result);
            }
        }
    }

    info!("Nu session thread exiting");
}

fn run_command(session: &mut NushellSession, command: &str) -> Result<Observation> {
    match session.eval(command) {
        Ok((value, exit_code)) => Ok(value_to_observation(value, exit_code)),
        Err(e) => {
            session.reset_stack();
            Ok(Observation::Error {
                message: e.to_string(),
                exit_code: Some(1),
                tool_call_summary: format!("shell: {}", truncate(command, 60)),
            })
        }
    }
}

fn run_tool(session: &mut NushellSession, script_path: &std::path::Path, flags: &str) -> Result<Observation> {
    match session.call_tool(script_path, flags) {
        Ok((value, exit_code)) => Ok(value_to_observation(value, exit_code)),
        Err(e) => {
            // Tool errors don't reset the stack — they're clean failures
            Ok(Observation::Error {
                message: e.to_string(),
                exit_code: Some(1),
                tool_call_summary: format!("tool: {}", truncate(&script_path.display().to_string(), 60)),
            })
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

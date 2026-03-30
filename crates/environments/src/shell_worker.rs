//! `ShellWorker` — dedicated thread owning the `NushellSession`.

use std::sync::mpsc;
use std::thread;

use anyhow::{anyhow, Context, Result};
use mswea_core::observation::Observation;
use tracing::{debug, error, info};

use crate::session::NushellSession;
use crate::value_to_observation::value_to_observation;

struct ShellRequest {
    command: String,
    reply: mpsc::SyncSender<Result<Observation>>,
}

#[derive(Clone)]
pub struct ShellWorker {
    tx: mpsc::SyncSender<ShellRequest>,
}

impl ShellWorker {
    pub fn spawn(cwd: impl Into<String>) -> Result<Self> {
        let cwd = cwd.into();
        let (tx, rx) = mpsc::sync_channel::<ShellRequest>(32);

        let thread_cwd = cwd.clone();
        thread::Builder::new()
            .name("nu-session".into())
            .spawn(move || session_thread(rx, &thread_cwd))
            .context("Spawning nu-session thread")?;

        info!(cwd, "ShellWorker started");
        Ok(Self { tx })
    }

    pub async fn exec(&self, command: impl Into<String>) -> Result<Observation> {
        let command = command.into();
        let tx = self.tx.clone();

        tokio::task::spawn_blocking(move || {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            tx.send(ShellRequest {
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
}

fn session_thread(rx: mpsc::Receiver<ShellRequest>, cwd: &str) {
    let mut session = match NushellSession::new(cwd) {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "Failed to create NushellSession — shell thread exiting");
            return;
        }
    };

    info!("Nu session thread ready");

    for req in rx {
        debug!(command = %req.command, "Shell exec");
        let result = run_command(&mut session, &req.command);
        let _ = req.reply.send(result);
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

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

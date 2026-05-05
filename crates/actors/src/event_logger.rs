//! `EventLoggerActor` — subscribes to the event bus and writes JSONL trajectory.
//!
//! # Setup (in wiring.rs)
//!
//! ```no_run
//! # use std::path::PathBuf;
//! # use std::sync::Arc;
//! # use ractor::Actor;
//! # use ractor::port::OutputPort;
//! # use actors::{EventLoggerActor, EventLoggerArgs};
//! # use mswea_core::event::Event;
//! # #[tokio::main] async fn main() -> anyhow::Result<()> {
//! # let bus = Arc::new(OutputPort::<Event>::default());
//! let (logger_ref, _) = Actor::spawn(None, EventLoggerActor, EventLoggerArgs {
//!     event_bus: bus.clone(),
//!     output_path: PathBuf::from("trajectory.jsonl"),
//! }).await?;
//! # Ok(()) }
//! ```
//!
//! In `pre_start`, the actor subscribes to the `OutputPort<Event>` bus.
//! Every `Event` is mapped to `EventLoggerMsg::Log(event)` and dispatched
//! through the actor's mailbox, then written to disk.
//!
//! # Format
//!
//! One JSON object per line. Readable with `jq`:
//! ```sh
//! cat trajectory.jsonl | jq '.kind | keys[0]'
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use ractor::port::OutputPort;
use ractor_cluster::RactorMessage;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use mswea_core::event::Event;

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, RactorMessage)]
pub enum EventLoggerMsg {
    /// An event arrived from the bus — write it to disk.
    Log(Event),
}

// ── Arguments ─────────────────────────────────────────────────────────────────

pub struct EventLoggerArgs {
    pub event_bus: Arc<OutputPort<Event>>,
    pub output_path: PathBuf,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct EventLoggerState {
    writer: tokio::io::BufWriter<tokio::fs::File>,
    count: u64,
}

// ── Actor ─────────────────────────────────────────────────────────────────────

pub struct EventLoggerActor;

impl Actor for EventLoggerActor {
    type Msg = EventLoggerMsg;
    type State = EventLoggerState;
    type Arguments = EventLoggerArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!(path = %args.output_path.display(), "EventLogger starting");

        // Subscribe to the event bus. Every Event is wrapped in Log(event)
        // and delivered to this actor's mailbox.
        args.event_bus.subscribe(myself, |event| {
            Some(EventLoggerMsg::Log(event))
        });

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&args.output_path)
            .await
            .with_context(|| format!("Opening {}", args.output_path.display()))
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

        // Ensure clean separation from any previous run's content
        file.write_all(b"\n").await.ok();

        Ok(EventLoggerState {
            writer: tokio::io::BufWriter::new(file),
            count: 0,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            EventLoggerMsg::Log(event) => {
                match serde_json::to_string(&event) {
                    Ok(line) => {
                        if let Err(e) = state.writer.write_all(line.as_bytes()).await {
                            warn!(error = %e, "EventLogger write error");
                            return Ok(());
                        }
                        let _ = state.writer.write_all(b"\n").await;
                        let _ = state.writer.flush().await;  // ← always flush
                        state.count += 1;
                    }
                    Err(e) => {
                        warn!(error = %e, "EventLogger serialization error");
                    }
                }
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let _ = state.writer.flush().await;
        info!(count = state.count, "EventLogger stopped — trajectory flushed");
        Ok(())
    }
}

//! `EventLoggerActor` — subscribes to the event bus and writes JSONL trajectory.
//!
//! # Setup (in wiring.rs)
//!
//! ```rust
//! let (logger_ref, _) = Actor::spawn(None, EventLoggerActor, EventLoggerArgs {
//!     event_bus: bus.clone(),
//!     output_path: PathBuf::from("trajectory.jsonl"),
//! }).await?;
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
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use mswea_core::event::Event;

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
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

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&args.output_path)
            .await
            .with_context(|| format!("Opening {}", args.output_path.display()))
            .map_err(|e| ActorProcessingErr::from(e.to_string()))?;

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
                        state.count += 1;
                        // Flush every 50 events.
                        if state.count % 50 == 0 {
                            let _ = state.writer.flush().await;
                        }
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

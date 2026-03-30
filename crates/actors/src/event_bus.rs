//! Event bus — `Arc<OutputPort<Event>>` shared across all actors.
//!
//! Any actor that emits events clones the `Arc` and calls `.send(event)`.
//! Subscribers register via `.subscribe(actor_ref, mapper)` in `pre_start`.
//!
//! This is the native ractor fan-out primitive — no tokio broadcast needed.

use std::sync::Arc;
use ractor::port::OutputPort;
use mswea_core::event::Event;

/// Shared event bus. Clone the `Arc` freely — it's cheap.
pub type EventBus = Arc<OutputPort<Event>>;

/// Construct a new event bus.
pub fn new_event_bus() -> EventBus {
    Arc::new(OutputPort::default())
}

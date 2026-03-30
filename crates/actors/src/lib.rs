//! `actors` — ractor-based actor system for mini-swe-agent.
//!
//! Actors:
//!   ✅ `EventLoggerActor`  — subscribes to event bus, writes JSONL trajectory
//!   ✅ `OrchestratorActor` — CapabilityMap, system prompt generation
//!   ✅ `ToolRouterActor`   — dispatches ToolCall to appropriate handler

pub mod event_bus;
pub mod event_logger;
pub mod orchestrator;
pub mod tool_router;

pub use event_bus::{new_event_bus, EventBus};
pub use event_logger::{EventLoggerActor, EventLoggerArgs, EventLoggerMsg};
pub use orchestrator::{
    register_builtins, OrchestratorActor, OrchestratorArgs, OrchestratorMsg,
};
pub use tool_router::{RouteRequest, ToolRouterActor, ToolRouterArgs};

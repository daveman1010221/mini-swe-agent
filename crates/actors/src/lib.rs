//! `actors` — ractor-based actor system for mini-swe-agent.
//!
//! Actors:
//!   ✅ `EventLoggerActor`  — subscribes to event bus, writes JSONL trajectory
//!   ✅ `OrchestratorActor` — CapabilityMap + system prompt via minijinja
//!   ✅ `ToolboxActor`      — tool registry, playbook registry, skills, preflight
//!   ✅ `ToolRouterActor`   — dispatches ToolCall to appropriate handler

pub mod event_bus;
pub mod event_logger;
pub mod orchestrator;
pub mod toolbox;
pub mod tool_router;

pub use event_bus::{new_event_bus, EventBus};
pub use event_logger::{EventLoggerActor, EventLoggerArgs, EventLoggerMsg};
pub use orchestrator::{
    register_builtins, OrchestratorActor, OrchestratorArgs, OrchestratorMsg,
};
pub use toolbox::{ToolboxActor, ToolboxArgs, ToolboxMsg};
pub use tool_router::{RouteRequest, ToolRouterActor, ToolRouterArgs};

pub mod policy_messages;
pub use policy_messages::{NormalizeRequest, ConstraintRequest, PolicyContextUpdate};

pub mod arg_normalizer;
pub use arg_normalizer::{ArgNormalizerActor, ArgNormalizerArgs};

pub mod constraint_checker;
pub use constraint_checker::{
    ConstraintCheckerActor, ConstraintCheckerArgs, ConstraintCheckerMsg,
};

//! `MsweaPlugin` — implements the `nu_plugin::Plugin` trait.
//!
//! Holds the ractor cluster context (ActorRefs) shared across all commands.
//! Commands are stateless structs; all state lives here and is accessed via
//! the `plugin` reference passed to each command's `run()` method.

use nu_plugin::{Plugin, PluginCommand};
use std::sync::Arc;
use ractor::ActorRef;
use actors::task_actor::TaskMsg;
use actors::constraint_checker::ConstraintCheckerMsg;

use crate::commands::{
    cargo::{MsweaCargoCheck, MsweaCargoTest},
    rpc::{
        MsweaRpcAdvance, MsweaRpcHalt, MsweaRpcRecordAttempt, MsweaRpcRecordOrient,
        MsweaRpcTaskState, MsweaRpcWriteCoveragePlan,
    },
};

/// The mswea plugin state, shared across all command invocations.
///
/// Holds ActorRefs to the ractor cluster nodes. Initialized once at plugin
/// startup and reused for the lifetime of the plugin process / session.
pub struct MsweaPlugin {
    /// Direct ActorRef to TaskActor on the mswea-core cluster node.
    /// None if the cluster connection has not yet been established.
    pub task_actor: Option<ActorRef<TaskMsg>>,
    /// Direct ActorRef to ConstraintCheckerActor on the mswea-core node.
    pub constraint_checker: Option<ActorRef<ConstraintCheckerMsg>>,
}

impl MsweaPlugin {
    pub fn new(
        task_actor: Option<ActorRef<TaskMsg>>,
        constraint_checker: Option<ActorRef<ConstraintCheckerMsg>>,
    ) -> Self {
        Self { task_actor, constraint_checker }
    }
}

impl Plugin for MsweaPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![
            // rpc family
            Box::new(MsweaRpcRecordOrient),
            Box::new(MsweaRpcRecordAttempt),
            Box::new(MsweaRpcTaskState),
            Box::new(MsweaRpcWriteCoveragePlan),
            Box::new(MsweaRpcHalt),
            Box::new(MsweaRpcAdvance),
            // cargo family
            Box::new(MsweaCargoCheck),
            Box::new(MsweaCargoTest),
        ]
    }
}

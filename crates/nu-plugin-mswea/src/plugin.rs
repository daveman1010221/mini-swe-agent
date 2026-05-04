//! `MsweaPlugin` — implements the `nu_plugin::Plugin` trait.
//!
//! Holds the ractor cluster context (ActorRefs) shared across all commands.
//! Commands are stateless structs; all state lives here and is accessed via
//! the `plugin` reference passed to each command's `run()` method.

use nu_plugin::{Plugin, PluginCommand};
use ractor::ActorRef;
use actors::task_actor::TaskMsg;
use actors::constraint_checker::ConstraintCheckerMsg;
use tokio::runtime::Handle;

use crate::commands::{
    cargo::{MsweaCargoCheck, MsweaCargoTest},
    rpc::{
        MsweaRpcAdvance, MsweaRpcHalt, MsweaRpcRecordAttempt, MsweaRpcRecordOrient,
        MsweaRpcTaskState, MsweaRpcWriteCoveragePlan,
    },
};


/// The mswea plugin state, shared across all command invocations.
///
/// Holds ActorRefs to the ractor cluster nodes and a Tokio runtime handle
/// for bridging sync plugin commands into async actor calls.
///
/// Initialized once at plugin startup and reused for the lifetime of the
/// plugin process / session.
pub struct MsweaPlugin {
    /// Direct ActorRef to TaskActor on the mswea-core cluster node.
    /// None if the cluster connection has not been established.
    pub task_actor: Option<ActorRef<TaskMsg>>,
    /// Direct ActorRef to ConstraintCheckerActor on the mswea-core node.
    pub constraint_checker: Option<ActorRef<ConstraintCheckerMsg>>,
    /// Tokio runtime handle for bridging sync plugin commands to async actor calls.
    pub rt: Handle,
}

impl MsweaPlugin {
    pub fn new(
        task_actor: Option<ActorRef<TaskMsg>>,
        constraint_checker: Option<ActorRef<ConstraintCheckerMsg>>,
        rt: Handle,
    ) -> Self {
        Self { task_actor, constraint_checker, rt }
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

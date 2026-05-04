//! `MsweaPlugin` — implements the `nu_plugin::Plugin` trait.
//!
//! Holds the ractor cluster context (ActorRefs) shared across all commands.
//! Commands are stateless structs; all state lives here and is accessed via
//! the `plugin` reference passed to each command's `run()` method.

use nu_plugin::{Plugin, PluginCommand};

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
    // TODO: ActorRef<TaskMsg>
    // TODO: ActorRef<ConstraintCheckerMsg>
    // TODO: workspace_root: PathBuf
}

impl MsweaPlugin {
    pub fn new() -> Self {
        Self {}
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

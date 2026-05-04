//! `nu-plugin-mswea` — nushell plugin providing the `mswea` command family.
//!
//! # Architecture
//!
//! This crate serves two roles:
//!
//! ## As a library (used by `crates/cli`)
//! Registers mswea commands into the embedded nushell EngineState at session
//! startup. Commands hold ActorRefs to the ractor cluster nodes for TaskActor
//! and ConstraintCheckerActor, resolved at registration time.
//!
//! ## As a binary (`nu_plugin_mswea`)
//! Spawned by nushell as an external plugin process. Joins the mswea ractor
//! cluster, resolves the same ActorRefs over the cluster, and serves the
//! nushell plugin protocol via local socket.
//!
//! # The Physical Enforcement Barrier
//!
//! Every tool call from a nushell script MUST go through a `mswea` command.
//! The `mswea` commands are the only approved path to:
//!   - Task state mutations (rpc record-orient, rpc advance, etc.)
//!   - Cargo invocations (cargo check, cargo test)
//!   - Policy-gated external tool execution
//!
//! Because the plugin process IS the only door, the agent cannot circumvent
//! policy by finding an alternate execution path. This is a physical barrier,
//! not a policy list.
//!
//! # Command Family
//!
//! ```text
//! mswea rpc record-orient   — record an orient step observation
//! mswea rpc record-attempt  — record a step attempt
//! mswea rpc task-state      — query current task state
//! mswea rpc write-coverage-plan — record coverage plan
//! mswea rpc halt            — halt the current task
//! mswea rpc advance         — advance to next playbook step
//! mswea cargo check         — run cargo check, return structured diagnostics
//! mswea cargo test          — run cargo test, return structured results
//! ```

pub mod commands;
pub mod plugin;

pub use plugin::MsweaPlugin;

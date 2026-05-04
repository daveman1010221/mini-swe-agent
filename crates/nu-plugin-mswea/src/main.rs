//! `nu_plugin_mswea` — external plugin binary.
//!
//! When nushell spawns this binary, it:
//!   1. Connects to the mswea ractor cluster node as "mswea-plugin"
//!   2. Resolves ActorRefs to TaskActor and ConstraintCheckerActor
//!   3. Serves the nushell plugin protocol (local socket or stdio)
//!
//! The cluster node address is passed via $env.MSWEA_CLUSTER_ADDR at spawn time.

fn main() {
    // TODO: initialize ractor cluster node, serve_plugin
    eprintln!("nu_plugin_mswea: not yet implemented");
    std::process::exit(1);
}

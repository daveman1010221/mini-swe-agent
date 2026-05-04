//! `nu_plugin_mswea` binary entry point.
//!
//! Spawned by nushell as an external plugin process. Joins the mswea ractor
//! cluster as "mswea-plugin", resolves ActorRefs to TaskActor and
//! ConstraintCheckerActor, then serves the nushell plugin protocol.
//!
//! Environment variables (injected by wiring.rs at session startup):
//!   MSWEA_CLUSTER_ADDR   — host:port of the mswea-core cluster node
//!   MSWEA_CLUSTER_COOKIE — shared authentication cookie

use nu_plugin::{MsgPackSerializer, serve_plugin};
use ractor::{Actor, ActorRef};
use ractor_cluster::{NodeServer, client_connect, node::NodeConnectionMode};
use std::net::{IpAddr, Ipv4Addr};
use tokio::time::{Duration, sleep};

use nu_plugin_mswea::MsweaPlugin;
use mswea_core::task::TaskMsg;
use mswea_core::policy::ConstraintCheckerMsg;

fn main() {
    // Read cluster connection details from environment
    let cluster_addr = std::env::var("MSWEA_CLUSTER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9000".to_string());
    let cluster_cookie = std::env::var("MSWEA_CLUSTER_COOKIE")
        .unwrap_or_else(|_| "mswea-cluster-cookie".to_string());

    // Build a Tokio runtime for cluster operations.
    // serve_plugin() takes over the main thread after this.
    let rt = tokio::runtime::Runtime::new()
        .expect("Failed to create Tokio runtime");

    let rt_handle = rt.handle().clone();

    let plugin = rt.block_on(async {
        // Start our node server on an ephemeral port (0 = OS picks)
        let (node_server, _handle) = Actor::spawn(
            Some("mswea-plugin-node".into()),
            NodeServer::new(
                0,
                cluster_cookie,
                "mswea-plugin".to_string(),
                "localhost".to_string(),
                None,
                Some(NodeConnectionMode::Isolated),
            ).with_listen_addr(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            (),
        )
        .await
        .expect("Failed to spawn plugin NodeServer");

        // Connect to the mswea-core node
        client_connect(&node_server, &cluster_addr)
            .await
            .expect("Failed to connect to mswea-core cluster node");

        // Give the cluster handshake time to complete and actors to sync
        sleep(Duration::from_millis(500)).await;

        // Resolve remote ActorRefs by name from the cluster registry
        let task_actor: Option<ActorRef<TaskMsg>> =
            ActorRef::where_is("task-actor".to_string());
        let constraint_checker: Option<ActorRef<ConstraintCheckerMsg>> =
            ActorRef::where_is("constraint-checker".to_string());

        MsweaPlugin::new(task_actor, constraint_checker, rt_handle)
    });

    // serve_plugin takes over — handles the nushell plugin protocol
    // on stdin/stdout or local socket, depending on how nushell spawned us.
    serve_plugin(&plugin, MsgPackSerializer);
}

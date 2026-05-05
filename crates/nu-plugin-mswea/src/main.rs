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
use ractor_cluster::{NodeServer, client_connect, node::{NodeConnectionMode, NodeServerSessionInformation}};
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

    let rt = tokio::runtime::Runtime::new()
        .expect("Failed to create Tokio runtime");

    let rt_handle = rt.handle().clone();

    let plugin = rt.block_on(async {
        use std::sync::Arc;
        use tokio::sync::Notify;

        // Start our node server on an ephemeral port (0 = OS picks)
        let (node_server, _handle) = Actor::spawn(
            Some("mswea-plugin-node".into()),
            NodeServer::new(
                0,
                cluster_cookie,
                format!("mswea-plugin-{}", std::process::id()),
                "localhost".to_string(),
                None,
                Some(NodeConnectionMode::Isolated),
            ).with_listen_addr(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            (),
        )
        .await
        .expect("Failed to spawn plugin NodeServer");

        // Subscribe to node events so we know when Ready handshake completes
        let notify = Arc::new(Notify::new());
        let notify_clone = Arc::clone(&notify);

        struct ReadyNotifier(Arc<Notify>);
        impl ractor_cluster::NodeEventSubscription for ReadyNotifier {
            fn node_session_opened(&self, _: NodeServerSessionInformation) {}
            fn node_session_disconnected(&self, _: NodeServerSessionInformation) {}
            fn node_session_authenicated(&self, _: NodeServerSessionInformation) {}
            fn node_session_ready(&self, _: NodeServerSessionInformation) {
                self.0.notify_one();
            }
        }

        node_server.cast(ractor_cluster::NodeServerMessage::SubscribeToEvents {
            id: "ready-waiter".to_string(),
            subscription: Box::new(ReadyNotifier(notify_clone)),
        }).expect("Failed to subscribe to node events");

        // Connect to the mswea-core node
        client_connect(&node_server, &cluster_addr)
            .await
            .expect("Failed to connect to mswea-core cluster node");

        // Wait for Ready handshake to complete (with timeout)
        tokio::time::timeout(
            Duration::from_secs(10),
            notify.notified(),
        )
        .await
        .expect("Cluster Ready handshake timed out after 10s");

        // Wait for PG group to be synced from mswea-core
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let members = ractor::pg::get_members(&"mswea-task-actors".to_string());
            if !members.is_empty() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("mswea-task-actors PG group not visible after 10s");
            }
            sleep(Duration::from_millis(50)).await;
        }

        let task_actor: Option<ActorRef<TaskMsg>> = ractor::pg::get_members(&"mswea-task-actors".to_string())
            .into_iter()
            .next()
            .map(|cell| cell.into());

        let constraint_checker: Option<ActorRef<ConstraintCheckerMsg>> = ractor::pg::get_members(&"mswea-constraint-checkers".to_string())
            .into_iter()
            .next()
            .map(|cell| cell.into());

        MsweaPlugin::new(task_actor, constraint_checker, rt_handle)
    });

    // serve_plugin takes over — handles the nushell plugin protocol
    serve_plugin(&plugin, MsgPackSerializer);
}

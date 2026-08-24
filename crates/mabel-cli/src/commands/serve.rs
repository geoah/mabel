//! `mabel serve`: serve this home until ctrl-c (proposal 006 section 8).
//!
//! One command for every node. What this node can do is read from what it
//! holds: the identities in the home are what it signs for, and
//! `node.json.witness_for` is who it accepts strangers' pushes on behalf of.
//! `wallet serve` and `witness run` are hidden aliases of this command and
//! print the same document.
//!
//! The command runs the node runtime of `mabel-node` on its own tokio runtime,
//! since every other command in this binary is synchronous. The endpoint id and
//! both bound addresses go to stderr as soon as they are known, so a caller
//! reading `--json` on stdout gets one document and nothing else.

use std::net::SocketAddr;
use std::path::PathBuf;

use mabel_node::api::UiSource;
use mabel_node::api::documents::Id;
use mabel_node::{NodeOptions, NodeRuntime};
use serde::Serialize;

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::parse_peers;
use crate::render::Outcome;

/// What `mabel serve --json` prints when the node stops.
#[derive(Debug, Serialize)]
pub struct ServedNode {
    /// This node's Iroh endpoint id, which a peer dials to reach it.
    pub endpoint_id: Id,
    /// Where the HTTP API listened.
    pub http_bind: SocketAddr,
    /// The UDP sockets the Iroh endpoint bound.
    pub iroh_bind: Vec<SocketAddr>,
    /// Identities this home holds, which is what it can sign for.
    pub identity_count: u64,
    /// Ledgers it holds.
    pub ledger_count: u64,
    /// Fork records it holds.
    pub fork_count: u64,
    /// The witness identities it witnesses for, empty when it witnesses for
    /// nobody.
    pub witness_for: Vec<Id>,
}

/// `mabel serve [--http <addr>] [--iroh-port <n>] [--peer <ticket>]
/// [--ui-dir <dir>] [--allow-host <host[:port]>]`.
///
/// `--ui-dir` serves the UI from a directory instead of the bundle compiled
/// into the binary, which is what a person editing the UI wants.
///
/// `--allow-host` widens the `Host` and `Origin` sets the loopback middleware
/// accepts, adding to whatever `node.json`'s `allowed_hosts` records. Without
/// it the API answers loopback alone (decision 018).
///
/// # Errors
///
/// Returns code 60 for an insecure `node.key`, code 2 for a `--peer` value
/// that is not an endpoint ticket, and code 30 when a listener cannot bind.
pub fn serve(
    ctx: &Context,
    http: Option<SocketAddr>,
    iroh_port: Option<u16>,
    tickets: &[String],
    ui_dir: Option<PathBuf>,
    allowed_hosts: &[String],
) -> Result<Outcome> {
    let peers = parse_peers(tickets)?;
    let allowed_hosts = allowed_hosts.to_vec();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::internal("runtime_unavailable", error.to_string()))?;

    runtime.block_on(async move {
        let node = NodeRuntime::start(
            ctx.home().clone(),
            NodeOptions {
                http_bind: http,
                iroh_port,
                peers,
                ui: UiSource::from_option(ui_dir),
                allowed_hosts,
                ..NodeOptions::default()
            },
        )
        .await
        .map_err(failed)?;

        let totals = node.storage().totals();
        let served = ServedNode {
            endpoint_id: ids::key(&node.endpoint_id()),
            http_bind: node.http_address(),
            iroh_bind: node.iroh_addresses().to_vec(),
            identity_count: node.core().identities().map_or(0, |held| held.len() as u64),
            ledger_count: totals.ledger_count,
            fork_count: totals.fork_count,
            witness_for: node
                .storage()
                .witness_for()
                .iter()
                .map(|identity| ids::identity(*identity))
                .collect(),
        };
        announce(&served, &node);
        node.serve().await.map_err(failed)?;
        Outcome::new(&served, String::new())
    })
}

/// The lines a person watching the process reads.
fn announce(served: &ServedNode, node: &NodeRuntime) {
    eprintln!("node   {}", served.endpoint_id);
    eprintln!("http   {}", served.http_bind);
    for address in &served.iroh_bind {
        eprintln!("iroh   {address}");
    }
    eprintln!(
        "holding {} identities, {} ledgers and {} fork records",
        served.identity_count, served.ledger_count, served.fork_count
    );
    for witness in &served.witness_for {
        eprintln!("witnessing for {witness}");
    }
    for host in node.allowed_hosts() {
        eprintln!("host   {host} is accepted beyond loopback");
    }
    if let Some(notice) = node.role_notice() {
        eprintln!("notice: {notice}");
    }
    if let Some(warning) = node.warning() {
        eprintln!("warning: {warning}");
    }
}

/// A failure from the runtime, keeping the exit code of a storage error.
fn failed(error: anyhow::Error) -> CliError {
    match error.downcast::<mabel_node::StorageError>() {
        Ok(storage) => CliError::from(storage),
        Err(error) => CliError::network("node_unavailable", error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::ServedNode;

    /// The shutdown document is frozen by `contracts/cli/serve.json`, whose one
    /// case covers `mabel serve` and its two hidden aliases: one command, one
    /// case (proposal 006 section 8).
    #[test]
    fn the_shutdown_document_carries_the_keys_the_fixture_freezes() {
        const SERVE: &str = include_str!("../../../../contracts/cli/serve.json");
        let fixture: serde_json::Value = serde_json::from_str(SERVE).expect("valid JSON");
        let cases = fixture["cases"].as_array().expect("cases");
        assert_eq!(cases.len(), 1, "one command, one case");
        assert_eq!(cases[0]["case"], serde_json::json!("served-until-ctrl-c"));

        let served = ServedNode {
            endpoint_id: crate::ids::key(&iroh_base::SecretKey::from_bytes(&[7u8; 32]).public()),
            http_bind: "127.0.0.1:9080".parse().expect("an address"),
            iroh_bind: vec!["0.0.0.0:9071".parse().expect("an address")],
            identity_count: 2,
            ledger_count: 4,
            fork_count: 1,
            witness_for: Vec::new(),
        };
        let mut rendered = serde_json::to_value(&served).expect("the document serializes");
        rendered
            .as_object_mut()
            .expect("an object")
            .insert("ok".to_owned(), serde_json::json!(true));

        let mut expected: Vec<&String> = cases[0]["document"]
            .as_object()
            .expect("a document")
            .keys()
            .collect();
        let mut actual: Vec<&String> = rendered.as_object().expect("an object").keys().collect();
        expected.sort();
        actual.sort();
        assert_eq!(actual, expected, "the fixture and the document disagree");
    }
}

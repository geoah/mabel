//! `mabel witness run`: serve this home as a witness until ctrl-c.
//!
//! The command runs the witness runtime of `mabel-node` on its own tokio
//! runtime, since every other command in this binary is synchronous. The
//! endpoint id and both bound addresses go to stderr as soon as they are
//! known, so a caller reading `--json` on stdout gets one document and nothing
//! else; `contracts/` freezes no shape for this command.

use std::net::SocketAddr;

use iroh_base::EndpointAddr;
use mabel_node::api::documents::Id;
use mabel_node::witness::{WitnessOptions, WitnessRuntime};
use serde::Serialize;

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::render::Outcome;

/// What `mabel witness run --json` prints when the witness stops.
#[derive(Debug, Serialize)]
pub struct ServedWitness {
    /// This node's Iroh endpoint id, which a wallet names in a witness set.
    pub endpoint_id: Id,
    /// Where the HTTP API listened.
    pub http_bind: SocketAddr,
    /// The UDP sockets the Iroh endpoint bound.
    pub iroh_bind: Vec<SocketAddr>,
    /// Ledgers this witness holds.
    pub ledger_count: u64,
    /// Fork records it holds.
    pub fork_count: u64,
}

/// `mabel witness run [--http <addr>] [--iroh-port <n>] [--peer <ticket>]`.
///
/// # Errors
///
/// Returns code 60 for an insecure `node.key`, code 2 for a `--peer` value
/// that is not an endpoint ticket, and code 30 when a listener cannot bind.
pub fn run(
    ctx: &Context,
    http: Option<SocketAddr>,
    iroh_port: Option<u16>,
    tickets: &[String],
) -> Result<Outcome> {
    let peers = parse_peers(tickets)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::internal("runtime_unavailable", error.to_string()))?;

    runtime.block_on(async move {
        let witness = WitnessRuntime::start(
            ctx.home().clone(),
            WitnessOptions {
                http_bind: http,
                iroh_port,
                peers,
                ..WitnessOptions::default()
            },
        )
        .await
        .map_err(failed)?;

        let totals = witness.storage().totals();
        let served = ServedWitness {
            endpoint_id: ids::key(&witness.endpoint_id()),
            http_bind: witness.http_address(),
            iroh_bind: witness.iroh_addresses().to_vec(),
            ledger_count: totals.ledger_count,
            fork_count: totals.fork_count,
        };
        announce(&served, witness.warning());
        witness.serve().await.map_err(failed)?;
        Outcome::new(&served, String::new())
    })
}

/// Reads the `--peer` tickets, which are address hints and never authorization
/// (proposal 001 section 4).
fn parse_peers(tickets: &[String]) -> Result<Vec<EndpointAddr>> {
    tickets
        .iter()
        .map(|ticket| {
            mabel_net::parse_peer_ticket(ticket).map_err(|error| {
                CliError::usage(
                    "malformed_peer_ticket",
                    format!("{ticket} is not an endpoint ticket: {error}"),
                )
                .with_detail("value", ticket)
            })
        })
        .collect()
}

/// The lines a person watching the process reads.
fn announce(served: &ServedWitness, warning: Option<&str>) {
    eprintln!("witness {}", served.endpoint_id);
    eprintln!("http    {}", served.http_bind);
    for address in &served.iroh_bind {
        eprintln!("iroh    {address}");
    }
    eprintln!(
        "holding {} ledgers and {} fork records",
        served.ledger_count, served.fork_count
    );
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
}

/// A failure from the runtime, keeping the exit code of a storage error.
fn failed(error: anyhow::Error) -> CliError {
    match error.downcast::<mabel_node::StorageError>() {
        Ok(storage) => CliError::from(storage),
        Err(error) => CliError::network("witness_unavailable", error.to_string()),
    }
}

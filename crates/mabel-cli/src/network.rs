//! The Iroh endpoint a network command dials from.
//!
//! Every other command in this binary is synchronous, so a command that needs
//! a peer builds a tokio runtime, binds one endpoint from `node.key`, runs its
//! body and closes the endpoint again. Dialling names an `EndpointId`; a
//! `--peer` ticket is seeded into the address lookup first and is an address
//! hint, never authorization (proposal 001 section 4).

use std::future::Future;

use iroh_base::EndpointAddr;
use mabel_node::wallet::{WalletCore, WalletSync};

use crate::context::Context;
use crate::error::{CliError, Result};

/// Binds an endpoint, runs `body` on it and closes it again.
///
/// # Errors
///
/// Returns code 2 for a `--peer` value that is not an endpoint ticket, code 60
/// for an insecure `node.key`, code 30 when the endpoint cannot bind, and
/// whatever `body` reports.
pub fn on_network<T, F, Fut>(ctx: &Context, tickets: &[String], body: F) -> Result<T>
where
    F: FnOnce(WalletCore, WalletSync) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let peers = parse_peers(tickets)?;
    let secret = ctx.home().node_key()?;
    let relay = ctx.home().config()?.relay;
    let core = WalletCore::new(ctx.home().clone());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::internal("runtime_unavailable", error.to_string()))?;

    runtime.block_on(async move {
        let endpoint = mabel_node::bind_endpoint(relay, secret, None, &peers)
            .await
            .map_err(|error| CliError::network("endpoint_unavailable", error.to_string()))?;
        let result = body(core, WalletSync::new(endpoint.clone())).await;
        endpoint.close().await;
        result
    })
}

/// Reads the `--peer` tickets.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_peer_ticket`.
pub fn parse_peers(tickets: &[String]) -> Result<Vec<EndpointAddr>> {
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

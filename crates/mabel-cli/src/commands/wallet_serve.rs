//! `mabel wallet serve`: serve this home as a wallet until ctrl-c.
//!
//! The command runs the wallet runtime of `mabel-node` on its own tokio
//! runtime, since every other command in this binary is synchronous. The
//! endpoint id and both bound addresses go to stderr as soon as they are
//! known, so a caller reading `--json` on stdout gets one document and nothing
//! else; `contracts/` freezes no shape for this command.

use std::net::SocketAddr;
use std::path::PathBuf;

use mabel_node::api::UiSource;
use mabel_node::api::documents::Id;
use mabel_node::wallet::{WalletOptions, WalletRuntime};
use serde::Serialize;

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::parse_peers;
use crate::render::Outcome;

/// What `mabel wallet serve --json` prints when the wallet stops.
#[derive(Debug, Serialize)]
pub struct ServedWallet {
    /// This node's Iroh endpoint id, which a peer dials to fetch its ledgers.
    pub endpoint_id: Id,
    /// Where the HTTP API listened.
    pub http_bind: SocketAddr,
    /// The UDP sockets the Iroh endpoint bound.
    pub iroh_bind: Vec<SocketAddr>,
    /// Identities this home holds.
    pub identity_count: u64,
}

/// `mabel wallet serve [--http <addr>] [--iroh-port <n>] [--peer <ticket>]
/// [--ui-dir <dir>]`.
///
/// `--ui-dir` serves the UI from a directory instead of the bundle compiled
/// into the binary, which is what a person editing the UI wants.
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
) -> Result<Outcome> {
    let peers = parse_peers(tickets)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::internal("runtime_unavailable", error.to_string()))?;

    runtime.block_on(async move {
        let wallet = WalletRuntime::start(
            ctx.home().clone(),
            WalletOptions {
                http_bind: http,
                iroh_port,
                peers,
                ui: UiSource::from_option(ui_dir),
            },
        )
        .await
        .map_err(failed)?;

        let served = ServedWallet {
            endpoint_id: ids::key(&wallet.endpoint_id()),
            http_bind: wallet.http_address(),
            iroh_bind: wallet.iroh_addresses().to_vec(),
            identity_count: wallet
                .core()
                .identities()
                .map_or(0, |held| held.len() as u64),
        };
        announce(&served, wallet.warning());
        wallet.serve().await.map_err(failed)?;
        Outcome::new(&served, String::new())
    })
}

/// The lines a person watching the process reads.
fn announce(served: &ServedWallet, warning: Option<&str>) {
    eprintln!("wallet {}", served.endpoint_id);
    eprintln!("http   {}", served.http_bind);
    for address in &served.iroh_bind {
        eprintln!("iroh   {address}");
    }
    eprintln!("holding {} identities", served.identity_count);
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
}

/// A failure from the runtime, keeping the exit code of a storage error.
fn failed(error: anyhow::Error) -> CliError {
    match error.downcast::<mabel_node::StorageError>() {
        Ok(storage) => CliError::from(storage),
        Err(error) => CliError::network("wallet_unavailable", error.to_string()),
    }
}

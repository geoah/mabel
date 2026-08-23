//! The wallet runtime: hold keys, append, push, fetch and verify (proposal
//! 001 sections 2, 5 and 3.7, ticket 011).
//!
//! A wallet is the node that signs. It holds the private keys of the
//! identities in its home, appends to the ledgers it controls, pushes them to
//! the witnesses those ledgers name, fetches other ledgers to answer a
//! verification, and serves its own ledgers read-only so a verifier can fetch
//! them back.
//!
//! Five pieces sit over one home:
//!
//! - [`WalletCore`] reads the home and owns every append rule. No network.
//! - [`WalletSync`] dials peers: push, fetch, and the append discipline that
//!   asks a ledger's witnesses where it ends before signing on top of it.
//! - [`Verifier`] compares candidates from several sources and renders the two
//!   verification reports.
//! - [`WalletReadStore`] is the read-only [`mabel_net::Store`] peers fetch
//!   from.
//! - [`WalletApiService`] is the HTTP surface of section 10.
//!
//! Two rules run through all of it. No source is trusted: every chain a peer
//! serves is folded from nothing and its ledger id is required to equal the
//! one that was asked for, before a byte of it is stored. And every answer
//! names where it came from: source, head sequence, head event and fetch time
//! (flag R, proposal 001 section 6).
//!
//! ```no_run
//! use mabel_node::wallet::{WalletOptions, WalletRuntime};
//! use mabel_node::{HomeOptions, NodeHome};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let home = NodeHome::open("/tmp/wallet", HomeOptions::default())?;
//! let wallet = WalletRuntime::start(home, WalletOptions::default()).await?;
//! println!("{} on {}", wallet.endpoint_id(), wallet.http_address());
//! wallet.serve().await
//! # }
//! ```

mod core;
mod error;
pub mod ids;
mod ledger;
mod runtime;
mod service;
mod store;
mod sync;
mod verify;

pub use core::{AppendedEvent, WalletCore, unknown_ledger};
pub use error::{
    artifact_error, build_error, equivocation, fold_error, no_source_available, peer_message,
    stale_head, storage_error, unreachable,
};
pub use ledger::LoadedLedger;
pub use runtime::{WalletOptions, WalletRuntime};
pub use service::WalletApiService;
pub use store::WalletReadStore;
pub use sync::{Fetched, Freshness, REQUEST_TIMEOUT, WalletSync};
pub use verify::{
    Candidate, Sources, Verified, Verifier, as_of, ledger_report, revocation_clause, sources,
    trust_report,
};

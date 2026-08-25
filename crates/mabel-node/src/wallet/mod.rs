//! Holding keys, appending, pushing, fetching and verifying (proposal 001
//! sections 2, 5 and 3.7, ticket 011).
//!
//! This is what a node does with the identities it holds the keys of: it
//! appends to the ledgers it controls, pushes them to the witnesses those
//! ledgers name, and fetches other ledgers to answer a verification. Every
//! node runs it, over the one store and the one runtime of proposal 006
//! section 8; a home holding no key simply has nothing to append to.
//!
//! Four pieces sit over one home:
//!
//! - [`WalletCore`] reads the home and owns every append rule. No network.
//! - [`WalletSync`] dials peers: push, fetch, and the append discipline that
//!   asks a ledger's witnesses where it ends before signing on top of it.
//! - [`Verifier`] compares candidates from several sources and renders the two
//!   verification reports.
//! - [`NodeApiService`] is the HTTP surface of section 10, the one every node
//!   serves.
//!
//! Two rules run through all of it. No source is trusted: every chain a peer
//! serves is folded from nothing and its ledger id is required to equal the
//! one that was asked for, before a byte of it is stored. And every answer
//! names where it came from: source, head sequence, head event and fetch time
//! (flag R, proposal 001 section 6).

pub(crate) mod core;
mod error;
pub mod ids;
mod ledger;
mod lookup;
pub(crate) mod service;
pub(crate) mod sync;
mod verify;

pub use core::{
    AppendedEvent, WalletCore, contact_document, no_local_signer, unknown_ledger,
    verification_document,
};
pub use error::{
    artifact_error, build_error, equivocation, fold_error, fold_error_at, fold_message,
    no_source_available, peer_message, stale_head, storage_error, unreachable,
};
pub use ledger::LoadedLedger;
pub use lookup::{KnownPage, Names, default_root, graph_status, known_identities, lookup_document};
pub use service::NodeApiService;
pub use sync::{Fetched, Freshness, REQUEST_TIMEOUT, WalletSync};
pub use verify::{
    Candidate, Sources, Verified, Verifier, as_of, ledger_report, revocation_clause, sources,
    trust_report,
};

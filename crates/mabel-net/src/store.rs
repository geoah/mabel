//! The trait the server answers requests from.
//!
//! A witness (ticket 010) and a wallet serving reads implement [`Store`]; the
//! server holds one behind an `Arc<dyn Store>` and never looks inside it.
//! Every method is async and returns a boxed future, so the trait stays
//! dyn-compatible; an implementation backed by blocking IO wraps its body in
//! `tokio::task::spawn_blocking`.
//!
//! Events cross this boundary as encoded `SignedEvent` bytes, never as
//! decoded structs: the bytes are what was signed and what is authoritative
//! (proposal 001 section 3.1).

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use iroh_base::EndpointId;
use mabel_core::proto::{DeclaredKind, RejectCode};
use mabel_core::{EventId, LedgerId};

use crate::error::Rejection;

/// The future a [`Store`] method returns.
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send + 'a>>;

/// Who sent a request.
///
/// The peer's `EndpointId` is authenticated by the QUIC handshake and is
/// recorded as provenance only: nothing in mabel authorizes on it (proposal
/// 001 section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Provenance {
    /// The peer's endpoint id, absent when the request did not arrive over a
    /// connection.
    pub endpoint: Option<EndpointId>,
}

impl Provenance {
    /// Provenance for a request that arrived from `endpoint`.
    pub fn from_endpoint(endpoint: EndpointId) -> Self {
        Self {
            endpoint: Some(endpoint),
        }
    }
}

/// Where a ledger currently ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    /// The sequence of the last stored event.
    pub head_seq: u64,
    /// The event id of the last stored event.
    pub head_event: EventId,
    /// When the store last accepted an event for this ledger.
    pub updated_ms: u64,
}

/// One page of a ledger's events.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventPage {
    /// Encoded `SignedEvent` bytes, contiguous and ascending from the
    /// requested `since`.
    pub events: Vec<Vec<u8>>,
    /// The ledger's head sequence at the time of the read.
    pub head_seq: u64,
    /// Whether events past this page exist.
    pub more: bool,
}

/// What a `Push` stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PushOutcome {
    /// The ledger's head sequence after the push.
    pub head_seq: u64,
    /// How many events this push newly stored.
    pub stored: u32,
}

/// One page of entries plus whether more follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The entries, in the order the protocol defines.
    pub items: Vec<T>,
    /// Whether entries past this page exist.
    pub more: bool,
}

impl<T> Default for Page<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            more: false,
        }
    }
}

/// What a `List` reports about one ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSummary {
    /// The ledger id.
    pub ledger: LedgerId,
    /// What the ledger's inception says it is. Advisory (proposal 002
    /// section 3).
    pub declared_kind: DeclaredKind,
    /// The sequence of the last stored event.
    pub head_seq: u64,
    /// The event id of the last stored event.
    pub head_event: EventId,
    /// How many events are stored.
    pub event_count: u64,
    /// When the store first saw the ledger.
    pub first_seen_ms: u64,
    /// When the store last accepted an event for it.
    pub updated_ms: u64,
    /// How many fork records are recorded for it.
    pub fork_count: u32,
    /// Whether fork recording stopped at the per-ledger cap.
    pub forks_truncated: bool,
}

/// Two validly signed events at one sequence of one ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkRecord {
    /// The ledger both events claim.
    pub ledger: LedgerId,
    /// The sequence they collide at.
    pub seq: u64,
    /// The event the store saw first and kept, encoded.
    pub kept: Vec<u8>,
    /// The conflicting event, encoded.
    pub conflicting: Vec<u8>,
    /// When the store observed the conflict.
    pub observed_ms: u64,
    /// The endpoint the conflicting event arrived from, provenance only.
    pub source_endpoint: Option<EndpointId>,
}

/// Why a store did not answer a request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The store refuses the request, with a code the peer sees verbatim.
    ///
    /// Typically `INVALID`, `FORK`, `NOT_ADMITTED`, plus `MALFORMED` for a
    /// gapped push and `TOO_LARGE` for a store-side cap; the remaining
    /// transport codes are this crate's to produce.
    #[error("{0}")]
    Rejected(Rejection),
    /// The store failed for a reason the peer cannot fix. The server logs it
    /// and answers `BUSY`, because the protocol has no internal-error code.
    #[error("the store is unavailable: {0}")]
    Unavailable(String),
}

impl StoreError {
    /// The pushed events did not verify.
    pub fn invalid(at_seq: u64, msg: impl Into<String>) -> Self {
        Self::Rejected(Rejection::at(RejectCode::Invalid, at_seq, msg))
    }

    /// A pushed event collides with a stored one at the same sequence.
    pub fn fork(at_seq: u64, msg: impl Into<String>) -> Self {
        Self::Rejected(Rejection::at(RejectCode::Fork, at_seq, msg))
    }

    /// The store does not accept pushes for this ledger.
    pub fn not_admitted(msg: impl Into<String>) -> Self {
        Self::Rejected(Rejection::new(RejectCode::NotAdmitted, msg))
    }
}

/// The ledger storage the sync server serves.
///
/// Counts reaching the store are already clamped: `limit` never exceeds
/// [`crate::MAX_GET_LIMIT`], [`crate::MAX_LIST_LIMIT`] or
/// [`crate::MAX_FORKS_LIMIT`] for the matching method, and every event handed
/// to [`Store::push`] has passed the field table and the per-event size cap.
pub trait Store: fmt::Debug + Send + Sync + 'static {
    /// Where `ledger` ends, or `None` if the store does not hold it.
    fn head(&self, ledger: LedgerId) -> StoreFuture<'_, Option<Head>>;

    /// Events from `since` inclusive, at most `limit` of them, or `None` if
    /// the store does not hold `ledger`.
    fn read_from(
        &self,
        ledger: LedgerId,
        since: u64,
        limit: usize,
    ) -> StoreFuture<'_, Option<EventPage>>;

    /// Offers `events` for `ledger`.
    ///
    /// `provenance` records who sent them and must not decide whether they
    /// are accepted (proposal 001 section 4).
    fn push(
        &self,
        ledger: LedgerId,
        events: Vec<Vec<u8>>,
        provenance: Provenance,
    ) -> StoreFuture<'_, PushOutcome>;

    /// The enumerable ledgers by ascending ledger id, so paging is stable.
    ///
    /// This is the set the store is willing to be known to hold, not everything
    /// it stores: on a node that is the ledgers it signs for plus the ones it
    /// keeps as a witness (proposal 006 section 8). A ledger the store holds and
    /// does not enumerate is still served by [`Store::head`] and
    /// [`Store::read_from`] to a caller that can already name its id.
    fn list(&self, offset: usize, limit: usize) -> StoreFuture<'_, Page<LedgerSummary>>;

    /// Fork records, for one ledger or for every ledger.
    fn forks(
        &self,
        ledger: Option<LedgerId>,
        offset: usize,
        limit: usize,
    ) -> StoreFuture<'_, Page<ForkRecord>>;
}

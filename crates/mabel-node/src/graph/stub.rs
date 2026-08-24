//! A [`LedgerFetcher`] that answers from a table, for tests of the crawler
//! and of anything built over a generation.
//!
//! It opens no socket and reads no home, so a crawl over it is offline and
//! finishes in microseconds unless a delay is asked for. Ledgers absent from
//! the table are unreachable, which is what a crawl meets at the edge of any
//! real graph.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use iroh::EndpointId;
use mabel_core::{EventId, IdentityId, LedgerId};

use crate::api::documents::DeclaredKind;
use crate::graph::fetcher::{FetchFuture, FetchOutcome, LedgerFetcher, LedgerSummary, TrustEdge};
use crate::graph::model::{Equivocation, EquivocationBranch, FetchSource};
use crate::wallet::ids;

/// A deterministic event id for the attestation from `issuer` to `subject`.
///
/// Tests need ids that are stable across runs and distinct per edge; nothing
/// folds these bytes, so any injective mixing does.
#[must_use]
pub fn stub_attestation(issuer: LedgerId, subject: IdentityId) -> EventId {
    let mut bytes = [0u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = issuer.as_bytes()[index] ^ subject.as_bytes()[index].rotate_left(3);
    }
    EventId::from_bytes(bytes)
}

/// A deterministic head event id for `ledger`.
#[must_use]
pub fn stub_head(ledger: LedgerId) -> EventId {
    let mut bytes = *ledger.as_bytes();
    bytes[0] = bytes[0].wrapping_add(1);
    EventId::from_bytes(bytes)
}

/// An identity id built from one byte, so a test can name nodes `1`, `2`, `3`
/// and read the crawl order at a glance.
#[must_use]
pub fn stub_identity(seed: u8) -> IdentityId {
    IdentityId::from_bytes([seed; 32])
}

/// The fetch time every stubbed outcome carries, so two crawls over one
/// table produce identical documents.
pub const STUB_FETCHED_AT_MS: u64 = 1_700_000_000_000;

/// A fetcher that answers from a table.
#[derive(Debug)]
pub struct StubFetcher {
    replies: BTreeMap<LedgerId, FetchOutcome>,
    delay: Duration,
    fetched_at_ms: u64,
    calls: Mutex<Vec<LedgerId>>,
}

impl Default for StubFetcher {
    fn default() -> Self {
        Self {
            replies: BTreeMap::new(),
            delay: Duration::ZERO,
            fetched_at_ms: STUB_FETCHED_AT_MS,
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl StubFetcher {
    /// An empty table: every ledger is unreachable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same table, stamping every outcome with `fetched_at_ms`.
    #[must_use]
    pub const fn fetched_at(mut self, fetched_at_ms: u64) -> Self {
        self.fetched_at_ms = fetched_at_ms;
        self
    }

    /// The same table, with every fetch taking `delay`.
    ///
    /// Under `#[tokio::test(start_paused = true)]` this advances the test
    /// clock without sleeping, which is how the whole-run budget is tested.
    #[must_use]
    pub const fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// A verified ledger attesting to `subjects`, in the order given.
    #[must_use]
    pub fn trusting(self, ledger: LedgerId, subjects: &[IdentityId]) -> Self {
        self.reply(FetchOutcome::verified(
            summary(ledger, subjects),
            FetchSource::Local,
            vec![FetchSource::Local],
        ))
    }

    /// A verified ledger whose sources disagreed at `at_seq`.
    #[must_use]
    pub fn equivocating(
        self,
        ledger: LedgerId,
        subjects: &[IdentityId],
        at_seq: u64,
        branches: [(EndpointId, EventId); 2],
    ) -> Self {
        let branch = |(endpoint, event): (EndpointId, EventId)| EquivocationBranch {
            source: FetchSource::NodeWitness {
                endpoint: ids::key(&endpoint),
            },
            event: ids::event(event),
        };
        self.reply(
            FetchOutcome::verified(
                summary(ledger, subjects),
                branch(branches[0]).source.clone(),
                vec![branch(branches[0]).source, branch(branches[1]).source],
            )
            .with_equivocation(Equivocation {
                at_seq,
                branches: vec![branch(branches[0]), branch(branches[1])],
            }),
        )
    }

    /// A ledger no source holds.
    #[must_use]
    pub fn unreachable(self, ledger: LedgerId) -> Self {
        self.reply(FetchOutcome::unreachable(ledger, vec![FetchSource::Local]))
    }

    /// One prepared outcome, replacing any entry for the same ledger.
    #[must_use]
    pub fn reply(mut self, outcome: FetchOutcome) -> Self {
        self.replies.insert(outcome.ledger, outcome);
        self
    }

    /// Every ledger asked for, in the order the fetches completed.
    ///
    /// # Panics
    ///
    /// Panics if a fetch panicked while holding the call log.
    #[must_use]
    pub fn calls(&self) -> Vec<LedgerId> {
        self.calls.lock().expect("the call log is poisoned").clone()
    }

    /// How many fetches the crawl made.
    ///
    /// # Panics
    ///
    /// Panics if a fetch panicked while holding the call log.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("the call log is poisoned").len()
    }
}

/// A folded summary for a ledger that trusts `subjects`.
fn summary(ledger: LedgerId, subjects: &[IdentityId]) -> LedgerSummary {
    LedgerSummary {
        ledger,
        declared_kind: DeclaredKind::Person,
        display_name: None,
        hostname: None,
        email: None,
        head_seq: subjects.len() as u64,
        head_event: stub_head(ledger),
        witnesses: Vec::new(),
        trust: subjects
            .iter()
            .enumerate()
            .map(|(index, subject)| TrustEdge {
                subject: *subject,
                attestation_event: stub_attestation(ledger, *subject),
                seq: index as u64 + 1,
            })
            .collect(),
    }
}

impl LedgerFetcher for StubFetcher {
    fn fetch_candidate(&self, ledger: LedgerId, _sources: Vec<EndpointId>) -> FetchFuture<'_> {
        Box::pin(async move {
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            self.calls
                .lock()
                .expect("the call log is poisoned")
                .push(ledger);
            self.replies
                .get(&ledger)
                .cloned()
                .unwrap_or_else(|| FetchOutcome::unreachable(ledger, vec![FetchSource::Local]))
                .at(self.fetched_at_ms)
        })
    }
}

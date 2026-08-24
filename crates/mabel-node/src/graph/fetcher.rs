//! Reading one frontier ledger, from every source that might hold it
//! (proposal 003 section 3).
//!
//! The source order is normative and every applicable source is queried
//! rather than the walk stopping at the first that answers, because a second
//! answer is how equivocation is seen at all:
//!
//! 1. a local copy under `ledgers/`;
//! 2. `peers.json` hints for that ledger id, plus any the crawl learned;
//! 3. the node-wide witnesses in `node.json`;
//! 4. witnesses named by a verified copy of that ledger's own
//!    `WitnessConfig`, reachable only once one of the first three produced a
//!    copy.
//!
//! Verification happens in memory, over the same [`WalletSync::candidate`]
//! path a deliberate fetch uses, and the crawl keeps only the folded summary.
//! No stranger's ledger is written under `ledgers/`: the crawler is a reader,
//! and a wallet that stored every ledger it glanced at would stop being able
//! to say which ledgers its owner is responsible for.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use iroh::EndpointId;
use mabel_core::{EventId, IdentityId, LedgerId};

use crate::api::documents::{DeclaredKind, Id};
use crate::api::error::ServiceError;
use crate::graph::model::{Equivocation, EquivocationBranch, FetchSource, NodeStatus};
use crate::home::NodeHome;
use crate::now_ms;
use crate::wallet::ids;
use crate::wallet::{LoadedLedger, WalletCore, WalletSync};

/// How long one source has to answer before the crawl moves on (proposal 003
/// section 3).
pub const PER_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// What [`LedgerFetcher::fetch_candidate`] returns, boxed so the trait is
/// object-safe and the crawler can hold `&dyn LedgerFetcher`.
pub type FetchFuture<'a> = Pin<Box<dyn Future<Output = FetchOutcome> + Send + 'a>>;

/// One outgoing attestation of a folded ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustEdge {
    /// The identity the attestation names.
    pub subject: IdentityId,
    /// The `TrustAttestation` event.
    pub attestation_event: EventId,
    /// Its position in the ledger.
    pub seq: u64,
}

/// Everything the crawl keeps from one verified ledger.
///
/// The events themselves are dropped with the fold: what a generation records
/// is this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSummary {
    /// The ledger that was read.
    pub ledger: LedgerId,
    /// What its inception declares, advisory (proposal 002 section 3).
    pub declared_kind: DeclaredKind,
    /// The name its profile publishes.
    pub display_name: Option<String>,
    /// The hostname its profile claims, unverified here.
    pub hostname: Option<String>,
    /// The email its profile publishes.
    pub email: Option<String>,
    /// The last position of the chain that was read.
    pub head_seq: u64,
    /// The event at that position.
    pub head_event: EventId,
    /// The witnesses its latest `WitnessConfig` names, which are source 4.
    pub witnesses: Vec<EndpointId>,
    /// Its current attestations, ascending by position. A revoked
    /// attestation is not an edge.
    pub trust: Vec<TrustEdge>,
}

impl LedgerSummary {
    /// The summary of a folded chain.
    #[must_use]
    pub fn of(loaded: &LoadedLedger) -> Self {
        let mut trust: Vec<TrustEdge> = loaded
            .state
            .trust()
            .iter()
            .filter(|(_, attestation)| !attestation.is_revoked())
            .map(|(event, attestation)| TrustEdge {
                subject: attestation.subject,
                attestation_event: *event,
                seq: loaded.seq_of.get(event).copied().unwrap_or_default(),
            })
            .collect();
        trust.sort_by(|left, right| {
            left.seq
                .cmp(&right.seq)
                .then_with(|| left.subject.cmp(&right.subject))
        });
        let profile = loaded.state.profile();
        Self {
            ledger: loaded.ledger,
            declared_kind: loaded.declared_kind(),
            display_name: profile.and_then(|profile| profile.display_name.clone()),
            hostname: profile.and_then(|profile| profile.hostname.clone()),
            email: profile.and_then(|profile| profile.email.clone()),
            head_seq: loaded.head_seq,
            head_event: loaded.head_event,
            witnesses: loaded.state.witnesses().to_vec(),
            trust,
        }
    }
}

/// What one attempt to read a ledger produced.
///
/// A failure is a value, never an error: a crawl that stopped at the first
/// unreachable ledger would report nothing about the ones it could read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    /// The ledger that was asked for.
    pub ledger: LedgerId,
    /// How the read ended.
    pub status: NodeStatus,
    /// The folded copy, present whenever a source served one that verifies,
    /// equivocation included.
    pub summary: Option<LedgerSummary>,
    /// The source that served the kept copy.
    pub source: Option<FetchSource>,
    /// Every source that was asked, in the order they were asked.
    pub sources_tried: Vec<FetchSource>,
    /// Both branches when two sources disagreed.
    pub equivocation: Option<Equivocation>,
    /// When the read finished.
    pub fetched_at_ms: u64,
    /// One sentence about a failure, for a person reading the file.
    pub detail: Option<String>,
}

impl FetchOutcome {
    /// A ledger no source served.
    #[must_use]
    pub fn unreachable(ledger: LedgerId, sources_tried: Vec<FetchSource>) -> Self {
        Self {
            ledger,
            status: NodeStatus::Unreachable,
            summary: None,
            source: None,
            sources_tried,
            equivocation: None,
            fetched_at_ms: now_ms(),
            detail: None,
        }
    }

    /// A ledger every answering source served badly.
    #[must_use]
    pub fn invalid(
        ledger: LedgerId,
        sources_tried: Vec<FetchSource>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status: NodeStatus::Invalid,
            detail: Some(detail.into()),
            ..Self::unreachable(ledger, sources_tried)
        }
    }

    /// A ledger one source served and that folded with no violation.
    #[must_use]
    pub fn verified(
        summary: LedgerSummary,
        source: FetchSource,
        sources_tried: Vec<FetchSource>,
    ) -> Self {
        Self {
            ledger: summary.ledger,
            status: NodeStatus::Ok,
            summary: Some(summary),
            source: Some(source),
            sources_tried,
            equivocation: None,
            fetched_at_ms: now_ms(),
            detail: None,
        }
    }

    /// The same outcome with an equivocation recorded on it.
    ///
    /// The kept summary is the first copy in source order; recording both
    /// branches is what stops the divergence from being resolved silently.
    #[must_use]
    pub fn with_equivocation(mut self, equivocation: Equivocation) -> Self {
        self.status = NodeStatus::Equivocation;
        self.equivocation = Some(equivocation);
        self
    }

    /// The same outcome stamped with `fetched_at_ms`, which a test fixes.
    #[must_use]
    pub const fn at(mut self, fetched_at_ms: u64) -> Self {
        self.fetched_at_ms = fetched_at_ms;
        self
    }
}

/// Reads one ledger from wherever it can be found.
///
/// One method, so the crawler's tests inject a stub and no unit test opens a
/// socket. `sources` carries endpoints the crawl learned for this ledger,
/// queried with the `peers.json` hints of step 2; the four ordered sources
/// themselves are the implementation's business, because only it knows the
/// node home.
pub trait LedgerFetcher: Send + Sync {
    /// Reads `ledger`, verifying every candidate from nothing.
    fn fetch_candidate(&self, ledger: LedgerId, sources: Vec<EndpointId>) -> FetchFuture<'_>;
}

/// One source the plan will ask, with the endpoint to dial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedSource {
    /// How the source was found, which is what the node file records.
    pub source: FetchSource,
    /// The endpoint to dial, absent for the local copy.
    pub endpoint: Option<EndpointId>,
}

impl PlannedSource {
    /// The local copy under `ledgers/`.
    #[must_use]
    pub const fn local() -> Self {
        Self {
            source: FetchSource::Local,
            endpoint: None,
        }
    }

    /// One endpoint found as `kind`.
    #[must_use]
    pub fn endpoint(endpoint: EndpointId, kind: fn(Id) -> FetchSource) -> Self {
        Self {
            source: kind(ids::key(&endpoint)),
            endpoint: Some(endpoint),
        }
    }
}

/// Sources 1 to 3 for `ledger`, in order, with no endpoint asked twice.
///
/// `learned` is what the crawl picked up elsewhere and is queried with the
/// `peers.json` hints. Source 4 comes from [`ledger_witness_sources`] once a
/// copy has verified.
///
/// # Errors
///
/// Returns the errors of reading `node.json` and `peers.json`.
pub fn plan_sources(
    core: &WalletCore,
    ledger: LedgerId,
    learned: &[EndpointId],
) -> Result<Vec<PlannedSource>, ServiceError> {
    let mut planned = Vec::new();
    if core.holds(ledger)? {
        planned.push(PlannedSource::local());
    }
    let peers = core.home().peers().map_err(crate::wallet::storage_error)?;
    for endpoint in peers.hints(ledger).iter().chain(learned) {
        push_unique(
            &mut planned,
            PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::PeerHint { endpoint }),
        );
    }
    for endpoint in &core.config()?.witnesses {
        push_unique(
            &mut planned,
            PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::NodeWitness { endpoint }),
        );
    }
    Ok(planned)
}

/// Source 4: the witnesses a verified copy names, minus everything already
/// asked.
#[must_use]
pub fn ledger_witness_sources(
    planned: &[PlannedSource],
    witnesses: &[EndpointId],
) -> Vec<PlannedSource> {
    let mut extra: Vec<PlannedSource> = Vec::new();
    for endpoint in witnesses {
        let next = PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::LedgerWitness {
            endpoint,
        });
        if planned.iter().any(|asked| asked.endpoint == next.endpoint) {
            continue;
        }
        push_unique(&mut extra, next);
    }
    extra
}

fn push_unique(planned: &mut Vec<PlannedSource>, next: PlannedSource) {
    if planned.iter().any(|asked| asked.endpoint == next.endpoint) {
        return;
    }
    planned.push(next);
}

/// The [`LedgerFetcher`] the node runs: the source order over one home and
/// one Iroh endpoint.
#[derive(Debug)]
pub struct NetLedgerFetcher {
    core: WalletCore,
    sync: WalletSync,
    /// Serializes the `peers.json` read-modify-write, so two fetches finishing
    /// together cannot drop one another's hint.
    hints: tokio::sync::Mutex<()>,
}

impl NetLedgerFetcher {
    /// A fetcher over `core`, dialling with `sync`.
    ///
    /// The per-source deadline is [`PER_FETCH_TIMEOUT`] whatever `sync` was
    /// configured with: a crawl asks strangers, and the ten seconds a
    /// deliberate push allows is too long to spend on one of five hundred.
    #[must_use]
    pub fn new(core: WalletCore, sync: WalletSync) -> Self {
        Self {
            core,
            sync: sync.with_timeout(PER_FETCH_TIMEOUT),
            hints: tokio::sync::Mutex::new(()),
        }
    }

    /// Reads `ledger` from every applicable source, in order.
    async fn fetch(&self, ledger: LedgerId, learned: Vec<EndpointId>) -> FetchOutcome {
        let mut planned = match plan_sources(&self.core, ledger, &learned) {
            Ok(planned) => planned,
            Err(error) => return FetchOutcome::invalid(ledger, Vec::new(), error.to_string()),
        };
        let mut tried: Vec<FetchSource> = Vec::new();
        let mut candidates: Vec<(FetchSource, LoadedLedger)> = Vec::new();
        let mut invalid: Option<String> = None;
        let mut index = 0;
        while index < planned.len() {
            let next = planned[index].clone();
            index += 1;
            tried.push(next.source.clone());
            match self.read(ledger, &next).await {
                Ok(Some(loaded)) => {
                    // Source 4 exists only once a copy verified: the witness
                    // set is a fact of the chain that was just folded.
                    let named = ledger_witness_sources(&planned, loaded.state.witnesses());
                    planned.extend(named);
                    candidates.push((next.source, loaded));
                }
                Ok(None) => {}
                Err(detail) => invalid = invalid.or(Some(detail)),
            }
        }
        let outcome = decide(ledger, tried, candidates, invalid);
        self.record_hints(ledger, &outcome).await;
        outcome
    }

    /// One source's answer: `Ok(None)` for an unreachable source, `Err` for
    /// one that served a chain which does not verify.
    async fn read(
        &self,
        ledger: LedgerId,
        planned: &PlannedSource,
    ) -> Result<Option<LoadedLedger>, String> {
        let Some(endpoint) = planned.endpoint else {
            let loaded = self.core.load(ledger).map_err(|error| error.to_string())?;
            if loaded.is_empty() {
                return Ok(None);
            }
            if loaded.violation.is_some() || loaded.state.ledger() != Some(ledger) {
                return Err(format!("the local copy of {ledger} does not verify"));
            }
            return Ok(Some(loaded));
        };
        let served =
            tokio::time::timeout(PER_FETCH_TIMEOUT, self.sync.candidate(endpoint, ledger)).await;
        match served {
            Ok(Ok(loaded)) => Ok(loaded),
            // Code 20 is a chain that does not verify; everything else is a
            // source that could not answer, which is not this ledger's fault.
            Ok(Err(error)) if error.code() == 20 => Err(error.to_string()),
            Ok(Err(_)) | Err(_) => Ok(None),
        }
    }

    /// Writes the endpoint that served a verified copy back to `peers.json`,
    /// so the next crawl asks it first.
    async fn record_hints(&self, ledger: LedgerId, outcome: &FetchOutcome) {
        let Some(endpoint) = outcome
            .source
            .as_ref()
            .and_then(FetchSource::endpoint)
            .and_then(|id| ids::parse_endpoint(id).ok())
        else {
            return;
        };
        let _guard = self.hints.lock().await;
        record_hint(self.core.home(), ledger, endpoint);
    }
}

/// Adds one endpoint to `peers.json` as a hint for `ledger`.
///
/// A hint is an address, never authorization: the next crawl still folds
/// whatever this endpoint serves from nothing (proposal 001 section 4). A
/// failed write is logged, because losing a hint costs one extra dial and
/// failing the crawl over it costs the whole graph.
pub fn record_hint(home: &NodeHome, ledger: LedgerId, endpoint: EndpointId) {
    let Ok(mut peers) = home.peers() else {
        return;
    };
    if peers.hints(ledger).contains(&endpoint) {
        return;
    }
    peers.add_hint(ledger, endpoint);
    if let Err(error) = home.write_peers(&peers) {
        tracing::warn!(%ledger, %error, "could not record a graph peer hint");
    }
}

impl LedgerFetcher for NetLedgerFetcher {
    fn fetch_candidate(&self, ledger: LedgerId, sources: Vec<EndpointId>) -> FetchFuture<'_> {
        Box::pin(self.fetch(ledger, sources))
    }
}

/// The outcome of comparing what the sources served.
///
/// The first copy in source order is the one kept, and a later copy that
/// diverges from it is recorded rather than compared away. A copy that merely
/// extends another is the same chain seen further along, so the longer one
/// wins and nothing is recorded.
pub(super) fn decide(
    ledger: LedgerId,
    tried: Vec<FetchSource>,
    candidates: Vec<(FetchSource, LoadedLedger)>,
    invalid: Option<String>,
) -> FetchOutcome {
    let Some((first_source, first)) = candidates.first() else {
        return match invalid {
            Some(detail) => FetchOutcome::invalid(ledger, tried, detail),
            None => FetchOutcome::unreachable(ledger, tried),
        };
    };
    let mut kept = (first_source.clone(), first);
    let mut equivocation = None;
    for (source, loaded) in candidates.iter().skip(1) {
        match divergence(first, loaded) {
            Some((at_seq, first_event, other_event)) => {
                equivocation.get_or_insert_with(|| Equivocation {
                    at_seq,
                    branches: vec![
                        EquivocationBranch {
                            source: first_source.clone(),
                            event: first_event,
                        },
                        EquivocationBranch {
                            source: source.clone(),
                            event: other_event,
                        },
                    ],
                });
            }
            None if loaded.event_count() > kept.1.event_count() => {
                kept = (source.clone(), loaded);
            }
            None => {}
        }
    }
    if equivocation.is_some() {
        // Nothing here resolves a divergence, so the copy kept is simply the
        // first in source order and both branches are on the record.
        kept = (first_source.clone(), first);
    }
    let outcome = FetchOutcome::verified(LedgerSummary::of(kept.1), kept.0, tried);
    match equivocation {
        Some(equivocation) => outcome.with_equivocation(equivocation),
        None => outcome,
    }
}

/// The first sequence where two chains hold different events, with the event
/// each holds there.
fn divergence(left: &LoadedLedger, right: &LoadedLedger) -> Option<(u64, Id, Id)> {
    for (seq, (one, other)) in left.events.iter().zip(right.events.iter()).enumerate() {
        if one == other {
            continue;
        }
        let event_of = |loaded: &LoadedLedger| {
            loaded
                .event_ids
                .get(seq)
                .copied()
                .flatten()
                .map_or_else(|| ids::bytes(&[0u8; 32]), ids::event)
        };
        return Some((seq as u64, event_of(left), event_of(right)));
    }
    None
}

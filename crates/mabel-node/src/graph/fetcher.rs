//! Reading one frontier ledger, from every source that might hold it
//! (proposal 006 section 5).
//!
//! The source order is normative and every applicable source is queried
//! rather than the walk stopping at the first that answers, because a second
//! answer is how equivocation is seen at all:
//!
//! 1. `Local`: a copy under `ledgers/`;
//! 2. `CallerHint`: an endpoint supplied with this request, from a `mabel://`
//!    link, a `--peer` ticket or `--from`;
//! 3. `PeerHint`: `peers.json` for this ledger, plus what this crawl learned;
//! 4. `NodeWitness`: the endpoints of each identity in `node.json.witnesses`,
//!    resolved by section 5.1, which needs no copy of anything;
//! 5. `LedgerEndpoint`: the endpoints the ledger's own tag-18 advertisement
//!    names, reachable only once another source produced a copy;
//! 6. `WitnessIdentity`: the endpoints of each identity in the ledger's tag-19
//!    `WitnessSet`, each resolved by section 5.1;
//! 7. `LegacyWitnessHint`: the endpoints in the ledger's retired tag-11
//!    `WitnessConfig`;
//! 8. `DnsEndpoint`: the `mabel-endpoints=` records of a hostname, queried only
//!    when sources 1 to 7 produced no reachable copy.
//!
//! One [`Resolution`] carries the dial budget, the deadline and the visited
//! identity set for the whole top-level operation (section 5.2), so an endpoint
//! three sources name costs one slot and 16 distinct endpoints is the whole
//! operation's ration.
//!
//! Verification happens in memory, over the same [`WalletSync::candidate`]
//! path a deliberate fetch uses, and the crawl keeps only the folded summary.
//! No stranger's ledger is written under `ledgers/`: the crawler is a reader,
//! and a wallet that stored every ledger it glanced at would stop being able
//! to say which ledgers its owner is responsible for.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use iroh::EndpointId;
use mabel_core::{EventId, IdentityId, LedgerId};

use crate::api::documents::{DeclaredKind, Id};
use crate::api::error::ServiceError;
use crate::graph::model::{Equivocation, EquivocationBranch, FetchSource, NodeStatus, SourceClass};
use crate::graph::resolve::Resolution;
use crate::graph::store::GraphStore;
use crate::home::NodeHome;
use crate::now_ms;
use crate::verification::{Resolver, endpoints_for_claim, query_name};
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
    /// The endpoints its own tag-18 `EndpointAdvertisement` names, which are
    /// source 5.
    pub endpoints: Vec<EndpointId>,
    /// The identities its tag-19 `WitnessSet` names, which are source 6 once
    /// each is resolved by proposal 006 section 5.1.
    pub witness_identities: Vec<IdentityId>,
    /// The endpoints its retired tag-11 `WitnessConfig` names, which are
    /// source 7. Never merged into [`LedgerSummary::endpoints`]: that field
    /// never promised an identity.
    pub legacy_witnesses: Vec<EndpointId>,
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
            endpoints: loaded.state.endpoints().to_vec(),
            witness_identities: loaded.state.witness_identities().to_vec(),
            legacy_witnesses: loaded.state.witness_endpoints().to_vec(),
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
/// queried with the `peers.json` hints of source 3; the ordered sources
/// themselves are the implementation's business, because only it knows the
/// node home. `resolution` is the operation's shared dial budget, deadline and
/// visited set (proposal 006 sections 5.1 and 5.2).
pub trait LedgerFetcher: Send + Sync {
    /// Reads `ledger`, verifying every candidate from nothing.
    fn fetch_candidate<'a>(
        &'a self,
        ledger: LedgerId,
        sources: Vec<EndpointId>,
        resolution: &'a Resolution,
    ) -> FetchFuture<'a>;
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

    /// One endpoint of a witness identity, found as `kind`.
    #[must_use]
    pub fn of_witness(
        witness: IdentityId,
        endpoint: EndpointId,
        kind: fn(Id, Id) -> FetchSource,
    ) -> Self {
        Self {
            source: kind(ids::identity(witness), ids::key(&endpoint)),
            endpoint: Some(endpoint),
        }
    }
}

/// Sources 1 to 4 for `ledger`, in order, with no endpoint asked twice and
/// every dial charged to `resolution`.
///
/// `learned` is what the crawl picked up elsewhere and is queried with the
/// `peers.json` hints of source 3. Sources 5 to 7 come from
/// [`chain_named_sources`] once a copy has verified, and source 8 only when
/// none did.
///
/// # Errors
///
/// Returns the errors of reading `node.json` and `peers.json`.
pub fn plan_sources(
    core: &WalletCore,
    ledger: LedgerId,
    learned: &[EndpointId],
    resolution: &Resolution,
) -> Result<Vec<PlannedSource>, ServiceError> {
    let mut planned = Vec::new();
    if core.holds(ledger)? {
        planned.push(PlannedSource::local());
    }
    // Source 2: a human just named these.
    for endpoint in resolution.caller_hints() {
        push_admitted(
            &mut planned,
            resolution,
            PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::CallerHint { endpoint }),
        );
    }
    // Source 3.
    let peers = core.home().peers().map_err(crate::wallet::storage_error)?;
    for endpoint in peers.hints(ledger).iter().chain(learned) {
        push_admitted(
            &mut planned,
            resolution,
            PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::PeerHint { endpoint }),
        );
    }
    // Source 4: the workhorse for a ledger this home has never seen, and the
    // class four of the sixteen slots are held back for.
    for entry in &core.config()?.witnesses {
        for endpoint in resolution.witness_endpoints(core, entry.identity)? {
            push_admitted(
                &mut planned,
                resolution,
                PlannedSource::of_witness(entry.identity, endpoint, |witness, endpoint| {
                    FetchSource::NodeWitness { witness, endpoint }
                }),
            );
        }
    }
    Ok(planned)
}

/// Sources 5, 6 and 7 off a copy that verified, minus everything already asked.
///
/// The three are one budget class and three sources: an endpoint reached
/// through the tag-11 list of source 7 is never merged into the tag-18
/// advertisement of source 5, and never establishes a binding.
///
/// # Errors
///
/// Returns the errors of reading `node.json` and `peers.json` while resolving
/// the identities the `WitnessSet` names.
pub fn chain_named_sources(
    core: &WalletCore,
    planned: &[PlannedSource],
    summary: &LedgerSummary,
    resolution: &Resolution,
) -> Result<Vec<PlannedSource>, ServiceError> {
    let mut extra: Vec<PlannedSource> = Vec::new();
    let add = |extra: &mut Vec<PlannedSource>, next: PlannedSource| {
        if planned.iter().any(|asked| asked.endpoint == next.endpoint) {
            return;
        }
        push_admitted(extra, resolution, next);
    };
    // Source 5.
    for endpoint in &summary.endpoints {
        add(
            &mut extra,
            PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::LedgerEndpoint {
                endpoint,
            }),
        );
    }
    // Source 6, each identity resolved by section 5.1 and each resolved once.
    for witness in &summary.witness_identities {
        for endpoint in resolution.witness_endpoints(core, *witness)? {
            add(
                &mut extra,
                PlannedSource::of_witness(*witness, endpoint, |witness, endpoint| {
                    FetchSource::WitnessIdentity { witness, endpoint }
                }),
            );
        }
    }
    // Source 7.
    for endpoint in &summary.legacy_witnesses {
        add(
            &mut extra,
            PlannedSource::endpoint(*endpoint, |endpoint| FetchSource::LegacyWitnessHint {
                endpoint,
            }),
        );
    }
    Ok(extra)
}

/// Adds `next` unless its endpoint was already planned or the dial budget
/// refuses it.
fn push_admitted(planned: &mut Vec<PlannedSource>, resolution: &Resolution, next: PlannedSource) {
    if planned.iter().any(|asked| asked.endpoint == next.endpoint) {
        return;
    }
    if let Some(endpoint) = next.endpoint
        && !resolution.admit(next.source.class(), endpoint)
    {
        return;
    }
    planned.push(next);
}

/// The [`LedgerFetcher`] the node runs: the source order over one home and
/// one Iroh endpoint.
pub struct NetLedgerFetcher {
    core: WalletCore,
    sync: WalletSync,
    /// The resolver source 8 queries, absent when this home has none. Source 8
    /// is skipped rather than failed then: a DNS hint is a recovery path, not a
    /// requirement.
    resolver: Option<Arc<dyn Resolver>>,
    /// Serializes the `peers.json` read-modify-write, so two fetches finishing
    /// together cannot drop one another's hint.
    hints: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for NetLedgerFetcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NetLedgerFetcher")
            .field("resolver", &self.resolver.is_some())
            .finish_non_exhaustive()
    }
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
            resolver: None,
            hints: tokio::sync::Mutex::new(()),
        }
    }

    /// The same fetcher with the resolver source 8 queries.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Arc<dyn Resolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Reads `ledger` from every applicable source, in order.
    async fn fetch(
        &self,
        ledger: LedgerId,
        learned: Vec<EndpointId>,
        resolution: &Resolution,
    ) -> FetchOutcome {
        let mut planned = match plan_sources(&self.core, ledger, &learned, resolution) {
            Ok(planned) => planned,
            Err(error) => return FetchOutcome::invalid(ledger, Vec::new(), error.to_string()),
        };
        let mut tried: Vec<FetchSource> = Vec::new();
        let mut candidates: Vec<(FetchSource, LoadedLedger)> = Vec::new();
        let mut failed: Vec<EndpointId> = Vec::new();
        let mut invalid: Option<String> = None;
        let mut index = 0;
        while index < planned.len() {
            if resolution.expired() {
                break;
            }
            let next = planned[index].clone();
            index += 1;
            tried.push(next.source.clone());
            match self.read(ledger, &next, resolution).await {
                Ok(Some(loaded)) => {
                    // Sources 5 to 7 exist only once a copy verified: the
                    // advertisement and the witness set are facts of the chain
                    // that was just folded.
                    let summary = LedgerSummary::of(&loaded);
                    match chain_named_sources(&self.core, &planned, &summary, resolution) {
                        Ok(named) => planned.extend(named),
                        Err(error) => invalid = invalid.or(Some(error.to_string())),
                    }
                    candidates.push((next.source, loaded));
                }
                Ok(None) => {
                    if let Some(endpoint) = next.endpoint {
                        failed.push(endpoint);
                    }
                }
                Err(detail) => invalid = invalid.or(Some(detail)),
            }
        }
        // Source 8 is queried only when sources 1 to 7 produced no reachable
        // copy: a DNS query tells a third-party resolver which identity this
        // wallet is looking for. The local copy is not a reachable copy, which
        // is the recovery path a rotation needs: a wallet holding an old copy of
        // a ledger whose every recorded endpoint is dead can still find its new
        // machines through the zone it already claimed (proposal 006 section 6).
        let reached = candidates
            .iter()
            .any(|(source, _)| source.endpoint().is_some());
        if !reached && !resolution.expired() {
            for next in self.dns_sources(ledger, resolution).await {
                if resolution.expired() {
                    break;
                }
                tried.push(next.source.clone());
                match self.read(ledger, &next, resolution).await {
                    Ok(Some(loaded)) => candidates.push((next.source, loaded)),
                    Ok(None) => {
                        if let Some(endpoint) = next.endpoint {
                            failed.push(endpoint);
                        }
                    }
                    Err(detail) => invalid = invalid.or(Some(detail)),
                }
            }
        }
        let outcome = decide(ledger, tried, candidates, invalid);
        self.record_hints(ledger, &outcome, &failed).await;
        outcome
    }

    /// Source 8: the `mabel-endpoints=` records of a hostname this home already
    /// holds for `ledger`, read under row 2 of the applicability matrix.
    ///
    /// The hostname comes from a stale local copy of the ledger or from the
    /// stored crawl generation, never from a guess, and the records count only
    /// when the same response carries `mabel=<ledger>` (proposal 006 section 6).
    async fn dns_sources(&self, ledger: LedgerId, resolution: &Resolution) -> Vec<PlannedSource> {
        let Some(resolver) = self.resolver.as_ref() else {
            return Vec::new();
        };
        let Some(hostname) = self.claimed_hostname(ledger) else {
            return Vec::new();
        };
        let Ok(records) = resolver.lookup_txt(&query_name(&hostname)).await else {
            return Vec::new();
        };
        let mut planned = Vec::new();
        for endpoint in endpoints_for_claim(&records, ledger) {
            if !resolution.admit(SourceClass::Dns, endpoint) {
                continue;
            }
            planned.push(PlannedSource {
                source: FetchSource::DnsEndpoint {
                    hostname: hostname.clone(),
                    endpoint: ids::key(&endpoint),
                },
                endpoint: Some(endpoint),
            });
        }
        planned
    }

    /// The hostname a stale local copy of `ledger` claims, or the one the
    /// stored crawl generation recorded for it.
    fn claimed_hostname(&self, ledger: LedgerId) -> Option<String> {
        if let Ok(loaded) = self.core.load(ledger)
            && !loaded.is_empty()
            && let Some(hostname) = loaded
                .state
                .profile()
                .and_then(|profile| profile.hostname.clone())
        {
            return Some(hostname);
        }
        let generation = GraphStore::in_home(self.core.home())
            .current_generation()
            .ok()
            .flatten()?;
        generation
            .node(&ids::identity(ledger))
            .and_then(|node| node.hostname.clone())
    }

    /// One source's answer: `Ok(None)` for an unreachable source, `Err` for
    /// one that served a chain which does not verify.
    async fn read(
        &self,
        ledger: LedgerId,
        planned: &PlannedSource,
        resolution: &Resolution,
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
        // No source outlives the operation's shared deadline.
        let deadline = PER_FETCH_TIMEOUT.min(resolution.remaining());
        let served = tokio::time::timeout(deadline, self.sync.candidate(endpoint, ledger)).await;
        match served {
            Ok(Ok(loaded)) => Ok(loaded),
            // Code 20 is a chain that does not verify; everything else is a
            // source that could not answer, which is not this ledger's fault.
            Ok(Err(error)) if error.code() == 20 => Err(error.to_string()),
            Ok(Err(_)) | Err(_) => Ok(None),
        }
    }

    /// Keeps `peers.json` honest about this ledger: the endpoint that served
    /// the kept copy gets a success, and every endpoint that did not answer
    /// gets a failure (proposal 006 section 5.3).
    ///
    /// A `CallerHint` endpoint is never written.
    async fn record_hints(&self, ledger: LedgerId, outcome: &FetchOutcome, failed: &[EndpointId]) {
        let served = outcome
            .source
            .as_ref()
            .filter(|source| source.may_record_hint())
            .and_then(FetchSource::endpoint)
            .and_then(|id| ids::parse_endpoint(id).ok());
        let caller: Vec<EndpointId> = outcome
            .sources_tried
            .iter()
            .filter(|source| !source.may_record_hint())
            .filter_map(FetchSource::endpoint)
            .filter_map(|id| ids::parse_endpoint(id).ok())
            .collect();
        let _guard = self.hints.lock().await;
        let home = self.core.home();
        let Ok(mut peers) = home.peers() else {
            return;
        };
        let before = peers.clone();
        for endpoint in failed {
            if caller.contains(endpoint) {
                continue;
            }
            peers.record_failure(ledger, *endpoint);
        }
        if let Some(endpoint) = served {
            peers.record_success(ledger, endpoint, now_ms());
        }
        peers.prune(now_ms());
        if peers == before {
            return;
        }
        if let Err(error) = home.write_peers(&peers) {
            tracing::warn!(%ledger, %error, "could not record a graph peer hint");
        }
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
    peers.record_success(ledger, endpoint, now_ms());
    peers.prune(now_ms());
    if let Err(error) = home.write_peers(&peers) {
        tracing::warn!(%ledger, %error, "could not record a graph peer hint");
    }
}

impl LedgerFetcher for NetLedgerFetcher {
    fn fetch_candidate<'a>(
        &'a self,
        ledger: LedgerId,
        sources: Vec<EndpointId>,
        resolution: &'a Resolution,
    ) -> FetchFuture<'a> {
        Box::pin(self.fetch(ledger, sources, resolution))
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

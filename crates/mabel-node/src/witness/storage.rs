//! What a witness holds, and every rule that decides whether it grows.
//!
//! One [`WitnessStorage`] owns a node home and an in-memory index over it: for
//! each ledger the folded [`LedgerState`], where the ledger ends, how many
//! bytes it takes and the fork records recorded for it. The index is a cache
//! and is rebuilt from the event files on startup or on demand
//! ([`WitnessStorage::reload`]); the files are the truth.
//!
//! Verification follows proposal 001 section 5: the first ingest of a ledger
//! folds the whole pushed chain from nothing, and every later push applies
//! only the spliced suffix to the kept state. Nothing is stored that the fold
//! did not accept, and stored bytes are the received bytes (section 3.1).
//!
//! Every method here is blocking `std::fs` work. The async surfaces,
//! [`crate::witness::WitnessStore`] and
//! [`crate::witness::WitnessReadService`], call them from
//! `tokio::task::spawn_blocking`.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use iroh_base::EndpointId;
use mabel_core::fold::LedgerState;
use mabel_core::fork::ForkError;
use mabel_core::proto::{DeclaredKind, RejectCode};
use mabel_core::{EventId, IdentityId, LedgerId, Reason, validate_fork_record};
use mabel_net::error::Rejection;
use mabel_net::store::{
    EventPage, ForkRecord, Head, LedgerSummary, Page, Provenance, PushOutcome, StoreError,
};
use mabel_net::wire;
use tracing::warn;

use crate::config::{DEFAULT_STORAGE_CAPACITY, NodeConfig};
use crate::error::{Result, StorageError, io_at};
use crate::home::NodeHome;
use crate::ledger::{LedgerStore, NewEvent};
use crate::now_ms;

/// What `node.json` says about which pushes this home takes (proposal 006
/// section 4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdmissionPolicy {
    /// The witness identities this home witnesses for. Empty means nobody.
    pub witness_for: Vec<IdentityId>,
    /// Whether the retired tag-11 clause may admit a push.
    pub accept_legacy_witness_config: bool,
}

impl AdmissionPolicy {
    /// The policy `node.json` records.
    #[must_use]
    pub fn from_config(config: &NodeConfig) -> Self {
        Self {
            witness_for: config.witness_for.clone(),
            accept_legacy_witness_config: config.accept_legacy_witness_config,
        }
    }

    /// A policy that witnesses for `witness_for` and refuses the tag-11
    /// clause, which is every home written after proposal 006.
    #[must_use]
    pub fn witnessing_for(witness_for: Vec<IdentityId>) -> Self {
        Self {
            witness_for,
            accept_legacy_witness_config: false,
        }
    }
}

/// Why a `witness_for` entry does not admit a ledger this home does not store
/// (proposal 006 section 4.1).
///
/// One of the three reasons the startup log and `GET /api/node` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertisementGap {
    /// This home holds no copy of that identity's ledger.
    NoLocalCopy,
    /// The copy it holds advertises no endpoint at all.
    AdvertisesNothing,
    /// The copy it holds advertises other endpoints and not this one.
    AdvertisesOtherEndpoints,
}

impl AdvertisementGap {
    /// The sentence the log line and the node document carry.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoLocalCopy => "this home holds no copy of that identity's ledger",
            Self::AdvertisesNothing => "that identity's ledger advertises no endpoint",
            Self::AdvertisesOtherEndpoints => {
                "that identity's ledger advertises other endpoints and not this one"
            }
        }
    }
}

impl std::fmt::Display for AdvertisementGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

/// One `witness_for` entry and whether it admits a new ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessForEntry {
    /// The witness identity `node.json` names.
    pub identity: IdentityId,
    /// `None` when the latest local copy of that identity advertises this
    /// home's endpoint, which is what proposal 006 section 4.1 requires.
    pub gap: Option<AdvertisementGap>,
}

impl WitnessForEntry {
    /// Whether this entry admits a ledger this home does not already store.
    #[must_use]
    pub const fn advertised(&self) -> bool {
        self.gap.is_none()
    }
}

/// The endpoint this home answers on and what each `witness_for` entry says
/// about it.
#[derive(Debug)]
struct Advertised {
    endpoint: EndpointId,
    entries: Vec<WitnessForEntry>,
}

/// Events one ledger may hold (proposal 001 section 5).
pub const MAX_EVENTS_PER_LEDGER: u64 = 4096;

/// Bytes of events one ledger may hold (proposal 001 section 5).
pub const MAX_BYTES_PER_LEDGER: u64 = 4 * 1024 * 1024;

/// Ledgers one witness may hold (proposal 001 section 5).
pub const MAX_LEDGERS: usize = 10_000;

/// Fork records one ledger may hold, after which recording stops and
/// `forks_truncated` is set (proposal 001 section 5).
pub const MAX_FORK_RECORDS: u32 = 8;

/// The caps one witness enforces.
///
/// `storage_capacity` comes from `node.json`; the other four are the fixed
/// numbers of proposal 001 section 5 and are settable so a test can cross a
/// cap without writing four thousand events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessCaps {
    /// Events one ledger may hold.
    pub events_per_ledger: u64,
    /// Bytes of events one ledger may hold.
    pub bytes_per_ledger: u64,
    /// Ledgers this witness may hold.
    pub ledgers: usize,
    /// Fork records one ledger may hold before recording stops.
    pub fork_records: u32,
    /// Bytes of event data this witness accepts before refusing more.
    pub storage_capacity: u64,
}

impl Default for WitnessCaps {
    fn default() -> Self {
        Self {
            events_per_ledger: MAX_EVENTS_PER_LEDGER,
            bytes_per_ledger: MAX_BYTES_PER_LEDGER,
            ledgers: MAX_LEDGERS,
            fork_records: MAX_FORK_RECORDS,
            storage_capacity: DEFAULT_STORAGE_CAPACITY,
        }
    }
}

impl WitnessCaps {
    /// The section 5 caps with `storage_capacity` from `node.json`.
    #[must_use]
    pub fn from_config(config: &NodeConfig) -> Self {
        Self {
            storage_capacity: config.storage_capacity,
            ..Self::default()
        }
    }
}

/// What the HTTP surface needs about one ledger, beyond its
/// [`LedgerSummary`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerReport {
    /// The row `List` reports.
    pub summary: LedgerSummary,
    /// The endpoint the ledger's first event arrived from, provenance only.
    pub source_endpoint: Option<EndpointId>,
    /// The identities the ledger's latest `WitnessSet` names (proposal 006
    /// section 1).
    pub witnesses: Vec<IdentityId>,
}

/// What `GET /api/node` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    /// Ledgers held.
    pub ledger_count: u64,
    /// Fork records held, over every ledger.
    pub fork_count: u64,
    /// Bytes of event data held.
    pub storage_used: u64,
}

/// One ledger's events, as a read serves them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPage {
    /// The ledger these events belong to.
    pub report: LedgerReport,
    /// The encoded `SignedEvent`s, ascending from the requested `since`.
    pub events: Vec<Vec<u8>>,
    /// Whether events past this page exist.
    pub more: bool,
}

/// One ledger in the index.
#[derive(Debug)]
struct Entry {
    /// The fold of the stored events.
    state: LedgerState,
    /// Position of the last stored event.
    head_seq: u64,
    /// Id of the last stored event.
    head_event: EventId,
    /// Bytes the stored events take.
    bytes: u64,
    /// When this witness first stored an event of the ledger.
    first_seen_ms: u64,
    /// When it last accepted one.
    updated_ms: u64,
    /// Where the first event came from, provenance only.
    source_endpoint: Option<EndpointId>,
    /// The fork records held, by ascending sequence then conflicting id.
    forks: Vec<ForkRecord>,
}

impl Entry {
    fn event_count(&self) -> u64 {
        self.head_seq + 1
    }

    fn declared_kind(&self) -> DeclaredKind {
        self.state
            .declared_kind()
            .unwrap_or(DeclaredKind::KindUnspecified)
    }

    /// Whether fork recording has stopped for this ledger.
    ///
    /// The flag is derived from the count rather than stored: recording stops
    /// once `cap` records exist, so the flag survives a restart with no extra
    /// file (proposal 001 section 5).
    fn forks_truncated(&self, cap: u32) -> bool {
        self.forks.len() as u32 >= cap
    }

    fn summary(&self, ledger: LedgerId, cap: u32) -> LedgerSummary {
        LedgerSummary {
            ledger,
            declared_kind: self.declared_kind(),
            head_seq: self.head_seq,
            head_event: self.head_event,
            event_count: self.event_count(),
            first_seen_ms: self.first_seen_ms,
            updated_ms: self.updated_ms,
            fork_count: self.forks.len() as u32,
            forks_truncated: self.forks_truncated(cap),
        }
    }

    fn report(&self, ledger: LedgerId, cap: u32) -> LedgerReport {
        LedgerReport {
            summary: self.summary(ledger, cap),
            source_endpoint: self.source_endpoint,
            witnesses: self.state.witness_identities().to_vec(),
        }
    }

    fn head(&self) -> Head {
        Head {
            head_seq: self.head_seq,
            head_event: self.head_event,
            updated_ms: self.updated_ms,
        }
    }
}

/// The whole index, ordered by ascending ledger id so paging is stable.
#[derive(Debug, Default)]
struct Index {
    ledgers: BTreeMap<LedgerId, Entry>,
    storage_used: u64,
}

/// A witness's ledgers, its folded-state cache, its caps and the identities it
/// witnesses for.
#[derive(Debug)]
pub struct WitnessStorage {
    home: NodeHome,
    caps: WitnessCaps,
    policy: AdmissionPolicy,
    index: Mutex<Index>,
    advertised: Mutex<Advertised>,
}

impl WitnessStorage {
    /// Opens a home, builds the index from the event files and checks the
    /// advertisement invariant of proposal 006 section 4.1 once, naming each
    /// failing `witness_for` entry in the log.
    ///
    /// A failing entry never stops the open: a witness whose advertisement has
    /// not landed yet serves what it has.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of reading the ledger directories.
    pub fn open(
        home: NodeHome,
        endpoint: EndpointId,
        caps: WitnessCaps,
        policy: AdmissionPolicy,
    ) -> Result<Self> {
        let storage = Self {
            home,
            caps,
            policy,
            index: Mutex::new(Index::default()),
            advertised: Mutex::new(Advertised {
                endpoint,
                entries: Vec::new(),
            }),
        };
        storage.reload()?;
        for entry in storage.witness_for_entries() {
            if let Some(gap) = entry.gap {
                warn!(
                    witness = %entry.identity,
                    reason = gap.reason(),
                    "this home witnesses for an identity that does not advertise it, so it takes \
                     no new ledger under that identity; the ledgers it already stores keep growing"
                );
            }
        }
        Ok(storage)
    }

    /// Opens a home with the caps and the admission policy `node.json` names.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`NodeHome::config`] and [`WitnessStorage::open`].
    pub fn open_from_config(home: NodeHome, endpoint: EndpointId) -> Result<Self> {
        let config = home.config()?;
        let caps = WitnessCaps::from_config(&config);
        let policy = AdmissionPolicy::from_config(&config);
        Self::open(home, endpoint, caps, policy)
    }

    /// The home this witness stores into.
    #[must_use]
    pub fn home(&self) -> &NodeHome {
        &self.home
    }

    /// This witness's own endpoint id, which is what admission checks for.
    #[must_use]
    pub fn endpoint(&self) -> EndpointId {
        self.advertisement().endpoint
    }

    /// The caps this witness enforces.
    #[must_use]
    pub fn caps(&self) -> WitnessCaps {
        self.caps
    }

    /// The witness identities this home witnesses for, from
    /// `node.json.witness_for` (proposal 006 section 4).
    ///
    /// Empty means this home witnesses for nobody, and every push for a ledger
    /// it holds no signing key for is refused.
    #[must_use]
    pub fn witness_for(&self) -> &[IdentityId] {
        &self.policy.witness_for
    }

    /// Whether `node.json` turns the retired tag-11 clause on.
    #[must_use]
    pub fn accepts_legacy_witness_config(&self) -> bool {
        self.policy.accept_legacy_witness_config
    }

    /// Each `witness_for` entry with the reason it admits no new ledger, or
    /// `None` per entry when it does (proposal 006 section 4.1).
    ///
    /// This is what `GET /api/node` reports beside the id.
    #[must_use]
    pub fn witness_for_entries(&self) -> Vec<WitnessForEntry> {
        self.advertisement().entries.clone()
    }

    /// Records a new endpoint id for this home and rechecks the advertisement
    /// invariant against it, which is what a regenerated `node.key` needs
    /// (proposal 006 section 4.1).
    pub fn note_endpoint(&self, endpoint: EndpointId) {
        {
            let mut advertised = self.advertisement();
            if advertised.endpoint == endpoint {
                return;
            }
            advertised.endpoint = endpoint;
        }
        let index = self.lock();
        self.recheck(&index);
    }

    /// Rebuilds the folded-state cache from the event files, and the
    /// advertisement verdicts with it.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of reading the ledger directories.
    pub fn reload(&self) -> Result<()> {
        let mut rebuilt = Index::default();
        for ledger in self.home.ledgers()? {
            let store = self.home.ledger(ledger);
            let Some(entry) = load_entry(&store)? else {
                continue;
            };
            rebuilt.storage_used += entry.bytes;
            rebuilt.ledgers.insert(ledger, entry);
        }
        let mut index = self.lock();
        *index = rebuilt;
        self.recheck(&index);
        Ok(())
    }

    // ------------------------------------------------------------ reads ----

    /// Where a ledger ends, or `None` if this witness does not hold it.
    #[must_use]
    pub fn head(&self, ledger: LedgerId) -> Option<Head> {
        self.lock().ledgers.get(&ledger).map(Entry::head)
    }

    /// One page of a ledger's events from `since` inclusive.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::MissingEvent`] if an
    /// event file the index counts cannot be read.
    pub fn read_from(
        &self,
        ledger: LedgerId,
        since: u64,
        limit: usize,
    ) -> Result<Option<EventPage>> {
        let Some(page) = self.page(ledger, since, limit)? else {
            return Ok(None);
        };
        Ok(Some(EventPage {
            head_seq: page.report.summary.head_seq,
            events: page.events,
            more: page.more,
        }))
    }

    /// One page of a ledger's events, with the ledger's row beside them.
    ///
    /// # Errors
    ///
    /// As [`WitnessStorage::read_from`].
    pub fn page(&self, ledger: LedgerId, since: u64, limit: usize) -> Result<Option<StoredPage>> {
        let (report, head_seq) = {
            let index = self.lock();
            let Some(entry) = index.ledgers.get(&ledger) else {
                return Ok(None);
            };
            (entry.report(ledger, self.caps.fork_records), entry.head_seq)
        };
        let store = self.home.ledger(ledger);
        let mut events = Vec::new();
        let mut seq = since;
        while seq <= head_seq && events.len() < limit {
            events.push(store.read_event(seq)?);
            seq += 1;
        }
        Ok(Some(StoredPage {
            report,
            events,
            more: seq <= head_seq,
        }))
    }

    /// Stored ledgers by ascending ledger id.
    #[must_use]
    pub fn list(&self, offset: usize, limit: usize) -> Page<LedgerSummary> {
        let index = self.lock();
        let items: Vec<LedgerSummary> = index
            .ledgers
            .iter()
            .skip(offset)
            .take(limit)
            .map(|(ledger, entry)| entry.summary(*ledger, self.caps.fork_records))
            .collect();
        Page {
            more: offset + items.len() < index.ledgers.len(),
            items,
        }
    }

    /// Stored ledgers with their provenance and witness sets, by ascending
    /// ledger id.
    #[must_use]
    pub fn reports(&self, offset: usize, limit: usize) -> Page<LedgerReport> {
        let index = self.lock();
        let items: Vec<LedgerReport> = index
            .ledgers
            .iter()
            .skip(offset)
            .take(limit)
            .map(|(ledger, entry)| entry.report(*ledger, self.caps.fork_records))
            .collect();
        Page {
            more: offset + items.len() < index.ledgers.len(),
            items,
        }
    }

    /// One ledger's row, or `None` if this witness does not hold it.
    #[must_use]
    pub fn report(&self, ledger: LedgerId) -> Option<LedgerReport> {
        self.lock()
            .ledgers
            .get(&ledger)
            .map(|entry| entry.report(ledger, self.caps.fork_records))
    }

    /// Fork records, for one ledger or for every ledger, by ascending ledger
    /// id then sequence.
    #[must_use]
    pub fn forks(&self, ledger: Option<LedgerId>, offset: usize, limit: usize) -> Page<ForkRecord> {
        let index = self.lock();
        let all: Vec<&ForkRecord> = index
            .ledgers
            .iter()
            .filter(|(id, _)| ledger.is_none_or(|wanted| wanted == **id))
            .flat_map(|(_, entry)| entry.forks.iter())
            .collect();
        let items: Vec<ForkRecord> = all
            .iter()
            .skip(offset)
            .take(limit)
            .map(|record| (*record).clone())
            .collect();
        Page {
            more: offset + items.len() < all.len(),
            items,
        }
    }

    /// What `GET /api/node` counts.
    #[must_use]
    pub fn totals(&self) -> Totals {
        let index = self.lock();
        Totals {
            ledger_count: index.ledgers.len() as u64,
            fork_count: index
                .ledgers
                .values()
                .map(|entry| entry.forks.len() as u64)
                .sum(),
            storage_used: index.storage_used,
        }
    }

    // ----------------------------------------------------------- pushes ----

    /// Offers events for one ledger, applying the push semantics of proposal
    /// 001 section 5.
    ///
    /// # Errors
    ///
    /// Returns `NOT_ADMITTED` when [`WitnessStorage::admits`] refuses,
    /// `MALFORMED` for a gap, `TOO_LARGE` for a cap,
    /// `FORK` when a pushed event contends with a stored one, and `INVALID`
    /// when an event does not verify, with the valid prefix stored first.
    pub fn push(&self, ledger: LedgerId, events: &[Vec<u8>], provenance: Provenance) -> PushResult {
        let mut index = self.lock();
        let held = index.ledgers.contains_key(&ledger);

        if events.is_empty() {
            return match index.ledgers.get(&ledger) {
                Some(entry) => Ok(PushOutcome {
                    head_seq: entry.head_seq,
                    stored: 0,
                }),
                // Nothing to fold, so no state names a witness: an empty push
                // for a ledger this home does not store is refused, in the
                // words of the rule that refused it.
                None => {
                    Err(self.refusal(ledger, &LedgerState::default(), &self.witness_for_entries()))
                }
            };
        }

        // The run must be contiguous and ascending: anything else is the gap
        // of section 5, whichever end of the push it sits at.
        let first = seq_of(&events[0])?;
        for (offset, event) in events.iter().enumerate() {
            let want = first + offset as u64;
            let seq = seq_of(event)?;
            if seq != want {
                return Err(malformed(
                    seq,
                    format!("the push carries seq {seq} where seq {want} belongs"),
                ));
            }
        }

        if held {
            self.splice(&mut index, ledger, first, events, provenance)
        } else {
            self.ingest(&mut index, ledger, first, events, provenance)
        }
    }

    /// The first ingest of a ledger: full verification from nothing, then the
    /// admission rule, then the valid prefix.
    fn ingest(
        &self,
        index: &mut Index,
        ledger: LedgerId,
        first: u64,
        events: &[Vec<u8>],
        provenance: Provenance,
    ) -> PushResult {
        if first != 0 {
            return Err(malformed(
                first,
                "this witness holds no such ledger, so the push must start at seq 0",
            ));
        }

        let mut state = LedgerState::default();
        let (valid, violation) = apply_run(&mut state, events);
        if valid == 0 {
            let reason = violation.expect("an empty valid prefix carries a violation");
            return Err(StoreError::invalid(0, reason.to_string()));
        }
        // The seq-0 event's id is the ledger id, so a chain that hashes to
        // something else belongs to another ledger.
        if state.ledger() != Some(ledger) {
            return Err(StoreError::invalid(
                0,
                "the seq-0 event does not hash to the ledger this push names",
            ));
        }
        // Admission on the first push, where the stored state is empty: the
        // pushed state's witness set must name an identity this home witnesses
        // for and that identity must advertise this home, or this home must
        // hold a signing key for the ledger (proposal 006 sections 4 and 4.1).
        self.admit(ledger, None, &state)?;
        let stored = self.store_run(index, ledger, state, &events[..valid], provenance)?;
        match violation {
            Some(reason) => Err(StoreError::invalid(valid as u64, reason.to_string())),
            None => Ok(stored),
        }
    }

    /// A push against a ledger this witness holds: the overlap, then the
    /// suffix verified against the kept state.
    fn splice(
        &self,
        index: &mut Index,
        ledger: LedgerId,
        first: u64,
        events: &[Vec<u8>],
        provenance: Provenance,
    ) -> PushResult {
        let head_seq = index
            .ledgers
            .get(&ledger)
            .expect("the caller checked the ledger is held")
            .head_seq;
        if first > head_seq + 1 {
            return Err(malformed(
                first,
                format!("the ledger ends at seq {head_seq}, so seq {first} leaves a gap"),
            ));
        }

        // The overlap must be byte-identical, which is what makes a retry
        // idempotent; the event stored first at a sequence is never replaced.
        let store = self.home.ledger(ledger);
        let mut fresh = events.len();
        for (offset, event) in events.iter().enumerate() {
            let seq = first + offset as u64;
            if seq > head_seq {
                fresh = offset;
                break;
            }
            let stored = store.read_event(seq).map_err(unavailable)?;
            if stored != *event {
                return self.record_divergence(index, ledger, seq, &stored, event, provenance);
            }
        }

        let suffix = &events[fresh..];
        if suffix.is_empty() {
            return Ok(PushOutcome {
                head_seq,
                stored: 0,
            });
        }

        let stored_state = index
            .ledgers
            .get(&ledger)
            .expect("the ledger is held")
            .state
            .clone();
        let mut state = stored_state.clone();
        let (valid, violation) = apply_run(&mut state, suffix);
        // Admission again, now that both states are known. The stored state is
        // what admits the removal event itself: a controller who appends a
        // witness set dropping this home needs that event to reach it
        // (proposal 006 section 4).
        self.admit(ledger, Some(&stored_state), &state)?;
        let stored = self.store_run(index, ledger, state, &suffix[..valid], provenance)?;
        match violation {
            Some(reason) => Err(StoreError::invalid(
                first + (fresh + valid) as u64,
                reason.to_string(),
            )),
            None => Ok(stored),
        }
    }

    /// Whether this home takes a push for `ledger`, given the state it already
    /// stores and the state the push folds to (proposal 006 section 4).
    ///
    /// Four clauses, in order:
    ///
    /// 1. this home holds a signing key for `ledger`, so it controls it;
    /// 2. the stored state's witness set names an identity this home witnesses
    ///    for, which is what admits the very event that drops this home from
    ///    the set;
    /// 3. the pushed state's witness set names an identity this home witnesses
    ///    for **and** advertises this home, which is what admits a first push;
    /// 4. the retired tag-11 clause, gated on a non-empty `witness_for`, on
    ///    `accept_legacy_witness_config` and on either state's tag-11 list
    ///    naming this home's own endpoint id.
    ///
    /// Clause 3 alone answers to the advertisement invariant of section 4.1: an
    /// entry whose identity does not advertise this home stops taking ledgers
    /// this home does not store, and the ones it stores keep growing under
    /// clause 2.
    fn admit(
        &self,
        ledger: LedgerId,
        pre: Option<&LedgerState>,
        post: &LedgerState,
    ) -> std::result::Result<(), StoreError> {
        // Clause 1.
        if self.home.can_sign_for(ledger) {
            return Ok(());
        }
        let names = |state: &LedgerState, witnesses: &[IdentityId]| {
            state
                .witness_identities()
                .iter()
                .any(|witness| witnesses.contains(witness))
        };
        // Clause 2, over every entry: a ledger already stored under an identity
        // this home witnesses for keeps taking extensions.
        if pre.is_some_and(|pre| names(pre, &self.policy.witness_for)) {
            return Ok(());
        }
        // Clause 3, over the entries whose identity advertises this home.
        let entries = self.witness_for_entries();
        let advertising: Vec<IdentityId> = entries
            .iter()
            .filter(|entry| entry.advertised())
            .map(|entry| entry.identity)
            .collect();
        if names(post, &advertising) {
            return Ok(());
        }
        // Clause 4, twice gated and off by default.
        if self.legacy_admits(pre, post) {
            return Ok(());
        }
        Err(self.refusal(ledger, post, &entries))
    }

    /// The retired tag-11 clause of proposal 006 section 4.
    ///
    /// It holds only when `witness_for` is non-empty, `node.json` turns the
    /// switch on, and one of the two states carries a tag-11 `WitnessConfig`
    /// naming this home's own endpoint id. Gating on `witness_for` is what
    /// keeps the promise that a home witnessing for nobody takes no stranger's
    /// push, whatever the switch says.
    fn legacy_admits(&self, pre: Option<&LedgerState>, post: &LedgerState) -> bool {
        if self.policy.witness_for.is_empty() || !self.policy.accept_legacy_witness_config {
            return false;
        }
        let endpoint = self.endpoint();
        let listed = |state: &LedgerState| state.witness_endpoints().contains(&endpoint);
        pre.is_some_and(listed) || listed(post)
    }

    /// Why a push was not admitted, in the words of the rule that refused it.
    fn refusal(
        &self,
        ledger: LedgerId,
        post: &LedgerState,
        entries: &[WitnessForEntry],
    ) -> StoreError {
        if entries.is_empty() {
            return StoreError::not_admitted(format!(
                "this home witnesses for nobody, so it does not take pushes for {ledger}"
            ));
        }
        // A witness set that names an entry which is failing section 4.1 is the
        // one refusal an operator can act on, so it is named first.
        let blocked = entries.iter().find(|entry| {
            entry.gap.is_some() && post.witness_identities().contains(&entry.identity)
        });
        if let Some(entry) = blocked {
            let gap = entry.gap.expect("the entry was filtered on its gap");
            return StoreError::not_admitted(format!(
                "{ledger} names {} as a witness and this home witnesses for it, but {}, so this \
                 home takes no ledger it does not already store under it",
                entry.identity,
                gap.reason()
            ));
        }
        StoreError::not_admitted(format!(
            "the witness set of {ledger} names none of the {} identities this home witnesses for",
            entries.len()
        ))
    }

    /// Rechecks the advertisement invariant against `index` and records the
    /// verdicts (proposal 006 section 4.1).
    ///
    /// The caller holds the index, so the lock order is index then
    /// advertisement, everywhere.
    fn recheck(&self, index: &Index) {
        let mut advertised = self.advertisement();
        let endpoint = advertised.endpoint;
        advertised.entries = self
            .policy
            .witness_for
            .iter()
            .map(|identity| WitnessForEntry {
                identity: *identity,
                gap: gap_for(index, *identity, endpoint),
            })
            .collect();
    }

    fn advertisement(&self) -> MutexGuard<'_, Advertised> {
        self.advertised
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Writes a verified run of events and updates the index.
    ///
    /// The caps are checked here, before anything lands, so a push over a cap
    /// stores nothing. `state` is the fold after the run.
    fn store_run(
        &self,
        index: &mut Index,
        ledger: LedgerId,
        state: LedgerState,
        events: &[Vec<u8>],
        provenance: Provenance,
    ) -> PushResult {
        let held = index.ledgers.contains_key(&ledger);
        let head_seq = index.ledgers.get(&ledger).map(|entry| entry.head_seq);
        if events.is_empty() {
            return Ok(PushOutcome {
                head_seq: head_seq.unwrap_or(0),
                stored: 0,
            });
        }
        let bytes: u64 = events.iter().map(|event| event.len() as u64).sum();
        self.check_caps(index, ledger, events.len() as u64, bytes)?;

        let store = self.home.ledger(ledger);
        let start = head_seq.map_or(0, |seq| seq + 1);
        let mut batch = Vec::with_capacity(events.len());
        for (offset, event) in events.iter().enumerate() {
            let seq = start + offset as u64;
            let event_id = wire::signed_event_id(event)
                .ok_or_else(|| StoreError::invalid(seq, "the event carries no readable body"))?;
            batch.push(NewEvent {
                seq,
                event_id,
                bytes: event,
            });
        }
        let mut first_seen_ms = now_ms();
        let mut source_endpoint = None;
        if !held {
            let meta = store
                .note_first_seen(provenance.endpoint)
                .map_err(unavailable)?;
            first_seen_ms = meta.first_seen_ms;
            source_endpoint = meta.source_endpoint;
        }
        let written = store
            .append(&batch)
            .map_err(unavailable)?
            .expect("a non-empty append has a head");

        index.storage_used += bytes;
        let entry = index.ledgers.entry(ledger).or_insert_with(|| Entry {
            state: LedgerState::default(),
            head_seq: written.seq,
            head_event: written.event_id,
            bytes: 0,
            first_seen_ms,
            updated_ms: written.updated_ms,
            source_endpoint,
            forks: Vec::new(),
        });
        entry.state = state;
        entry.head_seq = written.seq;
        entry.head_event = written.event_id;
        entry.bytes += bytes;
        entry.updated_ms = written.updated_ms;
        // A longer copy of an identity this home witnesses for may have landed
        // the advertisement the invariant waits for, or dropped it (proposal
        // 006 section 4.1).
        if self.policy.witness_for.contains(&ledger) {
            let before = self.witness_for_entries();
            self.recheck(index);
            for (was, now) in before.iter().zip(self.witness_for_entries()) {
                if was.identity != now.identity || was.gap == now.gap {
                    continue;
                }
                match now.gap {
                    Some(gap) => warn!(
                        witness = %now.identity,
                        reason = gap.reason(),
                        "a longer copy of this identity stops it admitting new ledgers here"
                    ),
                    None => tracing::info!(
                        witness = %now.identity,
                        "this identity now advertises this home, which admits new ledgers under it"
                    ),
                }
            }
        }
        Ok(PushOutcome {
            head_seq: written.seq,
            stored: events.len() as u32,
        })
    }

    /// The caps of proposal 001 section 5, checked before anything is written.
    fn check_caps(
        &self,
        index: &Index,
        ledger: LedgerId,
        events: u64,
        bytes: u64,
    ) -> std::result::Result<(), StoreError> {
        let entry = index.ledgers.get(&ledger);
        let (held_events, held_bytes) =
            entry.map_or((0, 0), |entry| (entry.event_count(), entry.bytes));
        if held_events + events > self.caps.events_per_ledger {
            return Err(too_large(format!(
                "the ledger would hold {} events, over the {}-event cap",
                held_events + events,
                self.caps.events_per_ledger
            )));
        }
        if held_bytes + bytes > self.caps.bytes_per_ledger {
            return Err(too_large(format!(
                "the ledger would hold {} bytes, over the {}-byte cap",
                held_bytes + bytes,
                self.caps.bytes_per_ledger
            )));
        }
        if entry.is_none() && index.ledgers.len() >= self.caps.ledgers {
            return Err(too_large(format!(
                "this witness already holds its cap of {} ledgers",
                self.caps.ledgers
            )));
        }
        if index.storage_used + bytes > self.caps.storage_capacity {
            return Err(too_large(format!(
                "this witness would store {} bytes, over its {}-byte capacity",
                index.storage_used + bytes,
                self.caps.storage_capacity
            )));
        }
        Ok(())
    }

    /// A pushed event that differs from the one stored at its sequence.
    ///
    /// Only an event that fully verifies against the shared prefix is a fork;
    /// anything else is `INVALID` and is not stored (proposal 001 section 5).
    /// The stored event is kept either way.
    fn record_divergence(
        &self,
        index: &mut Index,
        ledger: LedgerId,
        seq: u64,
        kept: &[u8],
        conflicting: &[u8],
        provenance: Provenance,
    ) -> PushResult {
        let store = self.home.ledger(ledger);
        let prefix = self.folded_to(&store, seq).map_err(unavailable)?;
        let fork = match validate_fork_record(&prefix, kept, conflicting) {
            Ok(fork) => fork,
            Err(ForkError::Conflicting(violation)) => {
                return Err(StoreError::invalid(seq, violation.reason.to_string()));
            }
            Err(ForkError::SameEvent(_)) => {
                return Err(StoreError::invalid(
                    seq,
                    "the event differs from the stored one only outside its signed body",
                ));
            }
            Err(ForkError::EmptyPrefix) => {
                return Err(StoreError::invalid(
                    seq,
                    "a seq-0 event that differs from the stored one names another ledger",
                ));
            }
            Err(ForkError::Kept(violation)) => {
                return Err(StoreError::Unavailable(format!(
                    "the stored event at seq {seq} of {ledger} does not verify: {violation}"
                )));
            }
            // `ForkError` is non-exhaustive; a class this build does not know
            // is not evidence of a fork either.
            Err(error) => return Err(StoreError::invalid(seq, error.to_string())),
        };

        let entry = index
            .ledgers
            .get_mut(&ledger)
            .expect("the caller checked the ledger is held");
        let known = entry.forks.iter().any(|record| {
            record.seq == seq
                && wire::signed_event_id(&record.conflicting) == Some(fork.conflicting)
        });
        if !known && !entry.forks_truncated(self.caps.fork_records) {
            let record = ForkRecord {
                ledger,
                seq,
                kept: kept.to_vec(),
                conflicting: conflicting.to_vec(),
                observed_ms: now_ms(),
                source_endpoint: provenance.endpoint,
            };
            store
                .record_fork(seq, fork.conflicting, &wire::fork_entry(&record))
                .map_err(unavailable)?;
            entry.forks.push(record);
            entry.forks.sort_by_key(fork_order);
        }
        Err(StoreError::fork(
            seq,
            format!("another validly signed event is stored at seq {seq}"),
        ))
    }

    /// The state folded from the events before `seq`, which is the prefix both
    /// branches of a fork agree on.
    fn folded_to(&self, store: &LedgerStore, seq: u64) -> Result<LedgerState> {
        let mut state = LedgerState::default();
        for event in store.read_from(0, Some(usize::try_from(seq).unwrap_or(usize::MAX)))? {
            if state.apply(&event.bytes).is_err() {
                break;
            }
        }
        Ok(state)
    }

    fn lock(&self) -> MutexGuard<'_, Index> {
        // A panic while the index is held must not turn every later request
        // into a second panic; the index is a cache and `reload` rebuilds it.
        self.index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// What a push answers: the outcome, or the rejection the peer sees.
pub type PushResult = std::result::Result<PushOutcome, StoreError>;

/// Applies events to `state` until one fails, reporting how many were applied
/// and why the next one was not.
fn apply_run(state: &mut LedgerState, events: &[Vec<u8>]) -> (usize, Option<Reason>) {
    for (offset, event) in events.iter().enumerate() {
        if let Err(reason) = state.apply(event) {
            return (offset, Some(reason));
        }
    }
    (events.len(), None)
}

/// Fork records are ordered by sequence, then by the conflicting event's id, so
/// paging is stable and a restart does not reorder them.
fn fork_order(record: &ForkRecord) -> (u64, Option<EventId>) {
    (record.seq, wire::signed_event_id(&record.conflicting))
}

/// Reads one ledger directory into an index entry, or `None` when nothing in
/// it folds.
fn load_entry(store: &LedgerStore) -> Result<Option<Entry>> {
    let ledger = store.ledger_id();
    let stored = store.read_all()?;
    if stored.is_empty() {
        return Ok(None);
    }
    let mut state = LedgerState::default();
    let mut bytes = 0u64;
    let mut head_seq = 0u64;
    let mut head_event = EventId::from_bytes([0u8; 32]);
    for event in &stored {
        if let Err(reason) = state.apply(&event.bytes) {
            warn!(
                %ledger,
                seq = event.seq,
                %reason,
                "a stored event does not verify; the ledger is served up to the event before it"
            );
            break;
        }
        bytes += event.bytes.len() as u64;
        head_seq = event.seq;
        head_event = state.head().expect("an applied event is the head").event_id;
    }
    if state.is_empty() {
        warn!(%ledger, "no stored event of this ledger verifies; it is not served");
        return Ok(None);
    }
    let meta = store.meta()?;
    let updated_ms = store
        .cached_head()?
        .map_or_else(now_ms, |head| head.updated_ms);
    let mut forks = Vec::new();
    for file in store.forks()? {
        let raw = std::fs::read(&file.path).map_err(io_at(&file.path))?;
        match decode_fork_record(&raw) {
            Some(record) => forks.push(record),
            None => {
                warn!(path = %file.path.display(), "a fork record does not decode; skipping it");
            }
        }
    }
    forks.sort_by_key(fork_order);
    Ok(Some(Entry {
        state,
        head_seq,
        head_event,
        bytes,
        first_seen_ms: meta.as_ref().map_or(updated_ms, |meta| meta.first_seen_ms),
        updated_ms,
        source_endpoint: meta.and_then(|meta| meta.source_endpoint),
        forks,
    }))
}

/// Reads a `.fork` file, an encoded `mabel.v0.ForkRecord`.
///
/// Both events come back as the byte strings the file holds, never re-encoded
/// (proposal 001 section 3.1).
fn decode_fork_record(raw: &[u8]) -> Option<ForkRecord> {
    let fields = wire::fields(raw)?;
    let source = match wire::bytes(&fields, 6) {
        Some(bytes) => {
            let bytes: [u8; 32] = bytes.try_into().ok()?;
            Some(EndpointId::from_bytes(&bytes).ok()?)
        }
        None => None,
    };
    Some(ForkRecord {
        ledger: LedgerId::from_slice(wire::bytes(&fields, 1)?).ok()?,
        seq: wire::uint(&fields, 2),
        kept: wire::bytes(&fields, 3)?.to_vec(),
        conflicting: wire::bytes(&fields, 4)?.to_vec(),
        observed_ms: wire::uint(&fields, 5),
        source_endpoint: source,
    })
}

/// The `seq` an encoded `SignedEvent` declares.
fn seq_of(event: &[u8]) -> std::result::Result<u64, StoreError> {
    wire::signed_event_seq(event)
        .ok_or_else(|| malformed(0, "an event in the push carries no readable seq"))
}

/// A gap or an unreadable event: the code section 5 names for a push that does
/// not splice.
fn malformed(at_seq: u64, msg: impl Into<String>) -> StoreError {
    StoreError::Rejected(Rejection::at(RejectCode::Malformed, at_seq, msg))
}

/// A cap this witness enforces.
fn too_large(msg: impl Into<String>) -> StoreError {
    StoreError::Rejected(Rejection::new(RejectCode::TooLarge, msg))
}

/// Whether the local copy of `witness` advertises `endpoint`, and why not when
/// it does not (proposal 006 section 4.1).
///
/// The copy is the one this home stores: the kept chain, which a fork record
/// does not replace (proposal 001 section 5).
fn gap_for(index: &Index, witness: IdentityId, endpoint: EndpointId) -> Option<AdvertisementGap> {
    let Some(entry) = index.ledgers.get(&witness) else {
        return Some(AdvertisementGap::NoLocalCopy);
    };
    let endpoints = entry.state.endpoints();
    if endpoints.is_empty() {
        return Some(AdvertisementGap::AdvertisesNothing);
    }
    if !endpoints.contains(&endpoint) {
        return Some(AdvertisementGap::AdvertisesOtherEndpoints);
    }
    None
}

fn unavailable(error: StorageError) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

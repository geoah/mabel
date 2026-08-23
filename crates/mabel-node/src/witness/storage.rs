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
use mabel_core::{EventId, LedgerId, Reason, validate_fork_record};
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
    /// The witness set of the ledger's latest `WitnessConfig`.
    pub witnesses: Vec<EndpointId>,
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
            witnesses: self.state.witnesses().to_vec(),
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

/// A witness's ledgers, its folded-state cache and its caps.
#[derive(Debug)]
pub struct WitnessStorage {
    home: NodeHome,
    endpoint: EndpointId,
    caps: WitnessCaps,
    index: Mutex<Index>,
}

impl WitnessStorage {
    /// Opens a home and builds the index from the event files.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of reading the ledger directories.
    pub fn open(home: NodeHome, endpoint: EndpointId, caps: WitnessCaps) -> Result<Self> {
        let storage = Self {
            home,
            endpoint,
            caps,
            index: Mutex::new(Index::default()),
        };
        storage.reload()?;
        Ok(storage)
    }

    /// Opens a home with the caps `node.json` names.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`NodeHome::config`] and [`WitnessStorage::open`].
    pub fn open_from_config(home: NodeHome, endpoint: EndpointId) -> Result<Self> {
        let caps = WitnessCaps::from_config(&home.config()?);
        Self::open(home, endpoint, caps)
    }

    /// The home this witness stores into.
    #[must_use]
    pub fn home(&self) -> &NodeHome {
        &self.home
    }

    /// This witness's own endpoint id, which is what admission checks for.
    #[must_use]
    pub fn endpoint(&self) -> EndpointId {
        self.endpoint
    }

    /// The caps this witness enforces.
    #[must_use]
    pub fn caps(&self) -> WitnessCaps {
        self.caps
    }

    /// Rebuilds the folded-state cache from the event files.
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
        *self.lock() = rebuilt;
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
    /// Returns `NOT_ADMITTED` for a ledger this witness neither holds nor is
    /// named a witness of, `MALFORMED` for a gap, `TOO_LARGE` for a cap,
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
                None => Err(not_admitted(ledger)),
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
        // Admission: the folded witness set must name this witness (proposal
        // 001 section 5).
        if !state.witnesses().contains(&self.endpoint) {
            return Err(not_admitted(ledger));
        }
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

        let mut state = index
            .ledgers
            .get(&ledger)
            .expect("the ledger is held")
            .state
            .clone();
        let (valid, violation) = apply_run(&mut state, suffix);
        let stored = self.store_run(index, ledger, state, &suffix[..valid], provenance)?;
        match violation {
            Some(reason) => Err(StoreError::invalid(
                first + (fresh + valid) as u64,
                reason.to_string(),
            )),
            None => Ok(stored),
        }
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

fn not_admitted(ledger: LedgerId) -> StoreError {
    StoreError::not_admitted(format!(
        "this witness does not hold {ledger} and the pushed chain does not name it a witness"
    ))
}

fn unavailable(error: StorageError) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

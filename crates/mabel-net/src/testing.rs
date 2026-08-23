//! An in-memory [`Store`] and event fixtures.
//!
//! This is a test double, not a witness: it keeps everything in a map, and
//! its push rules are the minimum the protocol tests need. Real admission,
//! verification and fork recording are ticket 010's.

use std::collections::BTreeMap;
use std::sync::Mutex;

use mabel_core::proto::DeclaredKind;
use mabel_core::sign::{Position, Root, build_inception, build_trust_attestation};
use mabel_core::{EventId, IdentityId, LedgerId};
use tokio::sync::{Notify, RwLock, RwLockWriteGuard};

use crate::store::{
    EventPage, ForkRecord, Head, LedgerSummary, Page, Provenance, PushOutcome, Store, StoreError,
    StoreFuture,
};
use crate::wire::{signed_event_id, signed_event_seq};

/// The timestamp every fixture event carries.
pub const FIXTURE_MS: u64 = 1_700_000_000_000;

fn secret(seed: u8) -> iroh_base::SecretKey {
    iroh_base::SecretKey::from_bytes(&[seed; 32])
}

/// Builds a chain of `count` encoded `SignedEvent`s at sequences `0..count`.
///
/// The first is a person inception, the rest are trust attestations, which is
/// enough for a transport test: this crate never folds them.
pub fn sample_events(count: usize) -> Vec<Vec<u8>> {
    sample_chain(1, count).1
}

/// Builds a chain like [`sample_events`] and returns its ledger id too.
///
/// `seed` picks the signing key, so two seeds give two distinct ledgers.
pub fn sample_chain(seed: u8, count: usize) -> (LedgerId, Vec<Vec<u8>>) {
    let signer = secret(seed);
    let inception = build_inception(
        &signer,
        DeclaredKind::Person,
        Root::Raw {
            reserve_key: &secret(seed.wrapping_add(128)).public(),
        },
        [seed; 16],
        FIXTURE_MS,
    )
    .expect("the inception builds");
    let ledger: LedgerId = inception.event_id.into();

    let mut events = vec![inception.signed_event.clone()];
    let mut prev = inception.event_id;
    for index in 1..count {
        let built = build_trust_attestation(
            &signer,
            &Position {
                ledger,
                seq: index as u64,
                prev,
                prev_timestamp_ms: FIXTURE_MS,
            },
            IdentityId::from_bytes([index as u8; 32]),
            FIXTURE_MS + index as u64,
        )
        .expect("the attestation builds");
        prev = built.event_id;
        events.push(built.signed_event);
    }
    (ledger, events)
}

/// A ledger that really forked: two valid events at one sequence.
#[derive(Debug, Clone)]
pub struct SampleFork {
    /// The ledger both events claim.
    pub ledger: LedgerId,
    /// The events below [`SampleFork::seq`], which both branches share.
    pub prefix: Vec<Vec<u8>>,
    /// The sequence the two events collide at.
    pub seq: u64,
    /// The event a store saw first and kept, encoded.
    pub kept: Vec<u8>,
    /// The other event at that sequence, encoded.
    pub conflicting: Vec<u8>,
}

impl SampleFork {
    /// The chain a store holds: the shared prefix plus the kept event.
    pub fn stored(&self) -> Vec<Vec<u8>> {
        let mut events = self.prefix.clone();
        events.push(self.kept.clone());
        events
    }
}

/// Builds a ledger whose seq 1 holds two valid events, signed by the one key
/// that may append to it.
///
/// Both events pass the whole fold at seq 1, so this is a real fork and not a
/// forgery: a verifier must accept a record carrying these two.
pub fn sample_fork(seed: u8) -> SampleFork {
    let signer = secret(seed);
    let (ledger, prefix) = sample_chain(seed, 1);
    let inception = signed_event_id(&prefix[0]).expect("the inception is readable");
    let at = Position {
        ledger,
        seq: 1,
        prev: inception,
        prev_timestamp_ms: FIXTURE_MS,
    };
    let branch = |subject: u8| {
        build_trust_attestation(
            &signer,
            &at,
            IdentityId::from_bytes([subject; 32]),
            FIXTURE_MS + 1,
        )
        .expect("the attestation builds")
        .signed_event
    };
    SampleFork {
        ledger,
        prefix,
        seq: 1,
        kept: branch(0xaa),
        conflicting: branch(0xbb),
    }
}

/// One call a [`MemoryStore`] served, so a test can assert what the server
/// clamped before it reached the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// [`Store::head`].
    Head {
        /// The ledger asked about.
        ledger: LedgerId,
    },
    /// [`Store::read_from`].
    Read {
        /// The ledger asked about.
        ledger: LedgerId,
        /// The first sequence wanted.
        since: u64,
        /// The clamped count.
        limit: usize,
    },
    /// [`Store::push`].
    Push {
        /// The ledger the events belong to.
        ledger: LedgerId,
        /// How many events arrived.
        count: usize,
        /// Who sent them.
        provenance: Provenance,
    },
    /// [`Store::list`].
    List {
        /// How many ledgers to skip.
        offset: usize,
        /// The clamped count.
        limit: usize,
    },
    /// [`Store::forks`].
    Forks {
        /// The ledger asked about.
        ledger: Option<LedgerId>,
        /// How many records to skip.
        offset: usize,
        /// The clamped count.
        limit: usize,
    },
}

#[derive(Debug)]
struct Stored {
    declared_kind: DeclaredKind,
    events: Vec<Vec<u8>>,
    first_seen_ms: u64,
    updated_ms: u64,
    forks: Vec<ForkRecord>,
}

impl Stored {
    fn empty() -> Self {
        Self {
            declared_kind: DeclaredKind::Person,
            events: Vec::new(),
            first_seen_ms: FIXTURE_MS,
            updated_ms: FIXTURE_MS,
            forks: Vec::new(),
        }
    }
}

/// A [`Store`] backed by a map.
#[derive(Debug, Default)]
pub struct MemoryStore {
    ledgers: Mutex<BTreeMap<LedgerId, Stored>>,
    calls: Mutex<Vec<Call>>,
    gate: RwLock<()>,
    entered: Notify,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a ledger's events, replacing anything already there.
    pub fn insert(&self, ledger: LedgerId, events: Vec<Vec<u8>>) {
        let mut ledgers = self.ledgers.lock().expect("poisoned");
        ledgers.insert(
            ledger,
            Stored {
                events,
                ..Stored::empty()
            },
        );
    }

    /// Records a fork for a ledger the store already holds.
    pub fn insert_fork(&self, record: ForkRecord) {
        let mut ledgers = self.ledgers.lock().expect("poisoned");
        if let Some(stored) = ledgers.get_mut(&record.ledger) {
            stored.forks.push(record);
        }
    }

    /// Every call the store has served, in order.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("poisoned").clone()
    }

    /// The events currently stored for a ledger.
    pub fn events(&self, ledger: LedgerId) -> Vec<Vec<u8>> {
        self.ledgers
            .lock()
            .expect("poisoned")
            .get(&ledger)
            .map(|stored| stored.events.clone())
            .unwrap_or_default()
    }

    /// Blocks every read until the returned guard is dropped, so a test can
    /// hold a server's verification permit without sleeping.
    pub async fn hold_reads(&self) -> RwLockWriteGuard<'_, ()> {
        self.gate.write().await
    }

    /// Resolves once a read has begun. Paired with [`MemoryStore::hold_reads`]
    /// this pins the moment a request owns a verification permit.
    pub async fn read_started(&self) {
        self.entered.notified().await;
    }

    fn record(&self, call: Call) {
        self.calls.lock().expect("poisoned").push(call);
    }

    async fn enter_read(&self) {
        self.entered.notify_one();
        drop(self.gate.read().await);
    }
}

fn head_of(stored: &Stored) -> Option<Head> {
    let last = stored.events.last()?;
    Some(Head {
        head_seq: signed_event_seq(last)?,
        head_event: signed_event_id(last)?,
        updated_ms: stored.updated_ms,
    })
}

fn summary_of(ledger: LedgerId, stored: &Stored) -> LedgerSummary {
    let head = head_of(stored);
    LedgerSummary {
        ledger,
        declared_kind: stored.declared_kind,
        head_seq: head.map_or(0, |head| head.head_seq),
        head_event: head.map_or(EventId::from_bytes([0u8; 32]), |head| head.head_event),
        event_count: stored.events.len() as u64,
        first_seen_ms: stored.first_seen_ms,
        updated_ms: stored.updated_ms,
        fork_count: stored.forks.len() as u32,
        forks_truncated: false,
    }
}

impl Store for MemoryStore {
    fn head(&self, ledger: LedgerId) -> StoreFuture<'_, Option<Head>> {
        Box::pin(async move {
            self.record(Call::Head { ledger });
            self.enter_read().await;
            let ledgers = self.ledgers.lock().expect("poisoned");
            Ok(ledgers.get(&ledger).and_then(head_of))
        })
    }

    fn read_from(
        &self,
        ledger: LedgerId,
        since: u64,
        limit: usize,
    ) -> StoreFuture<'_, Option<EventPage>> {
        Box::pin(async move {
            self.record(Call::Read {
                ledger,
                since,
                limit,
            });
            self.enter_read().await;
            let ledgers = self.ledgers.lock().expect("poisoned");
            let Some(stored) = ledgers.get(&ledger) else {
                return Ok(None);
            };
            let head_seq = head_of(stored).map_or(0, |head| head.head_seq);
            let start = stored
                .events
                .iter()
                .position(|event| signed_event_seq(event).unwrap_or(0) >= since)
                .unwrap_or(stored.events.len());
            let events: Vec<Vec<u8>> = stored.events[start..].iter().take(limit).cloned().collect();
            Ok(Some(EventPage {
                more: start + events.len() < stored.events.len(),
                events,
                head_seq,
            }))
        })
    }

    fn push(
        &self,
        ledger: LedgerId,
        events: Vec<Vec<u8>>,
        provenance: Provenance,
    ) -> StoreFuture<'_, PushOutcome> {
        Box::pin(async move {
            self.record(Call::Push {
                ledger,
                count: events.len(),
                provenance,
            });
            let mut ledgers = self.ledgers.lock().expect("poisoned");
            let stored = ledgers.entry(ledger).or_insert_with(Stored::empty);
            let mut newly = 0u32;
            for event in events {
                let seq = signed_event_seq(&event)
                    .ok_or_else(|| StoreError::invalid(0, "an event carries no readable seq"))?;
                let index = usize::try_from(seq)
                    .map_err(|_| StoreError::invalid(seq, "seq is past what this store holds"))?;
                match index.cmp(&stored.events.len()) {
                    std::cmp::Ordering::Less => {
                        if stored.events[index] != event {
                            return Err(StoreError::fork(seq, "a different event is stored here"));
                        }
                    }
                    std::cmp::Ordering::Equal => {
                        stored.events.push(event);
                        newly += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        return Err(StoreError::invalid(seq, "the push leaves a gap"));
                    }
                }
            }
            stored.updated_ms = FIXTURE_MS + u64::from(newly);
            Ok(PushOutcome {
                head_seq: head_of(stored).map_or(0, |head| head.head_seq),
                stored: newly,
            })
        })
    }

    fn list(&self, offset: usize, limit: usize) -> StoreFuture<'_, Page<LedgerSummary>> {
        Box::pin(async move {
            self.record(Call::List { offset, limit });
            self.enter_read().await;
            let ledgers = self.ledgers.lock().expect("poisoned");
            let items: Vec<LedgerSummary> = ledgers
                .iter()
                .skip(offset)
                .take(limit)
                .map(|(ledger, stored)| summary_of(*ledger, stored))
                .collect();
            Ok(Page {
                more: offset + items.len() < ledgers.len(),
                items,
            })
        })
    }

    fn forks(
        &self,
        ledger: Option<LedgerId>,
        offset: usize,
        limit: usize,
    ) -> StoreFuture<'_, Page<ForkRecord>> {
        Box::pin(async move {
            self.record(Call::Forks {
                ledger,
                offset,
                limit,
            });
            self.enter_read().await;
            let ledgers = self.ledgers.lock().expect("poisoned");
            let all: Vec<ForkRecord> = ledgers
                .iter()
                .filter(|(id, _)| ledger.is_none_or(|wanted| wanted == **id))
                .flat_map(|(_, stored)| stored.forks.iter().cloned())
                .collect();
            let items: Vec<ForkRecord> = all.iter().skip(offset).take(limit).cloned().collect();
            Ok(Page {
                more: offset + items.len() < all.len(),
                items,
            })
        })
    }
}

//! The read-only [`Store`] a wallet serves `mabel/ledger/0` from.
//!
//! A wallet publishes what it holds so a verifier can fetch it, and accepts
//! nothing: replication into a wallet is a `sync fetch` the operator asks for,
//! never a push a stranger makes. A `Push` therefore answers `NOT_ADMITTED`
//! (proposal 001 sections 2 and 5).
//!
//! This is a thin adapter over the storage of ticket 007 and shares nothing
//! with the witness store: a wallet keeps no folded-state cache and records no
//! fork records.

use std::sync::Arc;

use mabel_core::LedgerId;
use mabel_net::store::{
    EventPage, ForkRecord, Head, LedgerSummary, Page, Provenance, PushOutcome, Store, StoreError,
    StoreFuture,
};

use crate::error::StorageError;
use crate::home::NodeHome;
use crate::ledger::LedgerStore;
use crate::wallet::ledger::LoadedLedger;

/// A read-only store over one wallet's home.
#[derive(Debug, Clone)]
pub struct WalletReadStore {
    home: Arc<NodeHome>,
}

impl WalletReadStore {
    /// A store over `home`.
    #[must_use]
    pub fn new(home: NodeHome) -> Self {
        Self {
            home: Arc::new(home),
        }
    }

    /// Runs blocking file work off the reactor.
    fn blocking<T, F>(&self, work: F) -> StoreFuture<'_, T>
    where
        T: Send + 'static,
        F: FnOnce(&NodeHome) -> Result<T, StoreError> + Send + 'static,
    {
        let home = self.home.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || work(&home)).await {
                Ok(result) => result,
                Err(error) => Err(StoreError::Unavailable(format!(
                    "the storage task did not finish: {error}"
                ))),
            }
        })
    }
}

impl Store for WalletReadStore {
    fn head(&self, ledger: LedgerId) -> StoreFuture<'_, Option<Head>> {
        self.blocking(move |home| {
            let store = home.ledger(ledger);
            Ok(store.head().map_err(unavailable)?.map(|head| Head {
                head_seq: head.seq,
                head_event: head.event_id,
                updated_ms: head.updated_ms,
            }))
        })
    }

    fn read_from(
        &self,
        ledger: LedgerId,
        since: u64,
        limit: usize,
    ) -> StoreFuture<'_, Option<EventPage>> {
        self.blocking(move |home| {
            let store = home.ledger(ledger);
            let Some(head) = store.head().map_err(unavailable)? else {
                return Ok(None);
            };
            let events = store
                .read_from(since, Some(limit))
                .map_err(unavailable)?
                .into_iter()
                .map(|event| event.bytes)
                .collect::<Vec<Vec<u8>>>();
            let last = since + events.len() as u64;
            Ok(Some(EventPage {
                more: last <= head.seq,
                events,
                head_seq: head.seq,
            }))
        })
    }

    fn push(
        &self,
        _ledger: LedgerId,
        _events: Vec<Vec<u8>>,
        _provenance: Provenance,
    ) -> StoreFuture<'_, PushOutcome> {
        Box::pin(async {
            Err(StoreError::not_admitted(
                "this node is a wallet and stores no pushed ledger; fetch from it instead",
            ))
        })
    }

    fn list(&self, offset: usize, limit: usize) -> StoreFuture<'_, Page<LedgerSummary>> {
        self.blocking(move |home| {
            let ledgers = home.ledgers().map_err(unavailable)?;
            let more = ledgers.len() > offset.saturating_add(limit);
            let mut items = Vec::new();
            for ledger in ledgers.into_iter().skip(offset).take(limit) {
                if let Some(summary) = summary(&home.ledger(ledger)).map_err(unavailable)? {
                    items.push(summary);
                }
            }
            Ok(Page { items, more })
        })
    }

    fn forks(
        &self,
        _ledger: Option<LedgerId>,
        _offset: usize,
        _limit: usize,
    ) -> StoreFuture<'_, Page<ForkRecord>> {
        Box::pin(async { Ok(Page::default()) })
    }
}

/// One row of a wallet's `List`, folded from the stored events.
///
/// A wallet keeps no index, so this reads the ledger; a wallet holds a handful
/// of ledgers, not the ten thousand a witness caps at.
fn summary(store: &LedgerStore) -> Result<Option<LedgerSummary>, StorageError> {
    let Some(head) = store.head()? else {
        return Ok(None);
    };
    let meta = store.meta()?.unwrap_or_default();
    let loaded = LoadedLedger::fold(
        store.ledger_id(),
        store
            .read_all()?
            .into_iter()
            .map(|event| event.bytes)
            .collect(),
    );
    Ok(Some(LedgerSummary {
        ledger: store.ledger_id(),
        declared_kind: loaded
            .state
            .declared_kind()
            .unwrap_or(mabel_core::proto::DeclaredKind::Person),
        head_seq: head.seq,
        head_event: head.event_id,
        event_count: loaded.event_count(),
        first_seen_ms: meta.first_seen_ms,
        updated_ms: head.updated_ms,
        fork_count: 0,
        forks_truncated: false,
    }))
}

fn unavailable(error: StorageError) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

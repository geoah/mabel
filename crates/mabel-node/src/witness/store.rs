//! The [`Store`] the sync server answers `mabel/ledger/0` from.
//!
//! Every method hands the blocking file work of [`WitnessStorage`] to
//! `tokio::task::spawn_blocking`, so a push that folds a chain and fsyncs
//! event files never runs on a reactor thread.

use std::sync::Arc;

use mabel_core::LedgerId;
use mabel_net::store::{
    EventPage, ForkRecord, Head, LedgerSummary, Page, Provenance, PushOutcome, Store, StoreError,
    StoreFuture,
};

use crate::error::StorageError;
use crate::witness::storage::WitnessStorage;

/// A [`Store`] over one witness's home.
#[derive(Debug, Clone)]
pub struct WitnessStore {
    storage: Arc<WitnessStorage>,
}

impl WitnessStore {
    /// A store over `storage`.
    #[must_use]
    pub fn new(storage: Arc<WitnessStorage>) -> Self {
        Self { storage }
    }

    /// The storage this store serves, which the HTTP service shares.
    #[must_use]
    pub fn storage(&self) -> &Arc<WitnessStorage> {
        &self.storage
    }

    /// Runs blocking storage work off the reactor.
    fn blocking<T, F>(&self, work: F) -> StoreFuture<'_, T>
    where
        T: Send + 'static,
        F: FnOnce(&WitnessStorage) -> Result<T, StoreError> + Send + 'static,
    {
        let storage = self.storage.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || work(&storage)).await {
                Ok(result) => result,
                Err(error) => Err(StoreError::Unavailable(format!(
                    "the storage task did not finish: {error}"
                ))),
            }
        })
    }
}

impl From<Arc<WitnessStorage>> for WitnessStore {
    fn from(storage: Arc<WitnessStorage>) -> Self {
        Self::new(storage)
    }
}

impl Store for WitnessStore {
    fn head(&self, ledger: LedgerId) -> StoreFuture<'_, Option<Head>> {
        self.blocking(move |storage| Ok(storage.head(ledger)))
    }

    fn read_from(
        &self,
        ledger: LedgerId,
        since: u64,
        limit: usize,
    ) -> StoreFuture<'_, Option<EventPage>> {
        self.blocking(move |storage| storage.read_from(ledger, since, limit).map_err(unavailable))
    }

    fn push(
        &self,
        ledger: LedgerId,
        events: Vec<Vec<u8>>,
        provenance: Provenance,
    ) -> StoreFuture<'_, PushOutcome> {
        self.blocking(move |storage| storage.push(ledger, &events, provenance))
    }

    fn list(&self, offset: usize, limit: usize) -> StoreFuture<'_, Page<LedgerSummary>> {
        self.blocking(move |storage| Ok(storage.list(offset, limit)))
    }

    fn forks(
        &self,
        ledger: Option<LedgerId>,
        offset: usize,
        limit: usize,
    ) -> StoreFuture<'_, Page<ForkRecord>> {
        self.blocking(move |storage| Ok(storage.forks(ledger, offset, limit)))
    }
}

fn unavailable(error: StorageError) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

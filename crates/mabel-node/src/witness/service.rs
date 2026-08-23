//! The witness HTTP surface, answered from the same storage the sync server
//! serves.
//!
//! Read-only: a witness signs nothing and holds no identity keys (proposal 001
//! section 2), so every method here reads the index and the event files and
//! renders the documents the fixtures under `contracts/http/` freeze. The
//! handlers in [`crate::api`] decide nothing.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use mabel_core::LedgerId;
use mabel_net::store::ForkRecord as StoredFork;

use crate::api::documents::{
    ForkList, ForkRecord, Id, LedgerEntry, LedgerList, LedgerPage, LedgerView, Relay, Role,
    WitnessNode,
};
use crate::api::error::ServiceError;
use crate::api::service::{
    EventPageRequest, ForkQuery, PageRequest, ServiceFuture, WitnessService,
};
use crate::config::RelayMode;
use crate::error::StorageError;
use crate::witness::events::{self, id_of};
use crate::witness::storage::{LedgerReport, WitnessStorage};

/// The version `GET /api/node` reports.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The sentence a fork record carries for a person (proposal 001 section 5,
/// flag W).
#[must_use]
pub fn fork_statement(ledger: &Id, seq: u64) -> String {
    format!(
        "two distinct validly signed events exist at seq {seq} of {ledger}, produced by whoever held signing authority there; this is evidence of equivocation or of a lost race between honest controllers"
    )
}

/// The witness API over one [`WitnessStorage`].
#[derive(Debug)]
pub struct WitnessReadService {
    storage: Arc<WitnessStorage>,
    http_bind: SocketAddr,
    relay: Relay,
}

impl WitnessReadService {
    /// A service over `storage`, reporting `http_bind` and `relay` as
    /// `GET /api/node`.
    #[must_use]
    pub fn new(storage: Arc<WitnessStorage>, http_bind: SocketAddr, relay: RelayMode) -> Self {
        Self {
            storage,
            http_bind,
            relay: match relay {
                RelayMode::N0 => Relay::N0,
                RelayMode::Disabled => Relay::Disabled,
            },
        }
    }

    /// The storage this service reads, which the sync server shares.
    #[must_use]
    pub fn storage(&self) -> &Arc<WitnessStorage> {
        &self.storage
    }

    /// Runs blocking storage work off the reactor.
    fn blocking<T, F>(&self, work: F) -> ServiceFuture<'_, T>
    where
        T: Send + 'static,
        F: FnOnce(&WitnessStorage) -> Result<T, ServiceError> + Send + 'static,
    {
        let storage = self.storage.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || work(&storage)).await {
                Ok(result) => result,
                Err(error) => Err(unavailable(&format!(
                    "the storage task did not finish: {error}"
                ))),
            }
        })
    }
}

impl WitnessService for WitnessReadService {
    fn node(&self) -> ServiceFuture<'_, WitnessNode> {
        let http_bind = self.http_bind;
        let relay = self.relay;
        self.blocking(move |storage| {
            let totals = storage.totals();
            Ok(WitnessNode {
                role: Role::Witness,
                endpoint_id: id_of(storage.endpoint().as_bytes()),
                http_bind,
                relay,
                // A witness pushes to nobody.
                witnesses: Vec::new(),
                storage_capacity: storage.caps().storage_capacity,
                storage_used: totals.storage_used,
                ledger_count: totals.ledger_count,
                fork_count: totals.fork_count,
                version: VERSION.to_owned(),
            })
        })
    }

    fn ledgers(&self, page: PageRequest) -> ServiceFuture<'_, LedgerList> {
        self.blocking(move |storage| {
            let found = storage.reports(page.offset as usize, page.limit as usize);
            Ok(LedgerList {
                offset: page.offset,
                limit: page.limit,
                more: found.more,
                entries: found.items.iter().map(ledger_entry).collect(),
            })
        })
    }

    fn ledger(&self, ledger_id: Id) -> ServiceFuture<'_, LedgerView> {
        self.blocking(move |storage| {
            let report = storage
                .report(parse_ledger(&ledger_id)?)
                .ok_or_else(|| not_held(&ledger_id))?;
            Ok(LedgerView {
                entry: ledger_entry(&report),
                witnesses: report
                    .witnesses
                    .iter()
                    .map(|witness| id_of(witness.as_bytes()))
                    .collect(),
            })
        })
    }

    fn ledger_events(
        &self,
        ledger_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage> {
        self.blocking(move |storage| {
            let ledger = parse_ledger(&ledger_id)?;
            let found = storage
                .page(ledger, page.since, page.limit as usize)
                .map_err(storage_error)?
                .ok_or_else(|| not_held(&ledger_id))?;
            let summary = &found.report.summary;
            let mut events = Vec::with_capacity(found.events.len());
            for bytes in &found.events {
                events.push(events::event_document(bytes)?);
            }
            Ok(LedgerPage {
                ledger_id,
                declared_kind: events::declared_kind(summary.declared_kind),
                since: page.since,
                limit: page.limit,
                head_seq: summary.head_seq,
                head_event: id_of(summary.head_event.as_bytes()),
                event_count: summary.event_count,
                more: found.more,
                events,
            })
        })
    }

    fn forks(&self, query: ForkQuery) -> ServiceFuture<'_, ForkList> {
        self.blocking(move |storage| {
            let ledger = match &query.ledger_id {
                Some(id) => Some(parse_ledger(id)?),
                None => None,
            };
            let page = query.page;
            let found = storage.forks(ledger, page.offset as usize, page.limit as usize);
            let mut entries = Vec::with_capacity(found.items.len());
            for record in &found.items {
                entries.push(fork_document(record)?);
            }
            Ok(ForkList {
                offset: page.offset,
                limit: page.limit,
                more: found.more,
                entries,
            })
        })
    }
}

/// One row of `GET /api/ledgers`.
fn ledger_entry(report: &LedgerReport) -> LedgerEntry {
    let summary = &report.summary;
    LedgerEntry {
        ledger_id: id_of(summary.ledger.as_bytes()),
        declared_kind: events::declared_kind(summary.declared_kind),
        head_seq: summary.head_seq,
        head_event: id_of(summary.head_event.as_bytes()),
        event_count: summary.event_count,
        first_seen_ms: summary.first_seen_ms,
        updated_ms: summary.updated_ms,
        fork_count: u64::from(summary.fork_count),
        forks_truncated: summary.forks_truncated,
        source_endpoint: endpoint_id(report.source_endpoint),
    }
}

/// One record of `GET /api/forks`.
fn fork_document(record: &StoredFork) -> Result<ForkRecord, ServiceError> {
    let ledger_id = id_of(record.ledger.as_bytes());
    Ok(ForkRecord {
        statement: fork_statement(&ledger_id, record.seq),
        ledger_id,
        seq: record.seq,
        observed_ms: record.observed_ms,
        source_endpoint: endpoint_id(record.source_endpoint),
        kept: events::event_document(&record.kept)?,
        conflicting: events::event_document(&record.conflicting)?,
    })
}

/// An endpoint id, or the all-zero id when provenance was never recorded.
///
/// The documents have no null here: `source_endpoint` is a string in every
/// fixture (`contracts/README.md`, "Nullability").
fn endpoint_id(endpoint: Option<iroh_base::EndpointId>) -> Id {
    endpoint.map_or_else(|| id_of(&[0u8; 32]), |endpoint| id_of(endpoint.as_bytes()))
}

/// The ledger a validated document id names.
fn parse_ledger(ledger_id: &Id) -> Result<LedgerId, ServiceError> {
    ledger_id.as_str().parse::<LedgerId>().map_err(|error| {
        ServiceError::schema(
            "malformed_ledger_id",
            format!("ledger id is not 52 base32 characters: {error}"),
        )
        .with_detail("value", ledger_id.as_str())
    })
}

/// The 404 the fixtures pin for a ledger this witness does not hold.
fn not_held(ledger_id: &Id) -> ServiceError {
    ServiceError::usage(
        "ledger_not_held",
        format!("this witness does not hold {ledger_id}"),
    )
    .with_detail("ledger_id", ledger_id.as_str())
    .with_status(StatusCode::NOT_FOUND)
}

/// A storage failure the caller cannot fix.
///
/// The table in `contracts/README.md` assigns no code to an internal failure,
/// so a malformed file is code 10 and everything else is code 50 with a 500:
/// the state on disk is not what the index says it is.
fn storage_error(error: StorageError) -> ServiceError {
    let message = error.to_string();
    match &error {
        StorageError::Json { .. }
        | StorageError::MalformedKey { .. }
        | StorageError::MalformedEvent { .. } => ServiceError::schema("malformed_file", message),
        StorageError::InsecurePermissions { path, mode } => ServiceError::permissions(
            "insecure_key_permissions",
            format!("{} is mode {mode:04o}", path.display()),
        ),
        _ => unavailable(&message),
    }
}

fn unavailable(message: &str) -> ServiceError {
    ServiceError::state("storage_unavailable", message)
        .with_status(StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod tests {
    use super::fork_statement;
    use crate::api::documents::Id;
    use crate::api::stub::Fixture;

    /// The sentence is pinned by `contracts/http/witness-get-forks.json`, so
    /// it is built here for that fixture's own ledger and sequence and
    /// compared verbatim.
    #[test]
    fn the_fork_statement_matches_the_fixture_verbatim() {
        let response = Fixture::named("witness-get-forks.json").response();
        let entry = &response["entries"][0];
        let ledger = Id::parse(entry["ledger_id"].as_str().expect("a ledger id"))
            .expect("the fixture id parses");
        let seq = entry["seq"].as_u64().expect("a seq");
        assert_eq!(
            fork_statement(&ledger, seq),
            entry["statement"].as_str().expect("a statement")
        );
    }
}

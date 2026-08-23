//! The trust graph: crawl outward from this node's identities, keep what was
//! seen as a generation, and answer "how do I know this identity" from it
//! (decision 016, proposal 003 section 3, ticket 025).
//!
//! Four pieces:
//!
//! - [`LedgerFetcher`] reads one frontier ledger from every source that might
//!   hold it, in the order proposal 003 section 3 fixes. [`NetLedgerFetcher`]
//!   is the real one; [`StubFetcher`] answers from a table so the crawler's
//!   tests are offline.
//! - [`crawl`] walks breadth-first from every local identity under the caps,
//!   verifying in memory and writing nothing.
//! - [`GraphStore`] puts a [`Generation`] under `graph/generations/<sync_id>/`
//!   and swaps `graph/current.json` with one rename.
//! - [`Generation`] is the reader: nodes, shortest paths, reverse edges and
//!   staleness.
//!
//! Three rules run through all of it. No stranger's ledger is stored: the
//! crawl folds a candidate in memory and keeps a summary, so `ledgers/` stays
//! the ledgers this node controls or fetched deliberately. Nothing is
//! resolved silently: two sources that disagree are both recorded on the
//! node. And every answer says how it was reached and when: a path is the
//! shortest **in this crawl**, a reverse list is who **this crawl** saw, and
//! both go stale 24 hours after the sync.
//!
//! ```no_run
//! use mabel_node::graph::{CrawlOptions, GraphStore, StubFetcher, crawl};
//! use mabel_node::{HomeOptions, NodeHome};
//!
//! # async fn run() -> Result<(), mabel_node::StorageError> {
//! let home = NodeHome::open("/tmp/wallet", HomeOptions::default())?;
//! let roots = home.identities()?;
//! let generation = crawl(&roots, &CrawlOptions::new(), &StubFetcher::new()).await;
//! GraphStore::in_home(&home).publish(&generation)?;
//! # Ok(())
//! # }
//! ```

mod crawl;
mod fetcher;
mod model;
mod store;
mod stub;

#[cfg(test)]
mod tests;

pub use crawl::{
    CrawlOptions, DEFAULT_DEPTH, IN_FLIGHT, MAX_DEPTH, MAX_FETCHES, MAX_NODES, MIN_DEPTH,
    RUN_BUDGET, crawl, mint_sync_id,
};
pub use fetcher::{
    FetchFuture, FetchOutcome, LedgerFetcher, LedgerSummary, NetLedgerFetcher, PER_FETCH_TIMEOUT,
    PlannedSource, TrustEdge, ledger_witness_sources, plan_sources, record_hint,
};
pub use model::{
    DiscoveredVia, Equivocation, EquivocationBranch, FetchSource, GraphEdge, GraphNode, GraphPath,
    GraphSummary, NodeStatus, PathHop, ReverseEdge, ReverseEdges, RootDepth, STALE_AFTER_MS,
    TruncatedBy, is_stale,
};
pub use store::{
    CURRENT_FILE, CurrentPointer, GENERATIONS_DIR, GRAPH_DIR, Generation, GraphStore,
    KEPT_GENERATIONS, MAX_PATHS, NODES_DIR, SUMMARY_FILE,
};
pub use stub::{STUB_FETCHED_AT_MS, StubFetcher, stub_attestation, stub_head, stub_identity};

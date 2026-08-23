//! The witness runtime: verify, then store, then serve reads (proposal 001
//! section 5, ticket 010).
//!
//! A witness is a passive replica. It holds no identity keys and signs nothing
//! (section 2), it accepts a `Push` only for a ledger it already holds or whose
//! folded `WitnessConfig` names its own `EndpointId`, and it stores only what
//! the fold accepted.
//!
//! Three pieces sit over one home:
//!
//! - [`WitnessStorage`] holds the ledgers, the folded-state cache and the fork
//!   records, and owns every rule about what may be stored.
//! - [`WitnessStore`] is the [`mabel_net::Store`] the sync server answers
//!   `mabel/ledger/0` from.
//! - [`WitnessReadService`] is the read-only HTTP surface of section 10.
//!
//! ```no_run
//! use mabel_node::witness::{WitnessOptions, WitnessRuntime};
//! use mabel_node::{HomeOptions, NodeHome};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let home = NodeHome::open("/tmp/witness", HomeOptions::default())?;
//! let witness = WitnessRuntime::start(home, WitnessOptions::default()).await?;
//! println!("{} on {}", witness.endpoint_id(), witness.http_address());
//! witness.serve().await
//! # }
//! ```

pub(crate) mod events;
mod runtime;
mod service;
mod storage;
mod store;

pub use runtime::{WitnessOptions, WitnessRuntime};
pub use service::{WitnessReadService, fork_statement};
pub use storage::{
    LedgerReport, MAX_BYTES_PER_LEDGER, MAX_EVENTS_PER_LEDGER, MAX_FORK_RECORDS, MAX_LEDGERS,
    PushResult, StoredPage, Totals, WitnessCaps, WitnessStorage,
};
pub use store::WitnessStore;

//! Node home, storage and runtimes (proposal 001 sections 7, 8, 10).
//!
//! The storage layer is plain files and synchronous `std::fs`: no database,
//! no index beyond a sorted directory listing, and no async traits. Callers
//! inside a tokio runtime reach it through `spawn_blocking`.
//!
//! Event bytes are stored exactly as the signer or the peer produced them and
//! are served back unmodified; nothing here decodes an event and re-encodes
//! it (proposal 001 section 3.1, byte authority).
//!
//! ```no_run
//! use mabel_node::{HomeOptions, NodeConfig, NodeHome, resolve_home};
//!
//! let root = resolve_home(None)?;
//! let home = NodeHome::open_or_create(root, &NodeConfig::default(), HomeOptions::default())?;
//! let endpoint_key = home.node_key()?;
//! # Ok::<(), mabel_node::StorageError>(())
//! ```

pub mod api;
mod atomic;
pub mod bindings;
mod config;
pub mod contacts;
mod endpoint;
mod error;
pub mod graph;
mod home;
pub mod keys;
mod ledger;
mod peers;
mod time;
pub mod verification;
pub mod wallet;
pub mod witness;

pub use atomic::{DATA_MODE, DIR_MODE, KEY_MODE};
pub use bindings::{BINDINGS_DIR, Binding, Bindings, BoundEndpoint, Observation};
pub use config::{
    DEFAULT_HTTP_BIND, DEFAULT_HTTP_PORT, DEFAULT_STORAGE_CAPACITY, MAX_WITNESS_FOR, NodeConfig,
    NodeRole, RelayMode, WITNESS_MIGRATION_HINT, WitnessEntry,
};
pub use contacts::{CONTACTS_DIR, ContactEntry, ContactStore};
pub use endpoint::bind_endpoint;
pub use error::{Result, StorageError};
pub use home::{
    ACTIVE_KEY_FILE, CONFIG_FILE, DEFAULT_HOME_NAME, DeclaredKind, HOME_ENV, HomeOptions,
    IDENTITY_META_FILE, IdentityMeta, NODE_KEY_FILE, NodeHome, PEERS_FILE, RESERVE_KEY_FILE,
    resolve_home,
};
pub use ledger::{
    EVENT_EXT, FORK_EXT, ForkFile, HEAD_FILE, Head, LedgerMeta, LedgerStore, META_FILE, NewEvent,
    SEQ_DIGITS, StoredEvent,
};
pub use peers::{HINT_MAX_AGE_MS, MAX_FAILURES, MAX_HINTS, PeerHint, Peers};
pub use time::{now_ms, rfc3339_utc};

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

mod atomic;
mod config;
mod error;
mod home;
pub mod keys;
mod ledger;
mod peers;

pub use atomic::{DATA_MODE, DIR_MODE, KEY_MODE};
pub use config::{
    DEFAULT_HTTP_BIND, DEFAULT_HTTP_PORT, DEFAULT_STORAGE_CAP, NodeConfig, NodeRole, RelayMode,
};
pub use error::{Result, StorageError};
pub use home::{
    ACTIVE_KEY_FILE, CONFIG_FILE, DEFAULT_HOME_NAME, HOME_ENV, HomeOptions, IDENTITY_META_FILE,
    IdentityKind, IdentityMeta, NODE_KEY_FILE, NodeHome, PEERS_FILE, RESERVE_KEY_FILE,
    resolve_home,
};
pub use ledger::{
    EVENT_EXT, FORK_EXT, ForkFile, HEAD_FILE, Head, LedgerMeta, LedgerStore, META_FILE, NewEvent,
    SEQ_DIGITS, StoredEvent,
};
pub use peers::Peers;

/// Milliseconds since the unix epoch, saturating at 0 before it.
#[must_use]
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

//! Storage errors and the exit codes the CLI maps them to (proposal 001
//! section 9).

use std::io;
use std::path::{Path, PathBuf};

use mabel_core::{EventId, IdentityId, LedgerId};

/// Result alias for every storage operation.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Why a node home operation failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// The filesystem refused an operation.
    #[error("io error on {path}: {source}")]
    Io {
        /// The path the operation targeted.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },

    /// A JSON file did not parse, or did not match its schema.
    #[error("{path} is not valid: {message}")]
    Json {
        /// The file that failed to parse.
        path: PathBuf,
        /// The parser's message.
        message: String,
    },

    /// A key file is readable by the group or the world.
    #[error(
        "{path} is group- or world-accessible (mode {mode:04o}); \
         pass --allow-insecure-permissions to use it anyway"
    )]
    InsecurePermissions {
        /// The key file.
        path: PathBuf,
        /// The file's permission bits.
        mode: u32,
    },

    /// A key file does not hold 32 hex-encoded bytes.
    #[error("{path} does not hold a 32-byte hex key: {message}")]
    MalformedKey {
        /// The key file.
        path: PathBuf,
        /// What was wrong with it.
        message: String,
    },

    /// An event file does not decode as a `SignedEvent`.
    #[error("{path} does not decode as a SignedEvent: {message}")]
    MalformedEvent {
        /// The event file.
        path: PathBuf,
        /// The decoder's message.
        message: String,
    },

    /// Neither `--home` nor `$MABEL_HOME` nor `$HOME` named a directory.
    #[error("no node home: set $MABEL_HOME or pass --home")]
    HomeUnknown,

    /// The directory holds no `node.json`.
    #[error("{path} is not a node home: node.json is missing")]
    NotAHome {
        /// The directory that was opened.
        path: PathBuf,
    },

    /// The directory already holds a `node.json`.
    #[error("{path} already holds a node home")]
    HomeExists {
        /// The directory that was being created.
        path: PathBuf,
    },

    /// An append did not start where the ledger ends.
    #[error("ledger {ledger} expects seq {expected}, got {got}")]
    OutOfOrderAppend {
        /// The ledger.
        ledger: LedgerId,
        /// The sequence the ledger expects next.
        expected: u64,
        /// The sequence the caller offered.
        got: u64,
    },

    /// An append would overwrite a stored event file with different bytes.
    ///
    /// The stored event may be one a crash left past the head cache, or one
    /// another writer landed a moment ago; either way it is an event somebody
    /// built and overwriting it loses it. Recovery is to drop the events past
    /// the head and rebuild the cache.
    #[error("ledger {ledger} holds a different event at seq {seq}, so {offered} cannot land there")]
    ConflictingEvent {
        /// The ledger.
        ledger: LedgerId,
        /// The sequence both events claim.
        seq: u64,
        /// The id of the event the caller offered.
        offered: EventId,
    },

    /// A caller's event id does not match the bytes it handed over.
    #[error("event bytes for seq {seq} hash to {actual}, not the given {claimed}")]
    EventIdMismatch {
        /// The sequence the event claimed.
        seq: u64,
        /// The id the caller passed.
        claimed: EventId,
        /// The id the bytes hash to.
        actual: EventId,
    },

    /// A read named a sequence the ledger does not hold.
    #[error("ledger {ledger} holds no event at seq {seq}")]
    MissingEvent {
        /// The ledger.
        ledger: LedgerId,
        /// The missing sequence.
        seq: u64,
    },

    /// The home holds no directory for that identity.
    #[error("identity {identity} is not in this home")]
    UnknownIdentity {
        /// The identity that was looked up.
        identity: IdentityId,
    },

    /// The operating system would not produce random bytes.
    #[error("random bytes unavailable: {0}")]
    Random(String),
}

impl StorageError {
    /// The process exit code for this error (proposal 001 section 9).
    ///
    /// 2 usage, 10 malformed input, 50 stale or conflicting state, 60
    /// insecure key file permissions. Everything else is a plain failure and
    /// exits 1, a code the table does not assign.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::HomeUnknown | Self::NotAHome { .. } | Self::HomeExists { .. } => 2,
            Self::Json { .. }
            | Self::MalformedKey { .. }
            | Self::MalformedEvent { .. }
            | Self::MissingEvent { .. }
            | Self::EventIdMismatch { .. }
            | Self::UnknownIdentity { .. } => 10,
            Self::OutOfOrderAppend { .. } | Self::ConflictingEvent { .. } => 50,
            Self::InsecurePermissions { .. } => 60,
            Self::Io { .. } | Self::Random(_) => 1,
        }
    }

    /// True when this is the insecure-permissions refusal, exit code 60.
    #[must_use]
    pub fn is_insecure_permissions(&self) -> bool {
        matches!(self, Self::InsecurePermissions { .. })
    }
}

/// Builds an [`StorageError::Io`] closure for one path.
pub(crate) fn io_at(path: &Path) -> impl FnOnce(io::Error) -> StorageError + use<'_> {
    move |source| StorageError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Builds a [`StorageError::Json`] closure for one path.
pub(crate) fn json_at(path: &Path) -> impl FnOnce(serde_json::Error) -> StorageError + use<'_> {
    move |source| StorageError::Json {
        path: path.to_path_buf(),
        message: source.to_string(),
    }
}

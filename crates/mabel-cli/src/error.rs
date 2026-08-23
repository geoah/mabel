//! The error envelope and the exit-code table (proposal 001 section 9,
//! `contracts/cli/errors.json`).
//!
//! One failure produces one document, `{ok: false, code, message, details}`,
//! and one exit code. [`ErrorLayer`] from `mabel-node` is the single table of
//! codes and message prefixes, so the CLI and the HTTP API cannot drift; the
//! CLI adds code 1, the unassigned code for a filesystem failure the table
//! does not cover.

use mabel_core::fold::Reason;
use mabel_core::sign::BuildError;
use mabel_node::StorageError;
use mabel_node::api::ErrorLayer;
use serde::Serialize;
use serde_json::{Map, Value};

/// Result of every command.
pub type Result<T> = std::result::Result<T, CliError>;

/// A failure, rendered as the error envelope or as one prefixed line.
#[derive(Debug, Clone)]
pub struct CliError {
    code: u16,
    reason: String,
    message: String,
    details: Map<String, Value>,
}

impl CliError {
    /// A failure in `layer`, classed by the snake_case `reason`.
    ///
    /// The message is passed without the layer prefix; the prefix is
    /// prepended here so no caller can spell it differently.
    pub fn new(layer: ErrorLayer, reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self {
            code: layer.code(),
            reason: reason.into(),
            message: format!("{}{}", layer.prefix(), message.as_ref()),
            details: Map::new(),
        }
    }

    /// Code 2, no prefix: a flag, a value or a name the command cannot use.
    pub fn usage(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Usage, reason, message)
    }

    /// Code 10, `Schema error:`.
    pub fn schema(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Schema, reason, message)
    }

    /// Code 20, `Ledger error:`.
    pub fn ledger(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Ledger, reason, message)
    }

    /// Code 20, `Policy error:`.
    pub fn policy(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Policy, reason, message)
    }

    /// Code 30, `Network error:`.
    pub fn network(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Network, reason, message)
    }

    /// Code 50, `State error:`.
    pub fn state(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::State, reason, message)
    }

    /// Code 60, no prefix.
    pub fn permissions(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Permissions, reason, message)
    }

    /// Code 70, no prefix.
    pub fn unsupported(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Unsupported, reason, message)
    }

    /// Code 1: a filesystem failure, which the section 9 table does not
    /// assign a code to.
    pub fn internal(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self {
            code: 1,
            reason: reason.into(),
            message: message.as_ref().to_owned(),
            details: Map::new(),
        }
    }

    /// Adds one key beside `reason` in `details`.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        let value = serde_json::to_value(value).unwrap_or(Value::Null);
        self.details.insert(key.into(), value);
        self
    }

    /// Replaces `details` wholesale; `reason` is added when it is rendered.
    #[must_use]
    pub fn with_details(mut self, details: Map<String, Value>) -> Self {
        self.details = details;
        self
    }

    /// Spells every path in the message and in `details.path` relative to the
    /// node home, which is how `contracts/cli/errors.json` renders them.
    #[must_use]
    pub fn relative_to_home(mut self, home: &std::path::Path) -> Self {
        let prefix = format!("{}/", home.display());
        self.message = self.message.replace(&prefix, "");
        if let Some(Value::String(path)) = self.details.get_mut("path") {
            *path = path.replace(&prefix, "");
        }
        self
    }

    /// The process exit code.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        i32::from(self.code)
    }

    /// The one-line message, layer prefix included.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The error envelope, with `reason` inside `details`.
    #[must_use]
    pub fn to_document(&self) -> Value {
        let mut details = Map::with_capacity(self.details.len() + 1);
        details.insert("reason".to_owned(), Value::String(self.reason.clone()));
        for (key, value) in &self.details {
            details.insert(key.clone(), value.clone());
        }
        let mut document = Map::with_capacity(4);
        document.insert("ok".to_owned(), Value::Bool(false));
        document.insert("code".to_owned(), Value::from(self.code));
        document.insert("message".to_owned(), Value::String(self.message.clone()));
        document.insert("details".to_owned(), Value::Object(details));
        Value::Object(document)
    }
}

/// A rejection from the fold, which is the authority on why an event is not
/// allowed.
///
/// Chain and cryptographic failures are `Ledger error:` and the semantic rules
/// are `Policy error:`; both exit 20. [`Reason::code`] is the `details.reason`,
/// so the CLI never invents a second spelling for a rejection the fold already
/// names.
impl From<&Reason> for CliError {
    fn from(reason: &Reason) -> Self {
        let message = reason.to_string();
        match reason {
            Reason::Wire(_) => Self::schema(reason.code(), message),
            Reason::WrongSeq { .. }
            | Reason::WrongLedger { .. }
            | Reason::BrokenPrevLink { .. }
            | Reason::BackwardsTimestamp { .. }
            | Reason::PayloadNotAllowed { .. }
            | Reason::InvalidPublicKey { .. }
            | Reason::UnauthorizedSigner { .. }
            | Reason::BadSignature => Self::ledger(reason.code(), message),
            _ => Self::policy(reason.code(), message),
        }
    }
}

/// A storage failure, carrying the code `mabel-node` assigned it.
impl From<StorageError> for CliError {
    fn from(error: StorageError) -> Self {
        let message = error.to_string();
        match &error {
            StorageError::InsecurePermissions { path, mode } => Self::permissions(
                "insecure_key_permissions",
                format!(
                    "key file has insecure permissions: {} is mode {mode:04o}, \
                     pass --allow-insecure-permissions to continue",
                    path.display()
                ),
            )
            .with_detail("path", path.display().to_string())
            .with_detail("mode", format!("{mode:04o}"))
            .with_detail("expected_mode", "0600"),
            StorageError::HomeUnknown | StorageError::NotAHome { .. } => {
                Self::usage("no_node_home", message)
            }
            StorageError::HomeExists { path } => {
                Self::usage("home_exists", message).with_detail("path", path.display().to_string())
            }
            StorageError::Json { .. }
            | StorageError::MalformedKey { .. }
            | StorageError::MalformedEvent { .. } => Self::schema("malformed_file", message),
            StorageError::UnknownIdentity { identity } => Self::usage("unknown_identity", message)
                .with_detail("identity", identity.to_string()),
            StorageError::MissingEvent { .. } | StorageError::EventIdMismatch { .. } => {
                Self::ledger("missing_event", message)
            }
            StorageError::OutOfOrderAppend { .. } => Self::state("out_of_order_append", message),
            _ => Self::internal("io_error", message),
        }
    }
}

/// A refusal from the signing path, which checks the byte-layout caps.
impl From<BuildError> for CliError {
    fn from(error: BuildError) -> Self {
        Self::schema("event_not_buildable", error.to_string())
    }
}

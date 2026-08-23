//! The error envelope: `{ok: false, code, message, details}`.
//!
//! `code` is the CLI exit code the same failure produces, so one table covers
//! the HTTP API and `mabel --json` (`contracts/README.md`, "The envelope").
//! Consumers branch on `code` and `details.reason`, never on `message`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Map, Value};

/// Which layer failed. The layer fixes the exit code, the message prefix and
/// the default HTTP status.
///
/// Two layers share code 20 and two share code 50, which is why this is an
/// enum of layers rather than of codes: the prefix is what tells
/// `Ledger error:` from `Policy error:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLayer {
    /// Code 2: usage, unknown route or parameter, rejected by the loopback
    /// rules. No prefix.
    Usage,
    /// Code 10: invalid schema or malformed input.
    Schema,
    /// Code 20: cryptographic or chain failure.
    Ledger,
    /// Code 20: semantic rule violation.
    Policy,
    /// Code 30: peer or network unavailable.
    Network,
    /// Code 50: stale state or a conflicting event.
    State,
    /// Code 50: replay of a single-use artifact.
    Replay,
    /// Code 60: insecure key file permissions. No prefix.
    Permissions,
    /// Code 70: unsupported feature or version. No prefix.
    Unsupported,
}

impl ErrorLayer {
    /// Every layer, for table tests.
    pub const ALL: [Self; 9] = [
        Self::Usage,
        Self::Schema,
        Self::Ledger,
        Self::Policy,
        Self::Network,
        Self::State,
        Self::Replay,
        Self::Permissions,
        Self::Unsupported,
    ];

    /// The CLI exit code this layer reports as `code`.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Usage => 2,
            Self::Schema => 10,
            Self::Ledger | Self::Policy => 20,
            Self::Network => 30,
            Self::State | Self::Replay => 50,
            Self::Permissions => 60,
            Self::Unsupported => 70,
        }
    }

    /// The prefix every message of this layer carries, empty for codes 2, 60
    /// and 70.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Usage | Self::Permissions | Self::Unsupported => "",
            Self::Schema => "Schema error: ",
            Self::Ledger => "Ledger error: ",
            Self::Policy => "Policy error: ",
            Self::Network => "Network error: ",
            Self::State => "State error: ",
            Self::Replay => "Replay error: ",
        }
    }

    /// The status this layer answers unless the caller overrides it.
    ///
    /// Code 2 defaults to 400, so the loopback rules and the not-found paths
    /// set 403, 404, 405 and 415 explicitly.
    #[must_use]
    pub const fn default_status(self) -> StatusCode {
        match self {
            Self::Usage | Self::Schema => StatusCode::BAD_REQUEST,
            Self::Ledger | Self::Policy | Self::State | Self::Replay => StatusCode::CONFLICT,
            Self::Network => StatusCode::BAD_GATEWAY,
            Self::Permissions => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unsupported => StatusCode::NOT_IMPLEMENTED,
        }
    }
}

/// A failure a service or a handler reports, rendered as the error envelope.
///
/// The `message` passed to the constructors is the sentence without the layer
/// prefix; the prefix is prepended once, here, so no caller can spell it
/// differently.
#[derive(Debug, Clone)]
pub struct ServiceError {
    layer: ErrorLayer,
    status: StatusCode,
    reason: String,
    message: String,
    details: Map<String, Value>,
}

impl ServiceError {
    /// A failure in `layer`, classed by the snake_case `reason`.
    #[must_use]
    pub fn new(layer: ErrorLayer, reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self {
            layer,
            status: layer.default_status(),
            reason: reason.into(),
            message: format!("{}{}", layer.prefix(), message.as_ref()),
            details: Map::new(),
        }
    }

    /// Code 2, no prefix: usage, an unknown route or parameter, or a request
    /// the loopback rules turned away.
    #[must_use]
    pub fn usage(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Usage, reason, message)
    }

    /// Code 10, `Schema error:`.
    #[must_use]
    pub fn schema(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Schema, reason, message)
    }

    /// Code 20, `Ledger error:`.
    #[must_use]
    pub fn ledger(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Ledger, reason, message)
    }

    /// Code 20, `Policy error:`.
    #[must_use]
    pub fn policy(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Policy, reason, message)
    }

    /// Code 30, `Network error:`.
    #[must_use]
    pub fn network(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Network, reason, message)
    }

    /// Code 50, `State error:`.
    #[must_use]
    pub fn state(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::State, reason, message)
    }

    /// Code 50, `Replay error:`.
    #[must_use]
    pub fn replay(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Replay, reason, message)
    }

    /// Code 60, no prefix.
    #[must_use]
    pub fn permissions(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Permissions, reason, message)
    }

    /// Code 70, no prefix.
    #[must_use]
    pub fn unsupported(reason: impl Into<String>, message: impl AsRef<str>) -> Self {
        Self::new(ErrorLayer::Unsupported, reason, message)
    }

    /// Answers `status` instead of the layer's default.
    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Adds one key to `details`, beside `reason`.
    ///
    /// A value that cannot serialize lands as `null` rather than losing the
    /// whole error.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        let value = serde_json::to_value(value).unwrap_or(Value::Null);
        self.details.insert(key.into(), value);
        self
    }

    /// The layer.
    #[must_use]
    pub const fn layer(&self) -> ErrorLayer {
        self.layer
    }

    /// The CLI exit code, reported as `code`.
    #[must_use]
    pub const fn code(&self) -> u16 {
        self.layer.code()
    }

    /// The HTTP status this error answers.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// The stable snake_case class name, reported as `details.reason`.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The one-line message, prefix included.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The error envelope as a JSON value, `reason` first inside `details`.
    #[must_use]
    pub fn to_document(&self) -> Value {
        let mut details = Map::with_capacity(self.details.len() + 1);
        details.insert("reason".to_owned(), Value::String(self.reason.clone()));
        for (key, value) in &self.details {
            details.insert(key.clone(), value.clone());
        }
        let mut document = Map::with_capacity(4);
        document.insert("ok".to_owned(), Value::Bool(false));
        document.insert("code".to_owned(), Value::from(self.code()));
        document.insert("message".to_owned(), Value::String(self.message.clone()));
        document.insert("details".to_owned(), Value::Object(details));
        Value::Object(document)
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({})", self.message, self.reason)
    }
}

impl std::error::Error for ServiceError {}

impl Serialize for ServiceError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_document().serialize(serializer)
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self.to_document())).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorLayer, ServiceError, StatusCode};
    use serde_json::json;

    #[test]
    fn the_envelope_carries_the_code_the_message_and_reason_first() {
        let error = ServiceError::policy(
            "duplicate_unrevoked_attestation",
            "an unrevoked attestation for bob already exists at seq 2",
        )
        .with_detail("subject", "bob")
        .with_detail("at_seq", 2);
        assert_eq!(error.status(), StatusCode::CONFLICT);
        assert_eq!(
            error.to_document(),
            json!({
                "ok": false,
                "code": 20,
                "message": "Policy error: an unrevoked attestation for bob already exists at seq 2",
                "details": {
                    "reason": "duplicate_unrevoked_attestation",
                    "subject": "bob",
                    "at_seq": 2
                }
            })
        );
    }

    #[test]
    fn every_layer_maps_to_the_code_and_prefix_of_the_contract_table() {
        let expected = [
            (ErrorLayer::Usage, 2, ""),
            (ErrorLayer::Schema, 10, "Schema error: "),
            (ErrorLayer::Ledger, 20, "Ledger error: "),
            (ErrorLayer::Policy, 20, "Policy error: "),
            (ErrorLayer::Network, 30, "Network error: "),
            (ErrorLayer::State, 50, "State error: "),
            (ErrorLayer::Replay, 50, "Replay error: "),
            (ErrorLayer::Permissions, 60, ""),
            (ErrorLayer::Unsupported, 70, ""),
        ];
        assert_eq!(expected.len(), ErrorLayer::ALL.len());
        for (layer, code, prefix) in expected {
            assert_eq!(layer.code(), code, "{layer:?}");
            assert_eq!(layer.prefix(), prefix, "{layer:?}");
            let error = ServiceError::new(layer, "reason", "something failed");
            assert_eq!(error.message(), format!("{prefix}something failed"));
        }
    }

    #[test]
    fn the_default_status_follows_the_contract_table() {
        assert_eq!(
            ServiceError::schema("r", "m").status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(ServiceError::state("r", "m").status(), StatusCode::CONFLICT);
        assert_eq!(
            ServiceError::network("r", "m").status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ServiceError::permissions("r", "m").status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServiceError::unsupported("r", "m").status(),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            ServiceError::usage("r", "m")
                .with_status(StatusCode::NOT_FOUND)
                .status(),
            StatusCode::NOT_FOUND
        );
    }
}

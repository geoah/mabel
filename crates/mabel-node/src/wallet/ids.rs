//! Rendering the 32-byte values a wallet document carries.
//!
//! One spelling everywhere: lowercase RFC 4648 base32 without padding, 52
//! characters (`contracts/README.md`, "Ids and byte fields").

use data_encoding::BASE32_NOPAD;
use iroh_base::PublicKey;
use mabel_core::{EventId, IdentityId, LedgerId};

use crate::api::documents::Id;

/// Renders 32 bytes as a document id.
///
/// # Panics
///
/// Panics if the encoding is not 52 base32 characters, which 32 bytes always
/// are.
#[must_use]
pub fn bytes(value: &[u8; 32]) -> Id {
    Id::parse(&BASE32_NOPAD.encode(value).to_ascii_lowercase())
        .expect("32 bytes encode as 52 base32 characters")
}

/// Renders an identity or ledger id.
#[must_use]
pub fn identity(id: IdentityId) -> Id {
    bytes(id.as_bytes())
}

/// Renders an event id.
#[must_use]
pub fn event(id: EventId) -> Id {
    bytes(id.as_bytes())
}

/// Renders a public key or an endpoint id.
#[must_use]
pub fn key(key: &PublicKey) -> Id {
    bytes(key.as_bytes())
}

/// Reads back an identity id a document already rendered.
///
/// # Errors
///
/// Returns code 10 when the string is not 52 base32 characters.
pub fn parse_identity(id: &Id) -> Result<IdentityId, crate::api::error::ServiceError> {
    id.as_str().parse::<IdentityId>().map_err(|error| {
        crate::api::error::ServiceError::schema(
            "malformed_identity_id",
            format!("identity id is not 52 base32 characters: {error}"),
        )
        .with_detail("value", id.as_str())
    })
}

/// Reads back a ledger id a document already rendered.
///
/// # Errors
///
/// As [`parse_identity`].
pub fn parse_ledger(id: &Id) -> Result<LedgerId, crate::api::error::ServiceError> {
    parse_identity(id)
}

/// Reads back an event id a document already rendered.
///
/// # Errors
///
/// Returns code 10 when the string is not 52 base32 characters.
pub fn parse_event(id: &Id) -> Result<EventId, crate::api::error::ServiceError> {
    id.as_str().parse::<EventId>().map_err(|error| {
        crate::api::error::ServiceError::schema(
            "malformed_event_id",
            format!("event id is not 52 base32 characters: {error}"),
        )
        .with_detail("value", id.as_str())
    })
}

/// Reads back an endpoint id a document already rendered.
///
/// # Errors
///
/// Returns code 10 when the string does not decode to a public key.
pub fn parse_endpoint(id: &Id) -> Result<PublicKey, crate::api::error::ServiceError> {
    let malformed = || {
        crate::api::error::ServiceError::schema(
            "malformed_endpoint_id",
            "endpoint id is not 52 base32 characters",
        )
        .with_detail("value", id.as_str())
    };
    let decoded = BASE32_NOPAD
        .decode(id.as_str().to_ascii_uppercase().as_bytes())
        .map_err(|_| malformed())?;
    let bytes: [u8; 32] = decoded.try_into().map_err(|_| malformed())?;
    PublicKey::from_bytes(&bytes).map_err(|_| malformed())
}

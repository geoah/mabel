//! Rendering and parsing the 32-byte values every document carries.
//!
//! One spelling everywhere: lowercase RFC 4648 base32 without padding, 52
//! characters, for identity ids, event ids, public keys and endpoint ids
//! (`contracts/README.md`, "Ids and byte fields"). `iroh_base` spells an
//! endpoint id as hex, so an endpoint typed on the command line is accepted in
//! either form and always rendered as base32.

use data_encoding::{BASE32_NOPAD, HEXLOWER_PERMISSIVE};
use iroh_base::{EndpointId, PublicKey};
use mabel_core::id::ID_STR_LEN;
use mabel_core::{EventId, IdentityId};
use mabel_node::api::documents::Id;
use mabel_node::verification::check_hostname;

use crate::error::{CliError, Result};

/// Renders 32 bytes as a document id.
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

/// Parses an event id typed on the command line.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_event_id`.
pub fn parse_event(raw: &str) -> Result<EventId> {
    raw.parse::<EventId>().map_err(|error| {
        CliError::usage(
            "malformed_event_id",
            format!("{raw} is not an event id: {error}"),
        )
        .with_detail("value", raw)
    })
}

/// Parses an endpoint id, base32 as the documents spell it or hex as
/// `iroh_base` and `node.json` do.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_endpoint_id`.
pub fn parse_endpoint(raw: &str) -> Result<EndpointId> {
    let decoded = if raw.len() == ID_STR_LEN {
        BASE32_NOPAD
            .decode(raw.to_ascii_uppercase().as_bytes())
            .ok()
    } else {
        HEXLOWER_PERMISSIVE.decode(raw.as_bytes()).ok()
    };
    let bytes: [u8; 32] = decoded
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| malformed_endpoint(raw))?;
    EndpointId::from_bytes(&bytes).map_err(|_| malformed_endpoint(raw))
}

/// Parses a hostname typed on the command line.
///
/// The syntax a profile hostname must satisfy (proposal 003 section 2), so a
/// name a flag accepts is a name a ledger could claim. Trimmed and lowercased,
/// the way `GET /api/resolve?input=` reads one.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_hostname`: a flag value that is not a
/// hostname is a command line to fix, not a schema failure.
pub fn parse_hostname(raw: &str) -> Result<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    check_hostname(&trimmed).map_err(|detail| {
        CliError::usage(
            "malformed_hostname",
            format!("{trimmed} is not a hostname: {detail}"),
        )
        .with_detail("value", trimmed.clone())
        .with_detail("detail", detail)
    })?;
    Ok(trimmed)
}

fn malformed_endpoint(raw: &str) -> CliError {
    CliError::usage(
        "malformed_endpoint_id",
        format!("{raw} is not an endpoint id: expected 52 base32 or 64 hex characters"),
    )
    .with_detail("value", raw)
}

#[cfg(test)]
mod tests {
    use super::{bytes, parse_endpoint, parse_event};

    #[test]
    fn thirty_two_bytes_render_as_fifty_two_lowercase_characters() {
        let rendered = bytes(&[0xab; 32]);
        assert_eq!(rendered.as_str().len(), 52);
        assert_eq!(rendered.as_str(), rendered.as_str().to_ascii_lowercase());
    }

    #[test]
    fn an_endpoint_parses_from_base32_and_from_hex_to_the_same_key() {
        let endpoint = iroh_base::SecretKey::from_bytes(&[7u8; 32]).public();
        let hex = data_encoding::HEXLOWER.encode(endpoint.as_bytes());
        let base32 = super::key(&endpoint);
        assert_eq!(parse_endpoint(&hex).unwrap(), endpoint);
        assert_eq!(parse_endpoint(base32.as_str()).unwrap(), endpoint);
        assert_eq!(
            parse_endpoint(&base32.as_str().to_ascii_uppercase()).unwrap(),
            endpoint
        );
    }

    #[test]
    fn a_name_that_is_not_an_id_is_a_usage_error() {
        for error in [
            parse_event("alice").unwrap_err(),
            parse_endpoint("alice").unwrap_err(),
        ] {
            assert_eq!(error.exit_code(), 2);
            let document = error.to_document();
            assert_eq!(document["ok"], false);
            assert_eq!(document["code"], 2);
            assert_eq!(document["details"]["value"], "alice");
        }
        assert_eq!(
            parse_event("alice").unwrap_err().to_document()["details"]["reason"],
            "malformed_event_id"
        );
    }
}

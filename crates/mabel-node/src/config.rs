//! The typed `node.json` (proposal 001 sections 5, 8 and the clarifications).
//!
//! Every field has a default, and an unknown field or an unknown value is a
//! load error rather than a silently ignored setting.

use std::net::SocketAddr;

use iroh_base::EndpointId;
use serde::{Deserialize, Serialize};

/// Default HTTP bind address.
///
/// Both roles bind loopback (proposal 001 section 10); the proposal names no
/// port, so the node layer picks one.
pub const DEFAULT_HTTP_BIND: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    DEFAULT_HTTP_PORT,
);

/// Port of [`DEFAULT_HTTP_BIND`].
pub const DEFAULT_HTTP_PORT: u16 = 9080;

/// Default storage capacity, 2 GiB (proposal 001 section 5).
pub const DEFAULT_STORAGE_CAPACITY: u64 = 2 * 1024 * 1024 * 1024;

/// What this node is (proposal 001 section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    /// Holds identity keys, appends events, pushes to witnesses.
    #[default]
    Wallet,
    /// Passive replica; signs nothing and holds no identity keys.
    Witness,
}

/// Whether the Iroh endpoint uses the n0 relays (proposal 001,
/// clarifications).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayMode {
    /// The n0 default relay set.
    #[default]
    N0,
    /// No relays; peers must be reachable directly or through a seeded
    /// ticket, which is what the compose topology uses.
    Disabled,
}

/// The contents of `node.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    /// Wallet or witness.
    #[serde(default)]
    pub role: NodeRole,
    /// Address the HTTP API and UI bind to.
    #[serde(default = "default_http_bind")]
    pub http_bind: SocketAddr,
    /// Witness endpoints this node pushes to by default.
    #[serde(default)]
    pub witnesses: Vec<EndpointId>,
    /// Bytes of stored ledger data this node accepts before refusing more.
    /// Named in full, like the `storage_capacity` the HTTP API reports
    /// (decision 012, contracts/README.md).
    #[serde(default = "default_storage_capacity")]
    pub storage_capacity: u64,
    /// Relay setting for the Iroh endpoint.
    #[serde(default)]
    pub relay: RelayMode,
}

fn default_http_bind() -> SocketAddr {
    DEFAULT_HTTP_BIND
}

fn default_storage_capacity() -> u64 {
    DEFAULT_STORAGE_CAPACITY
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            role: NodeRole::default(),
            http_bind: DEFAULT_HTTP_BIND,
            witnesses: Vec::new(),
            storage_capacity: DEFAULT_STORAGE_CAPACITY,
            relay: RelayMode::default(),
        }
    }
}

impl NodeConfig {
    /// A config for one role, everything else defaulted.
    #[must_use]
    pub fn for_role(role: NodeRole) -> Self {
        Self {
            role,
            ..Self::default()
        }
    }

    /// Parses `node.json` bytes.
    ///
    /// # Errors
    ///
    /// Returns the parse error for malformed JSON, an unknown field, an
    /// unknown `role` or `relay` value, or a malformed endpoint id.
    pub fn from_json(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }

    /// Renders `node.json`, pretty-printed with a trailing newline.
    ///
    /// # Errors
    ///
    /// Returns an error only if serialization fails, which the field types
    /// do not permit.
    pub fn to_json(&self) -> serde_json::Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_HTTP_BIND, DEFAULT_STORAGE_CAPACITY, NodeConfig, NodeRole, RelayMode};

    #[test]
    fn an_empty_object_loads_every_default() {
        let config = NodeConfig::from_json(b"{}").expect("defaults apply");
        assert_eq!(config, NodeConfig::default());
        assert_eq!(config.role, NodeRole::Wallet);
        assert_eq!(config.http_bind, DEFAULT_HTTP_BIND);
        assert_eq!(config.storage_capacity, DEFAULT_STORAGE_CAPACITY);
        assert_eq!(config.relay, RelayMode::N0);
        assert!(config.witnesses.is_empty());
    }

    #[test]
    fn round_trips_through_json() {
        let key = iroh_base::SecretKey::from_bytes(&[3u8; 32]).public();
        let config = NodeConfig {
            role: NodeRole::Witness,
            http_bind: "127.0.0.1:1234".parse().unwrap(),
            witnesses: vec![key],
            storage_capacity: 42,
            relay: RelayMode::Disabled,
        };
        let json = config.to_json().unwrap();
        assert_eq!(NodeConfig::from_json(&json).unwrap(), config);
        assert!(json.ends_with(b"\n"));
    }

    #[test]
    fn relay_renders_as_n0_and_disabled() {
        let json = NodeConfig::default().to_json().unwrap();
        let text = String::from_utf8(json).unwrap();
        assert!(text.contains("\"relay\": \"n0\""), "{text}");
        assert!(text.contains("\"role\": \"wallet\""), "{text}");
    }

    #[test]
    fn an_unknown_relay_value_is_a_load_error() {
        let error = NodeConfig::from_json(br#"{"relay": "sometimes"}"#)
            .expect_err("sometimes is not a relay mode");
        assert!(error.to_string().contains("sometimes"), "{error}");
    }

    #[test]
    fn an_unknown_role_is_a_load_error() {
        assert!(NodeConfig::from_json(br#"{"role": "oracle"}"#).is_err());
    }

    #[test]
    fn an_unknown_field_is_a_load_error() {
        let error = NodeConfig::from_json(br#"{"storage_cap": 1}"#).expect_err("unknown field");
        assert!(error.to_string().contains("storage_cap"), "{error}");
    }

    #[test]
    fn a_malformed_witness_endpoint_is_a_load_error() {
        assert!(NodeConfig::from_json(br#"{"witnesses": ["nope"]}"#).is_err());
    }
}

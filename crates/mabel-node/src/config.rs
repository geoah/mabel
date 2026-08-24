//! The typed `node.json` (proposal 001 sections 5, 8 and the clarifications).
//!
//! Every field has a default, and an unknown field or an unknown value is a
//! load error rather than a silently ignored setting.

use std::net::SocketAddr;

use iroh_base::EndpointId;
use mabel_core::IdentityId;
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

/// Witness identities one home may witness for (proposal 006 section 4).
pub const MAX_WITNESS_FOR: usize = 16;

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
    /// `Host` values the HTTP API accepts besides the two loopback spellings,
    /// each a `host` or a `host:port` (decision 018).
    ///
    /// Empty by default, which is loopback only. The `--allow-host` flags of
    /// `mabel wallet serve` and `mabel witness run` add to this set rather than
    /// replacing it.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Witness endpoints this node pushes to by default.
    #[serde(default)]
    pub witnesses: Vec<EndpointId>,
    /// The witness identities this home witnesses for (proposal 006
    /// section 4).
    ///
    /// Empty by default, and empty means this home witnesses for nobody: a
    /// push for a ledger it neither signs for nor already stores under a live
    /// witness set is refused. Exposure is an explicit operator act, so a
    /// wallet does not become a public dump the moment it holds an identity.
    ///
    /// An entry names an identity id and nothing else. It does not have to name
    /// an identity under `identities/`: a witness fleet is several machines
    /// answering for one witness identity, and only one of them holds that
    /// identity's keys.
    #[serde(default, deserialize_with = "witness_for")]
    pub witness_for: Vec<IdentityId>,
    /// Whether a pre-proposal tag-11 `WitnessConfig` naming this node's own
    /// endpoint id still admits a push (proposal 006 section 4, clause 4).
    ///
    /// False by default, and a migration switch: it exists so a ledger written
    /// before witnesses were identities can still be pushed to the home that
    /// kept it, and it goes with the last such ledger. The clause it opens is
    /// gated twice more, on a non-empty `witness_for` and on this node's own
    /// endpoint id being in the tag-11 list, so turning it on cannot make a
    /// home that witnesses for nobody take a stranger's push.
    #[serde(default)]
    pub accept_legacy_witness_config: bool,
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

/// Reads `witness_for`: at most [`MAX_WITNESS_FOR`] identity ids, no duplicate.
///
/// A malformed id, a repeat or a seventeenth entry is a load error, like every
/// other bad value in this file: a config that means something other than what
/// it says is worse than a node that refuses to start.
fn witness_for<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<IdentityId>, D::Error> {
    use serde::de::Error;

    let entries = <Vec<IdentityId> as Deserialize>::deserialize(deserializer)?;
    if entries.len() > MAX_WITNESS_FOR {
        return Err(D::Error::custom(format!(
            "witness_for holds {} identities, over the {MAX_WITNESS_FOR}-identity cap",
            entries.len()
        )));
    }
    for (index, entry) in entries.iter().enumerate() {
        if entries[index + 1..].contains(entry) {
            return Err(D::Error::custom(format!("witness_for names {entry} twice")));
        }
    }
    Ok(entries)
}

fn default_storage_capacity() -> u64 {
    DEFAULT_STORAGE_CAPACITY
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            role: NodeRole::default(),
            http_bind: DEFAULT_HTTP_BIND,
            allowed_hosts: Vec::new(),
            witnesses: Vec::new(),
            witness_for: Vec::new(),
            accept_legacy_witness_config: false,
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
    use super::{
        DEFAULT_HTTP_BIND, DEFAULT_STORAGE_CAPACITY, MAX_WITNESS_FOR, NodeConfig, NodeRole,
        RelayMode,
    };

    #[test]
    fn an_empty_object_loads_every_default() {
        let config = NodeConfig::from_json(b"{}").expect("defaults apply");
        assert_eq!(config, NodeConfig::default());
        assert_eq!(config.role, NodeRole::Wallet);
        assert_eq!(config.http_bind, DEFAULT_HTTP_BIND);
        assert_eq!(config.storage_capacity, DEFAULT_STORAGE_CAPACITY);
        assert_eq!(config.relay, RelayMode::N0);
        assert!(config.witnesses.is_empty());
        assert!(config.witness_for.is_empty());
        assert!(
            !config.accept_legacy_witness_config,
            "the tag-11 migration switch is off unless a file turns it on"
        );
        assert!(config.allowed_hosts.is_empty());
    }

    /// The switch is a plain boolean, and a `node.json` written before it
    /// existed loads with it off (proposal 006 section 4).
    #[test]
    fn accept_legacy_witness_config_defaults_off_and_round_trips() {
        let config = NodeConfig::from_json(br#"{"accept_legacy_witness_config": true}"#)
            .expect("the switch loads");
        assert!(config.accept_legacy_witness_config);
        let text = String::from_utf8(config.to_json().unwrap()).unwrap();
        assert!(
            text.contains("\"accept_legacy_witness_config\": true"),
            "{text}"
        );
        assert!(NodeConfig::from_json(br#"{"accept_legacy_witness_config": "yes"}"#).is_err());
    }

    /// A home written before decision 018 loads with an empty
    /// `allowed_hosts`, which is loopback only.
    #[test]
    fn a_node_json_without_allowed_hosts_accepts_loopback_alone() {
        let config = NodeConfig::from_json(br#"{"role": "wallet"}"#).expect("the field defaults");
        assert!(config.allowed_hosts.is_empty());
    }

    #[test]
    fn allowed_hosts_round_trips_as_written() {
        let config = NodeConfig::from_json(br#"{"allowed_hosts": ["wallet.tailnet.example"]}"#)
            .expect("a host list loads");
        assert_eq!(config.allowed_hosts, ["wallet.tailnet.example"]);
        let text = String::from_utf8(config.to_json().unwrap()).unwrap();
        assert!(text.contains("\"wallet.tailnet.example\""), "{text}");
    }

    #[test]
    fn round_trips_through_json() {
        let key = iroh_base::SecretKey::from_bytes(&[3u8; 32]).public();
        let config = NodeConfig {
            role: NodeRole::Witness,
            http_bind: "127.0.0.1:1234".parse().unwrap(),
            allowed_hosts: vec!["witness.tailnet.example".to_owned()],
            witnesses: vec![key],
            witness_for: vec![mabel_core::IdentityId::from_bytes([5u8; 32])],
            accept_legacy_witness_config: true,
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

    /// `witness_for` names identity ids alone, at most 16, with no repeat
    /// (proposal 006 section 4). It needs no local key: an entry may name an
    /// identity this home holds nothing of.
    #[test]
    fn witness_for_takes_ids_alone_capped_and_distinct() {
        let alice = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
        let bob = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";

        let config = NodeConfig::from_json(format!(r#"{{"witness_for": ["{alice}"]}}"#).as_bytes())
            .expect("one identity loads");
        assert_eq!(config.witness_for.len(), 1);
        assert_eq!(config.witness_for[0].to_string(), alice);

        let error = NodeConfig::from_json(
            format!(r#"{{"witness_for": ["{alice}", "{alice}"]}}"#).as_bytes(),
        )
        .expect_err("a repeat is a load error");
        assert!(error.to_string().contains("twice"), "{error}");

        // An endpoint id renders as 64 hex characters, not 52 base32, so an
        // operator who pastes one gets a load error rather than a witness set
        // that means nothing.
        assert!(
            NodeConfig::from_json(
                br#"{"witness_for": ["1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809"]}"#
            )
            .is_err()
        );

        let over: Vec<String> = std::iter::repeat_n(bob, MAX_WITNESS_FOR + 1)
            .enumerate()
            .map(|(index, id)| {
                // Sixteen distinct ids plus one more, all well formed.
                let mut bytes = data_encoding::BASE32_NOPAD
                    .decode(id.to_ascii_uppercase().as_bytes())
                    .expect("a rendered id decodes");
                bytes[0] = index as u8;
                data_encoding::BASE32_NOPAD
                    .encode(&bytes)
                    .to_ascii_lowercase()
            })
            .collect();
        let json = format!(
            r#"{{"witness_for": {}}}"#,
            serde_json::to_string(&over).unwrap()
        );
        let error = NodeConfig::from_json(json.as_bytes()).expect_err("17 is over the cap");
        assert!(error.to_string().contains("17 identities"), "{error}");
    }
}

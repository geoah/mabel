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

/// Characters an endpoint id renders as, which `iroh_base` spells in hex.
///
/// A base32 identity id renders as 52, so the two shapes are told apart by
/// length alone and an old `node.json` is refused rather than misread
/// (proposal 006 section 5.4).
const ENDPOINT_ID_CHARS: usize = 64;

/// What to run when `witnesses` holds the pre-proposal-006 shape.
pub const WITNESS_MIGRATION_HINT: &str =
    "mabel witness set-default --witness <mabel-id> --endpoints <endpoint,...>";

/// One configured witness: the identity, and the raw endpoints that make it
/// reachable before anything is fetched (proposal 006 section 5.4).
///
/// The endpoints are bootstrap records, not a cache. `peers.json` has an
/// eviction rule and the one fact that makes a configured witness reachable at
/// all cannot live somewhere a cap can evict it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessEntry {
    /// The witness identity.
    pub identity: IdentityId,
    /// Endpoints to dial for it, in the order they were written.
    #[serde(default)]
    pub endpoints: Vec<EndpointId>,
}

impl WitnessEntry {
    /// An entry naming `identity` and the endpoints given for it.
    #[must_use]
    pub fn new(identity: IdentityId, endpoints: Vec<EndpointId>) -> Self {
        Self {
            identity,
            endpoints,
        }
    }
}

/// What this node was configured as, before one node served one API.
///
/// Read by nothing (proposal 006 section 8). It survives only so an existing
/// `node.json` still loads.
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
    /// Recognised, read by nothing, and never written (proposal 006 section
    /// 8).
    ///
    /// One node serves one API, and what it can do is read from the identities
    /// its home holds and from `witness_for`. The field stays because this
    /// struct denies unknown fields, so deleting it would stop every
    /// `node.json` written before this release from loading; a file that
    /// carries it loads and the node says so once at startup. It is not written
    /// back, so the fix the log line names, deleting the line, sticks.
    #[serde(default, skip_serializing)]
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
    /// Witness identities this node asks for any ledger, with the bootstrap
    /// endpoints that reach them (proposal 006 section 5.4).
    ///
    /// Source 4 of the resolution order: it needs no copy of anything, which is
    /// why it is the workhorse for a ledger this home has never seen.
    #[serde(default, deserialize_with = "witnesses")]
    pub witnesses: Vec<WitnessEntry>,
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

/// Reads `witnesses`: `{identity, endpoints}` objects, with the migration of
/// proposal 006 section 5.4 for the two older shapes.
///
/// A bare 52-character identity id loads as an entry with no bootstrap
/// endpoints, which resolution can still reach through a local copy, a hint or
/// DNS. A bare 64-character hex endpoint id is the pre-proposal-006 file and
/// fails to load naming what to run instead, because reading an endpoint id as
/// an identity id would silently configure a witness that is not one.
fn witnesses<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<WitnessEntry>, D::Error> {
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Named(String),
        Entry(WitnessEntry),
    }

    let raw = <Vec<Raw> as Deserialize>::deserialize(deserializer)?;
    let mut entries: Vec<WitnessEntry> = Vec::with_capacity(raw.len());
    for entry in raw {
        let entry = match entry {
            Raw::Entry(entry) => entry,
            Raw::Named(named) if named.len() == ENDPOINT_ID_CHARS => {
                return Err(D::Error::custom(format!(
                    "node.json names the endpoint id {named} under witnesses, which proposal 006 \
                     replaced with {{\"identity\", \"endpoints\"}} objects; run \
                     {WITNESS_MIGRATION_HINT}"
                )));
            }
            Raw::Named(named) => WitnessEntry::new(
                named.parse::<IdentityId>().map_err(|error| {
                    D::Error::custom(format!("witnesses names {named}: {error}"))
                })?,
                Vec::new(),
            ),
        };
        // Every id these parse errors quote is quoted as `node.json` spells it,
        // bare, so a reader can search the file for the string the error names.
        // Decision 019 keeps `node.json` a machine surface for the same reason.
        if entries.iter().any(|seen| seen.identity == entry.identity) {
            return Err(D::Error::custom(format!(
                "witnesses names {} twice",
                entry.identity
            )));
        }
        for (index, endpoint) in entry.endpoints.iter().enumerate() {
            if entry.endpoints[index + 1..].contains(endpoint) {
                return Err(D::Error::custom(format!(
                    "witness {} names the endpoint {endpoint} twice",
                    entry.identity
                )));
            }
        }
        entries.push(entry);
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

    /// The bootstrap endpoints recorded beside `witness`, empty when this file
    /// names no such witness.
    #[must_use]
    pub fn witness_endpoints(&self, witness: IdentityId) -> &[EndpointId] {
        self.witnesses
            .iter()
            .find(|entry| entry.identity == witness)
            .map_or(&[], |entry| entry.endpoints.as_slice())
    }

    /// Every bootstrap endpoint the configured witnesses name, in file order
    /// and without a repeat.
    #[must_use]
    pub fn witness_bootstrap(&self) -> Vec<EndpointId> {
        let mut endpoints: Vec<EndpointId> = Vec::new();
        for entry in &self.witnesses {
            for endpoint in &entry.endpoints {
                if !endpoints.contains(endpoint) {
                    endpoints.push(*endpoint);
                }
            }
        }
        endpoints
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
        RelayMode, WitnessEntry,
    };

    const ALICE: &str = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";

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
            // `role` is never written, so it never round trips: it defaults on
            // the way back in (proposal 006 section 8).
            role: NodeRole::default(),
            http_bind: "127.0.0.1:1234".parse().unwrap(),
            allowed_hosts: vec!["witness.tailnet.example".to_owned()],
            witnesses: vec![WitnessEntry::new(ALICE.parse().unwrap(), vec![key])],
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
    }

    /// `role` loads from a file that carries it and is never written back, so
    /// deleting the line sticks (proposal 006 section 8).
    #[test]
    fn role_loads_and_is_never_written() {
        let loaded = NodeConfig::from_json(br#"{"role": "witness"}"#).expect("role still loads");
        assert_eq!(loaded.role, NodeRole::Witness);
        let text = String::from_utf8(loaded.to_json().unwrap()).unwrap();
        assert!(!text.contains("role"), "{text}");
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

    /// `witnesses` names identities and the endpoints that reach them
    /// (proposal 006 section 5.4).
    #[test]
    fn witnesses_are_identity_and_endpoint_objects() {
        let key = iroh_base::SecretKey::from_bytes(&[3u8; 32]).public();
        let config = NodeConfig::from_json(
            format!(r#"{{"witnesses": [{{"identity": "{ALICE}", "endpoints": ["{key}"]}}]}}"#)
                .as_bytes(),
        )
        .expect("an entry loads");
        assert_eq!(
            config.witnesses,
            [WitnessEntry::new(ALICE.parse().unwrap(), vec![key])]
        );
        assert_eq!(config.witness_endpoints(ALICE.parse().unwrap()), [key]);
        assert_eq!(config.witness_bootstrap(), [key]);

        // No endpoints at all is legal: resolution can still reach the witness
        // through a local copy, a hint or DNS.
        let bare = NodeConfig::from_json(format!(r#"{{"witnesses": ["{ALICE}"]}}"#).as_bytes())
            .expect("a bare identity id loads");
        assert_eq!(
            bare.witnesses,
            [WitnessEntry::new(ALICE.parse().unwrap(), Vec::new())]
        );

        let error =
            NodeConfig::from_json(format!(r#"{{"witnesses": ["{ALICE}", "{ALICE}"]}}"#).as_bytes())
                .expect_err("a repeat is a load error");
        assert!(error.to_string().contains("twice"), "{error}");
    }

    /// The pre-proposal-006 file held 64-character hex endpoint ids. It fails
    /// to load and the message names the command that writes the new shape.
    #[test]
    fn an_old_witnesses_array_of_endpoint_ids_fails_to_load() {
        let key = iroh_base::SecretKey::from_bytes(&[3u8; 32]).public();
        let hex = key.to_string();
        assert_eq!(hex.len(), 64, "an endpoint id renders as hex");
        let error = NodeConfig::from_json(format!(r#"{{"witnesses": ["{hex}"]}}"#).as_bytes())
            .expect_err("the old shape is refused");
        let message = error.to_string();
        assert!(message.contains(&hex), "{message}");
        assert!(
            message.contains("mabel witness set-default --witness <mabel-id>"),
            "{message}"
        );
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

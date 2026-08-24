//! `bindings/<identity_id>.json`: which machines a witness identity's own
//! ledger vouches for (proposal 006 section 4.2).
//!
//! A pusher dials an endpoint id, which is an ed25519 public key, so QUIC
//! proves the remote holds that key's secret. A binding is the other half of
//! the proof: an `EndpointAdvertisement` on the witness identity's own chain,
//! served by somebody other than the endpoint it names. With both, the pusher
//! knows the machine it dialled is one a controller of that identity named.
//! Nothing here authorizes anything: the witness still authorizes no request on
//! transport identity (proposal 001 section 4), and a `hinted` endpoint is
//! pushed to anyway, with a warning.
//!
//! An endpoint `E` is [`Binding::Verified`] for identity `W` when all four
//! conditions of section 4.2 hold: a chain for `W` folds clean, its seq-0 event
//! hashes to `W`, its folded `endpoints()` holds `E`, and the chain came from a
//! source other than `E`. The first two are the fetch path's
//! ([`crate::wallet::WalletSync::candidate`] refuses anything else); the third
//! and fourth are [`apply`], which drops every endpoint that served its own
//! evidence. A former endpoint replaying a prefix that still names it stays
//! hinted, because the only evidence for it came from itself.
//!
//! The file is a derived cache and may be deleted: losing it costs one round of
//! hinted labels. It is not a copy of anyone's ledger, so the crawler's rule
//! stands (proposal 003 section 3).

use std::fs;

use iroh_base::EndpointId;
use mabel_core::{EventId, IdentityId};
use serde::{Deserialize, Serialize};

use crate::atomic::{DATA_MODE, create_dir, write_atomic};
use crate::error::{Result, io_at, json_at};
use crate::home::NodeHome;

/// Directory of the binding cache, beside `peers.json` and `verification/`.
pub const BINDINGS_DIR: &str = "bindings";

/// Whether a witness identity's own ledger vouches for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Binding {
    /// The four conditions of proposal 006 section 4.2 hold.
    Verified,
    /// Anything else, including an endpoint that only served its own
    /// advertisement.
    Hinted,
}

impl Binding {
    /// The JSON and text spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Hinted => "hinted",
        }
    }
}

/// One endpoint a witness identity's chain named, and the observation that
/// verified it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundEndpoint {
    /// The endpoint the chain named.
    pub endpoint: EndpointId,
    /// Where the chain that named it ended.
    pub head_seq: u64,
    /// The event at that position, which is what makes two chains at the same
    /// head seq comparable: equal seq and a different event is equivocation.
    pub head_event: EventId,
    /// The endpoint that served the chain, never [`BoundEndpoint::endpoint`].
    pub source: EndpointId,
    /// When it was served.
    pub observed_ms: u64,
}

/// The contents of `bindings/<identity_id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bindings {
    /// The witness identity these endpoints answer for.
    pub identity: IdentityId,
    /// One entry per verified endpoint.
    pub endpoints: Vec<BoundEndpoint>,
}

impl Bindings {
    /// An empty record for `identity`.
    #[must_use]
    pub const fn new(identity: IdentityId) -> Self {
        Self {
            identity,
            endpoints: Vec::new(),
        }
    }

    /// Whether this identity's ledger vouches for `endpoint`.
    #[must_use]
    pub fn binding(&self, endpoint: EndpointId) -> Binding {
        if self
            .endpoints
            .iter()
            .any(|bound| bound.endpoint == endpoint)
        {
            Binding::Verified
        } else {
            Binding::Hinted
        }
    }

    /// The highest head seq any entry derives from, which is the one a new
    /// observation is compared against.
    #[must_use]
    pub fn head_seq(&self) -> Option<u64> {
        self.endpoints.iter().map(|bound| bound.head_seq).max()
    }
}

/// One chain of a witness identity's own, as one source served it.
///
/// `endpoints` is the folded `endpoints()` of that chain, and `head_seq` and
/// `head_event` are where it ended. Conditions 1 and 2 of section 4.2 are the
/// caller's: an [`Observation`] is only built from a chain that folded clean and
/// whose seq-0 event hashes to `identity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// The witness identity whose chain this is.
    pub identity: IdentityId,
    /// Where the chain ended.
    pub head_seq: u64,
    /// The event at that position.
    pub head_event: EventId,
    /// The endpoints the chain advertises.
    pub endpoints: Vec<EndpointId>,
    /// The endpoint that served it.
    pub source: EndpointId,
    /// When it was served.
    pub observed_ms: u64,
    /// Whether the source's provenance may establish a binding at all.
    ///
    /// False for an endpoint reached through a ledger's retired tag-11
    /// `WitnessConfig` (proposal 006 section 5, source 7): that field never
    /// promised an identity, so a chain it served neither creates, refreshes nor
    /// clears a binding however clean it folds.
    pub may_bind: bool,
}

impl Observation {
    /// The endpoints this observation may verify: every advertised endpoint
    /// except the one that served the evidence (condition 4), and none at all
    /// when the provenance may not bind.
    #[must_use]
    pub fn vouched(&self) -> Vec<EndpointId> {
        if !self.may_bind {
            return Vec::new();
        }
        self.endpoints
            .iter()
            .copied()
            .filter(|endpoint| *endpoint != self.source)
            .collect()
    }
}

/// What an observation did to the record on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// The entry list was replaced, which is what a strictly greater head seq
    /// does: an endpoint absent from the newer chain drops back to hinted.
    Replaced(Bindings),
    /// Entries were added or refreshed at the head seq already recorded.
    Refreshed(Bindings),
    /// The chain is shorter than the one the record derives from, so it neither
    /// created nor refreshed anything.
    Ignored,
    /// Equal head seq, different event: equivocation, so every binding for this
    /// identity is gone. Which chain is this identity's is exactly what is open.
    Cleared,
    /// The chain vouched for nothing this source could verify, and there was
    /// nothing on disk to change.
    Nothing,
}

impl Recorded {
    /// The record this outcome leaves, or `None` when there is no file.
    #[must_use]
    pub const fn bindings(&self) -> Option<&Bindings> {
        match self {
            Self::Replaced(bindings) | Self::Refreshed(bindings) => Some(bindings),
            _ => None,
        }
    }
}

/// The head-seq rules of proposal 006 section 4.2, over the record on disk.
///
/// Pure, so the rules are testable without a home: [`record`] is this plus the
/// file.
#[must_use]
pub fn apply(existing: Option<&Bindings>, observation: &Observation) -> Recorded {
    if !observation.may_bind {
        // Source 7 evidence changes nothing, including on equivocation: which
        // chain is this identity's is not a question a tag-11 list may answer.
        return Recorded::Nothing;
    }
    let vouched = observation.vouched();
    let entry = |endpoint: EndpointId| BoundEndpoint {
        endpoint,
        head_seq: observation.head_seq,
        head_event: observation.head_event,
        source: observation.source,
        observed_ms: observation.observed_ms,
    };
    let Some(existing) = existing else {
        if vouched.is_empty() {
            return Recorded::Nothing;
        }
        return Recorded::Replaced(Bindings {
            identity: observation.identity,
            endpoints: vouched.into_iter().map(entry).collect(),
        });
    };

    let recorded = existing.head_seq();
    match recorded {
        // A record with no entry says nothing about any head seq.
        None => {
            if vouched.is_empty() {
                Recorded::Nothing
            } else {
                Recorded::Replaced(Bindings {
                    identity: observation.identity,
                    endpoints: vouched.into_iter().map(entry).collect(),
                })
            }
        }
        Some(recorded) if observation.head_seq < recorded => Recorded::Ignored,
        Some(recorded) if observation.head_seq > recorded => Recorded::Replaced(Bindings {
            identity: observation.identity,
            endpoints: vouched.into_iter().map(entry).collect(),
        }),
        Some(recorded) => {
            let divergent = existing.endpoints.iter().any(|bound| {
                bound.head_seq == recorded && bound.head_event != observation.head_event
            });
            if divergent {
                return Recorded::Cleared;
            }
            if vouched.is_empty() {
                return Recorded::Nothing;
            }
            let mut merged = existing.clone();
            for endpoint in vouched {
                match merged
                    .endpoints
                    .iter_mut()
                    .find(|bound| bound.endpoint == endpoint)
                {
                    Some(bound) => *bound = entry(endpoint),
                    None => merged.endpoints.push(entry(endpoint)),
                }
            }
            Recorded::Refreshed(merged)
        }
    }
}

/// Records one observation under `home`, writing, keeping or deleting
/// `bindings/<identity_id>.json`.
///
/// # Errors
///
/// Returns [`crate::error::StorageError::Json`] for a malformed record and
/// [`crate::error::StorageError::Io`] if the file cannot be read, written or
/// removed.
pub fn record(home: &NodeHome, observation: &Observation) -> Result<Recorded> {
    let existing = read(home, observation.identity)?;
    let outcome = apply(existing.as_ref(), observation);
    match &outcome {
        Recorded::Replaced(bindings) | Recorded::Refreshed(bindings) => write(home, bindings)?,
        Recorded::Cleared => remove(home, observation.identity)?,
        Recorded::Ignored | Recorded::Nothing => {}
    }
    Ok(outcome)
}

/// Reads `bindings/<identity_id>.json`, or `None` when there is none.
///
/// # Errors
///
/// Returns [`crate::error::StorageError::Json`] for a malformed file and
/// [`crate::error::StorageError::Io`] if it cannot be read.
pub fn read(home: &NodeHome, identity: IdentityId) -> Result<Option<Bindings>> {
    let path = home.bindings_path(identity);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).map_err(json_at(&path))?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_at(&path)(error)),
    }
}

/// Writes `bindings/<identity_id>.json`.
///
/// # Errors
///
/// Returns [`crate::error::StorageError::Io`] if the write fails.
pub fn write(home: &NodeHome, bindings: &Bindings) -> Result<()> {
    let dir = home.bindings_dir();
    create_dir(&dir)?;
    let path = home.bindings_path(bindings.identity);
    let mut bytes = serde_json::to_vec_pretty(bindings).map_err(json_at(&path))?;
    bytes.push(b'\n');
    write_atomic(&path, &bytes, DATA_MODE)
}

/// Deletes `bindings/<identity_id>.json`, which a caller may also do by hand.
///
/// # Errors
///
/// Returns [`crate::error::StorageError::Io`] if the file is there and cannot
/// be removed.
pub fn remove(home: &NodeHome, identity: IdentityId) -> Result<()> {
    let path = home.bindings_path(identity);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_at(&path)(error)),
    }
}

#[cfg(test)]
mod tests {
    use iroh_base::SecretKey;
    use mabel_core::{EventId, IdentityId};

    use super::{Binding, Bindings, Observation, Recorded, apply};

    fn endpoint(seed: u8) -> iroh_base::EndpointId {
        SecretKey::from_bytes(&[seed; 32]).public()
    }

    fn witness() -> IdentityId {
        IdentityId::from_bytes([0x77; 32])
    }

    fn observed(head_seq: u64, head: u8, endpoints: &[u8], source: u8) -> Observation {
        Observation {
            identity: witness(),
            head_seq,
            head_event: EventId::from_bytes([head; 32]),
            endpoints: endpoints.iter().copied().map(endpoint).collect(),
            source: endpoint(source),
            observed_ms: 1_700_000_000_000 + head_seq,
            may_bind: true,
        }
    }

    /// A chain reached through a ledger's retired tag-11 list establishes no
    /// binding, whatever it advertises (proposal 006 section 5, source 7).
    #[test]
    fn a_legacy_witness_hint_never_binds() {
        let observation = Observation {
            may_bind: false,
            ..observed(4, 9, &[1, 2], 3)
        };
        assert!(observation.vouched().is_empty());
        assert_eq!(apply(None, &observation), Recorded::Nothing);
        let existing = apply(None, &observed(4, 9, &[1, 2], 3));
        let existing = existing.bindings().cloned().expect("a record");
        assert_eq!(
            apply(Some(&existing), &observation),
            Recorded::Nothing,
            "it neither refreshes nor clears what a tag-18 source established"
        );
        assert_eq!(existing.binding(endpoint(1)), Binding::Verified);
    }

    /// Condition 4: a chain served only by the endpoint it vouches for leaves
    /// that endpoint hinted, and writes nothing.
    #[test]
    fn evidence_served_by_its_own_endpoint_creates_no_binding() {
        let observation = observed(41, 1, &[7], 7);
        assert!(observation.vouched().is_empty());
        assert_eq!(apply(None, &observation), Recorded::Nothing);

        // The same chain from a second machine verifies the first.
        let elsewhere = observed(41, 1, &[7], 8);
        let Recorded::Replaced(bindings) = apply(None, &elsewhere) else {
            panic!("a source other than the endpoint verifies it");
        };
        assert_eq!(bindings.binding(endpoint(7)), Binding::Verified);
        assert_eq!(bindings.binding(endpoint(8)), Binding::Hinted);
        assert_eq!(bindings.endpoints[0].source, endpoint(8));
        assert_eq!(bindings.head_seq(), Some(41));
    }

    /// A shorter chain neither creates nor refreshes.
    #[test]
    fn a_lower_head_seq_changes_nothing() {
        let Recorded::Replaced(bindings) = apply(None, &observed(41, 1, &[7], 8)) else {
            panic!("the first observation lands");
        };
        assert_eq!(
            apply(Some(&bindings), &observed(40, 2, &[7, 9], 8)),
            Recorded::Ignored
        );
        // Including one that would have verified an endpoint nothing else does.
        assert_eq!(bindings.binding(endpoint(9)), Binding::Hinted);
    }

    /// A strictly greater head seq replaces the whole list, so an endpoint the
    /// newer chain drops falls back to hinted.
    #[test]
    fn a_higher_head_seq_replaces_the_list() {
        let Recorded::Replaced(first) = apply(None, &observed(41, 1, &[7, 9], 8)) else {
            panic!("the first observation lands");
        };
        assert_eq!(first.binding(endpoint(9)), Binding::Verified);

        let Recorded::Replaced(second) = apply(Some(&first), &observed(42, 2, &[7], 8)) else {
            panic!("a longer chain replaces the list");
        };
        assert_eq!(second.binding(endpoint(7)), Binding::Verified);
        assert_eq!(
            second.binding(endpoint(9)),
            Binding::Hinted,
            "an endpoint the newer advertisement drops is hinted again"
        );
        assert_eq!(second.endpoints.len(), 1);
        assert_eq!(second.head_seq(), Some(42));
    }

    /// Equal head seq and the same event merges, so a second source verifies
    /// the endpoint the first one could not.
    #[test]
    fn an_equal_head_seq_from_another_source_merges() {
        let Recorded::Replaced(first) = apply(None, &observed(41, 1, &[7, 8], 8)) else {
            panic!("the first observation lands");
        };
        assert_eq!(first.binding(endpoint(8)), Binding::Hinted);

        let Recorded::Refreshed(merged) = apply(Some(&first), &observed(41, 1, &[7, 8], 7)) else {
            panic!("the same chain from the other machine merges");
        };
        assert_eq!(merged.binding(endpoint(7)), Binding::Verified);
        assert_eq!(merged.binding(endpoint(8)), Binding::Verified);
        assert_eq!(merged.endpoints.len(), 2);
    }

    /// Equal head seq with a different event is equivocation, and clears every
    /// binding for that identity.
    #[test]
    fn an_equal_head_seq_with_divergent_events_clears_every_binding() {
        let Recorded::Replaced(first) = apply(None, &observed(41, 1, &[7, 9], 8)) else {
            panic!("the first observation lands");
        };
        assert_eq!(
            apply(Some(&first), &observed(41, 2, &[7], 8)),
            Recorded::Cleared
        );
    }

    #[test]
    fn a_record_round_trips_and_refuses_an_unknown_field() {
        let Recorded::Replaced(bindings) = apply(None, &observed(41, 1, &[7], 8)) else {
            panic!("the observation lands");
        };
        let json = serde_json::to_string(&bindings).expect("serializes");
        assert!(json.contains(&witness().to_string()), "{json}");
        assert_eq!(
            serde_json::from_str::<Bindings>(&json).expect("reads back"),
            bindings
        );
        assert!(serde_json::from_str::<Bindings>(r#"{"identity": "x"}"#).is_err());
    }

    /// The file lands under `bindings/`, equivocation deletes it, and a home
    /// with no file is simply unbound: it is a cache.
    #[test]
    fn the_record_lands_under_bindings_and_is_deletable() {
        use crate::config::NodeConfig;
        use crate::home::{HomeOptions, NodeHome};

        let dir = tempfile::tempdir().expect("a temp directory");
        let home = NodeHome::create(dir.path(), &NodeConfig::default(), HomeOptions::default())
            .expect("the home is created");
        assert!(
            super::read(&home, witness())
                .expect("no file is no error")
                .is_none()
        );

        let outcome = super::record(&home, &observed(41, 1, &[7], 8)).expect("the record writes");
        assert!(matches!(outcome, Recorded::Replaced(_)));
        let path = home.bindings_path(witness());
        assert!(path.is_file(), "{}", path.display());
        assert_eq!(
            super::read(&home, witness())
                .expect("the record reads")
                .expect("the file is there")
                .binding(endpoint(7)),
            Binding::Verified
        );

        assert_eq!(
            super::record(&home, &observed(41, 2, &[7], 8)).expect("the record clears"),
            Recorded::Cleared
        );
        assert!(!path.exists(), "equivocation removes the file");
        assert!(
            super::read(&home, witness())
                .expect("no file is no error")
                .is_none()
        );
    }

    #[test]
    fn an_empty_record_says_nothing_about_any_head_seq() {
        let empty = Bindings::new(witness());
        assert_eq!(empty.head_seq(), None);
        assert_eq!(empty.binding(endpoint(7)), Binding::Hinted);
        let Recorded::Replaced(bindings) = apply(Some(&empty), &observed(1, 1, &[7], 8)) else {
            panic!("an empty record takes the first observation");
        };
        assert_eq!(bindings.binding(endpoint(7)), Binding::Verified);
    }
}

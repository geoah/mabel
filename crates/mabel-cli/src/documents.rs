//! The `--json` documents this ticket owns.
//!
//! `contracts/cli/identity-create.json`, `identity-list.json`,
//! `trust-add.json`, `verify-ledger.json` and `verify-trust.json` are
//! normative: a field spelled differently here is a bug here, not in the
//! fixture. The identity document, the trust entries and both verification
//! reports are the types `mabel-node` already shares with the HTTP API, so one
//! surface cannot drift from the other.
//!
//! `mabel trust revoke`, `trust list` and `witness add` have no fixture. They
//! reuse the frozen field names, and `contracts/cli/` grows a case when one is
//! pinned.

use mabel_node::api::documents::{DeclaredKind, Id, TrustEntry};
use serde::Serialize;

/// `mabel identity create --json`.
///
/// A raw-rooted identity carries `active_key` and `reserve_commit`; an
/// identity-rooted one holds no key of its own and omits both.
#[derive(Debug, Serialize)]
pub struct CreatedIdentity {
    /// The new ledger, which is also its inception event id.
    pub identity_id: Id,
    /// What it declares itself to be.
    pub declared_kind: DeclaredKind,
    /// The local label.
    pub alias: String,
    /// The signing key, on a raw root only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_key: Option<Id>,
    /// The reserve-key commitment, on a raw root only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve_commit: Option<Id>,
    /// `timestamp_ms` of the inception event.
    pub created_at_ms: u64,
    /// The inception event, which equals `identity_id`.
    pub inception_event: Id,
    /// Sequence number of the head event, 0 on a new ledger.
    pub head_seq: u64,
    /// Id of the head event.
    pub head_event: Id,
    /// Witness endpoints, empty on a new ledger.
    pub witnesses: Vec<Id>,
}

/// `mabel trust add --json`.
#[derive(Debug, Serialize)]
pub struct AddedTrust {
    /// The ledger that signed the attestation.
    pub issuer: Id,
    /// The identity it names.
    pub subject: Id,
    /// The attestation event.
    pub attestation_event: Id,
    /// Its position.
    pub attestation_seq: u64,
    /// The `timestamp_ms` the event carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
    /// Whether the event was pushed to a witness. Always false here; `sync
    /// push` is ticket 011.
    pub pushed: bool,
}

/// `mabel trust revoke --json`.
#[derive(Debug, Serialize)]
pub struct RevokedTrust {
    /// The ledger that signed the revocation.
    pub issuer: Id,
    /// The identity the revoked attestation names.
    pub subject: Id,
    /// The attestation that is now revoked.
    pub attestation_event: Id,
    /// Its position.
    pub attestation_seq: u64,
    /// The revocation event.
    pub revocation_event: Id,
    /// Its position.
    pub revocation_seq: u64,
    /// The `timestamp_ms` the revocation carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
    /// Whether the event was pushed to a witness. Always false here.
    pub pushed: bool,
}

/// `mabel trust list --json`.
#[derive(Debug, Serialize)]
pub struct TrustList {
    /// The ledger that was read.
    pub issuer: Id,
    /// Sequence number of its head event.
    pub head_seq: u64,
    /// Its head event.
    pub head_event: Id,
    /// Every attestation it issued, revoked ones included.
    pub trust: Vec<TrustEntry>,
}

/// `mabel witness add --json`.
#[derive(Debug, Serialize)]
pub struct AddedWitness {
    /// The identity whose ledger records the set.
    pub identity_id: Id,
    /// The endpoint that was added.
    pub endpoint: Id,
    /// The whole set the new event records.
    pub witnesses: Vec<Id>,
    /// The witness-config event.
    pub event_id: Id,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
}

/// `mabel identity list --json`.
#[derive(Debug, Serialize)]
pub struct IdentityList {
    /// Sorted by ascending `identity_id`.
    pub identities: Vec<mabel_node::api::documents::Identity>,
}

/// `mabel node id --json`.
#[derive(Debug, Serialize)]
pub struct NodeId {
    /// This node's Iroh endpoint id.
    pub endpoint_id: Id,
}

//! The `--json` documents this ticket owns.
//!
//! Every document below is pinned by a fixture in `contracts/cli/`, indexed in
//! `contracts/README.md`. The fixtures are normative: a field spelled
//! differently here is a bug here, not in the fixture. The identity document,
//! the trust entries and both verification reports are the types `mabel-node`
//! already shares with the HTTP API, so one surface cannot drift from the
//! other.
//!
//! The membership documents spell every ledger word as proposal 002 does:
//! `invitation`, never `invite`.

use mabel_core::fold::{InvitationStatus, LedgerRoot};
use mabel_node::api::documents::{DeclaredKind, Id, Profile, Pushed, TrustEntry};
use mabel_proto::v0::Role as ProtoRole;
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
    /// The profile the `ProfileUpdate` at seq 1 left, `null` when the create
    /// named neither a display name nor an email (proposal 005).
    pub profile: Option<Profile>,
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
    /// The witness identity that was added (proposal 006 section 1).
    pub witness: Id,
    /// The whole set the new event records.
    pub witnesses: Vec<Id>,
    /// The `WitnessSet` event.
    pub event_id: Id,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
}

/// `mabel identity endpoints replace --json`.
#[derive(Debug, Serialize)]
pub struct ReplacedEndpoints {
    /// The identity whose chain records the list.
    pub identity_id: Id,
    /// The whole list the new event records, empty when it advertises nothing.
    pub endpoints: Vec<Id>,
    /// What the ledger advertised before this event.
    pub previous: Vec<Id>,
    /// The `EndpointAdvertisement` event.
    pub event_id: Id,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
}

/// `mabel identity share --json` (proposal 006 section 7).
#[derive(Debug, Serialize)]
pub struct SharedIdentity {
    /// The identity the link names.
    pub identity_id: Id,
    /// The link, lowercase, the one string that is shared.
    pub link: String,
    /// The machines the link hints at, in the order it names them.
    pub endpoints: Vec<Id>,
    /// Where the hints came from: `advertised`, `node`, `flag` or `none`.
    pub endpoints_from: EndpointSource,
    /// The `.mabel` file that was written, `null` without `--out`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Its length in bytes, `null` without `--out`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

/// Where the endpoints in a shared link came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSource {
    /// The identity's own `EndpointAdvertisement`.
    Advertised,
    /// This node's endpoint id, because the home signs for the identity and the
    /// chain advertises nothing.
    Node,
    /// The list `--endpoints` named.
    Flag,
    /// Nothing: the link names the identity alone.
    None,
}

impl EndpointSource {
    /// The clause a person reads after the link.
    #[must_use]
    pub const fn clause(self) -> &'static str {
        match self {
            Self::Advertised => "advertised by the identity",
            Self::Node => "this node, which signs for the identity",
            Self::Flag => "named on the command line",
            Self::None => "none: the link names the identity alone",
        }
    }
}

/// The profile a replacement overwrote, every field as the fold reported it.
#[derive(Debug, Serialize)]
pub struct PreviousProfile {
    /// The name that was published before, `null` when there was none.
    pub display_name: Option<String>,
    /// The hostname that was claimed before, `null` when there was none.
    pub hostname: Option<String>,
    /// The email that was published before, `null` when there was none.
    pub email: Option<String>,
}

/// `mabel profile replace --json` (`contracts/cli/profile-replace.json`).
///
/// All three fields are here whether they were set or cleared, because the
/// operation is replacement: `null` is a field the update cleared.
#[derive(Debug, Serialize)]
pub struct ReplacedProfile {
    /// The ledger whose profile was replaced.
    pub identity_id: Id,
    /// The name it now publishes, `null` when the update cleared it.
    pub display_name: Option<String>,
    /// The hostname it now claims, `null` when the update cleared it.
    pub hostname: Option<String>,
    /// The email it now publishes, `null` when the update cleared it.
    pub email: Option<String>,
    /// What the update replaced, which is what the diff printed.
    pub previous: PreviousProfile,
    /// The `ProfileUpdate` event.
    pub profile_event: Id,
    /// Its position.
    pub profile_seq: u64,
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

/// `mabel sync push --json` (`contracts/cli/sync-push.json`).
///
/// The push report of the HTTP route with the identity named beside it, which
/// is what the command was given.
#[derive(Debug, Serialize)]
pub struct PushedLedger {
    /// The identity whose ledger was pushed.
    pub identity_id: Id,
    /// The report, flat beside it.
    #[serde(flatten)]
    pub pushed: Pushed,
}

/// `mabel sync fetch --json` (`contracts/cli/sync-fetch.json`).
#[derive(Debug, Serialize)]
pub struct FetchedLedger {
    /// The ledger that was fetched.
    pub ledger_id: Id,
    /// The endpoint that served it.
    pub source: Id,
    /// Events the source served.
    pub event_count: u64,
    /// Events this fetch newly stored.
    pub stored: u64,
    /// The head this home now holds.
    pub head_seq: u64,
    /// The head event this home now holds.
    pub head_event: Id,
    /// When the source answered.
    pub fetched_at_ms: u64,
    /// The local identity whose key signs for this ledger, when the fetched
    /// chain names one of this home's keys a controller. `null` means the
    /// ledger is stored read-only (ticket 031).
    pub controlled_by: Option<Id>,
}

/// `mabel node id --json`.
#[derive(Debug, Serialize)]
pub struct NodeId {
    /// This node's Iroh endpoint id.
    pub endpoint_id: Id,
}

/// `mabel node ticket --json`.
#[derive(Debug, Serialize)]
pub struct NodeTicket {
    /// This node's Iroh endpoint id, which is what the ticket names.
    pub endpoint_id: Id,
    /// The `IP:PORT` addresses the ticket carries, in the order it carries
    /// them. Empty when the ticket names the endpoint alone.
    pub addrs: Vec<String>,
    /// The `endpoint...` string `--peer` takes.
    pub ticket: String,
}

/// `mabel witness set-default --json`.
#[derive(Debug, Serialize)]
pub struct DefaultWitnesses {
    /// The witness identities `node.json` now names, each with the bootstrap
    /// endpoints recorded beside it (proposal 006 section 5.4).
    pub witnesses: Vec<DefaultWitness>,
}

/// One entry of `node.json.witnesses`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefaultWitness {
    /// The witness identity.
    pub identity_id: Id,
    /// The endpoints recorded beside it, in the order they were given.
    pub endpoints: Vec<Id>,
}

/// Where a ledger's signing authority came from (proposal 002 section 2).
///
/// This is what proposal 001 called the ledger kind. The declared kind is a
/// separate, advisory field and gates nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RootName {
    /// The ledger keys itself.
    Raw,
    /// One founding identity keys it.
    Identity,
}

impl RootName {
    /// The name for a folded root.
    #[must_use]
    pub const fn of(root: LedgerRoot) -> Self {
        match root {
            LedgerRoot::Raw { .. } => Self::Raw,
            LedgerRoot::Identity { .. } => Self::Identity,
        }
    }

    /// The lowercase name, which is also what the text output prints.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Identity => "identity",
        }
    }
}

/// What a principal or an invitation may do (proposal 002 section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleName {
    /// Recorded data with no signing authority.
    Member,
    /// May append to the ledger.
    Controller,
}

impl RoleName {
    /// The name for a folded role, or `None` for a value this build does not
    /// know, which the fold never records.
    #[must_use]
    pub const fn of(role: ProtoRole) -> Option<Self> {
        match role {
            ProtoRole::Member => Some(Self::Member),
            ProtoRole::Controller => Some(Self::Controller),
            ProtoRole::Unspecified => None,
        }
    }

    /// The lowercase name, which is also what the text output prints.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Controller => "controller",
        }
    }
}

/// What became of an invitation (proposal 002 section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusName {
    /// Issued and neither accepted nor cancelled.
    Open,
    /// An acceptance consumed it.
    Accepted,
    /// A removal cancelled it.
    Cancelled,
}

impl StatusName {
    /// The name for a folded status.
    #[must_use]
    pub const fn of(status: InvitationStatus) -> Self {
        match status {
            InvitationStatus::Open => Self::Open,
            InvitationStatus::Accepted => Self::Accepted,
            InvitationStatus::Cancelled => Self::Cancelled,
        }
    }

    /// The lowercase name, which is also what the text output prints.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One entry of a ledger's `principals` array.
#[derive(Debug, Serialize)]
pub struct PrincipalEntry {
    /// The identity the ledger records.
    pub identity: Id,
    /// The key that identity signs under here.
    pub active_key: Id,
    /// What it may do.
    pub role: RoleName,
    /// Whether this is the principal the inception seeded, which no removal
    /// may take off a raw-rooted ledger.
    pub is_root: bool,
}

/// One entry of a ledger's `invitations` array.
#[derive(Debug, Serialize)]
pub struct InvitationEntry {
    /// The invitation event, which an acceptance names.
    pub invitation_event: Id,
    /// Its position.
    pub invitation_seq: u64,
    /// The identity invited.
    pub invitee: Id,
    /// That identity's active key.
    pub invitee_key: Id,
    /// The role offered.
    pub role: RoleName,
    /// Whether it is still open.
    pub status: StatusName,
}

/// `mabel identity export --json`.
#[derive(Debug, Serialize)]
pub struct ExportedIdentity {
    /// The identity the descriptor describes.
    pub identity_id: Id,
    /// What its inception declares it is.
    pub declared_kind: DeclaredKind,
    /// Where its signing authority came from.
    pub root: RootName,
    /// The key it signs under, on a raw root only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_key: Option<Id>,
    /// The witness endpoints the descriptor carries.
    pub witnesses: Vec<Id>,
    /// The file that was written.
    pub path: String,
    /// Its length.
    pub bytes: u64,
}

/// `mabel membership invite --json`.
#[derive(Debug, Serialize)]
pub struct Invited {
    /// The ledger the invitation was appended to.
    pub ledger_id: Id,
    /// The identity that signed it.
    pub by: Id,
    /// The identity invited.
    pub invitee: Id,
    /// That identity's active key, from its descriptor.
    pub invitee_key: Id,
    /// The role offered.
    pub role: RoleName,
    /// The invitation event, which the acceptance names.
    pub invitation_event: Id,
    /// Its position.
    pub invitation_seq: u64,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
    /// The `InvitationBundle` that was written.
    pub path: String,
    /// Its length.
    pub bytes: u64,
    /// Events in the bundle, which are the ledger's `0..=invitation`.
    pub event_count: u64,
}

/// What `mabel membership accept` shows before it signs anything (proposal 002
/// section 4, accept surface).
///
/// `controller_on_raw_root` is the flag a screen reads; `warning` is the
/// sentence a person reads. Both are present exactly when accepting means
/// signing as the ledger's own identity.
#[derive(Debug, Serialize)]
pub struct AcceptSurface {
    /// The ledger the invitation admits into.
    pub ledger_id: Id,
    /// What that ledger declares itself to be. Advisory.
    pub declared_kind: DeclaredKind,
    /// Where its signing authority came from.
    pub root: RootName,
    /// Every identity that may currently append to it.
    pub controllers: Vec<PrincipalEntry>,
    /// The invitation event.
    pub invitation_event: Id,
    /// The identity invited.
    pub invitee: Id,
    /// That identity's active key.
    pub invitee_key: Id,
    /// The role offered.
    pub role: RoleName,
    /// Whether accepting means signing as the ledger's own identity.
    pub controller_on_raw_root: bool,
    /// The warning that flag carries, `null` when it is false.
    pub warning: Option<String>,
}

/// `mabel membership accept --json`: the surface that was shown, and the file
/// that was signed.
#[derive(Debug, Serialize)]
pub struct Accepted {
    /// The surface, flat beside the file fields.
    #[serde(flatten)]
    pub surface: AcceptSurface,
    /// The `AcceptanceFile` that was written.
    pub path: String,
    /// Its length.
    pub bytes: u64,
}

/// `mabel membership admit --json`.
#[derive(Debug, Serialize)]
pub struct Admitted {
    /// The ledger the acceptance was appended to.
    pub ledger_id: Id,
    /// The identity that signed the acceptance event.
    pub by: Id,
    /// The identity admitted.
    pub invitee: Id,
    /// The key it signs under here.
    pub invitee_key: Id,
    /// The role it now holds.
    pub role: RoleName,
    /// The invitation the acceptance consumed.
    pub invitation_event: Id,
    /// The acceptance event.
    pub acceptance_event: Id,
    /// Its position.
    pub acceptance_seq: u64,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
    /// The `AcceptanceFile` that was read.
    pub path: String,
}

/// `mabel membership remove --json`.
#[derive(Debug, Serialize)]
pub struct Removed {
    /// The ledger the removal was appended to.
    pub ledger_id: Id,
    /// The identity that signed it.
    pub by: Id,
    /// The identity removed.
    pub target: Id,
    /// Whether the target held a principal that the removal took away.
    pub principal_removed: bool,
    /// The open invitation the removal cancelled, `null` if there was none.
    pub invitation_cancelled: Option<Id>,
    /// The removal event.
    pub removal_event: Id,
    /// Its position.
    pub removal_seq: u64,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event.
    pub head_event: Id,
}

/// `mabel membership list --json`.
#[derive(Debug, Serialize)]
pub struct Membership {
    /// The ledger that was read.
    pub ledger_id: Id,
    /// What it declares itself to be. Advisory.
    pub declared_kind: DeclaredKind,
    /// Where its signing authority came from.
    pub root: RootName,
    /// Sequence number of its head event.
    pub head_seq: u64,
    /// Its head event.
    pub head_event: Id,
    /// Every identity it records, by ascending id.
    pub principals: Vec<PrincipalEntry>,
    /// Every invitation it issued, by ascending position, accepted and
    /// cancelled ones included.
    pub invitations: Vec<InvitationEntry>,
}

/// `mabel dev seed --json`.
///
/// `identities` is the array `mabel identity list --json` reports, in the same
/// document type, because a seeded home is an ordinary home: the answer to
/// "what did the seed create" is "these identities".
#[derive(Debug, Serialize)]
pub struct SeededHome {
    /// Every identity the seed created, by ascending identity id.
    pub identities: Vec<mabel_node::api::documents::Identity>,
    /// The witness identities every seeded ledger now names. Empty when the
    /// seed was given no ticket, since it wrote no witness set.
    pub witnesses: Vec<Id>,
    /// One entry per ledger pushed, in creation order. Empty when the seed was
    /// given no ticket.
    pub pushed: Vec<Pushed>,
    /// The crawl that ran after the push, `null` when the seed was given no
    /// ticket.
    pub graph: Option<mabel_node::api::documents::GraphStatus>,
}

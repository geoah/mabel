//! The JSON documents of `contracts/http/`.
//!
//! The fixtures under `contracts/http/` are normative: a field these types
//! serialize differently is a bug here, not in the fixture
//! (`contracts/README.md`). Every type derives `Deserialize` as well as
//! `Serialize` so the frozen fixtures can be parsed back into it, which is
//! what the contract tests and [`crate::api::stub`] do, and
//! `deny_unknown_fields` makes a fixture field this module forgot a parse
//! failure rather than a silent drop.
//!
//! Names follow decision 012: snake_case, full words, `storage_capacity` and
//! `declared_kind` regardless of what the internal field is called.

use std::fmt;
use std::net::SocketAddr;

use serde::de::{Error as _, Unexpected};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Characters of a rendered 32-byte value: lowercase RFC 4648 base32, no
/// padding (`contracts/README.md`).
pub const ID_LENGTH: usize = 52;

/// The flag-L sentence every trust report carries verbatim (proposal 001
/// section 6).
pub const SUBJECT_CONTROL_SENTENCE: &str = "subject control was not proven to this verifier; the issuer is responsible for out-of-band confirmation";

/// The pitfall-8 sentence every verification report carries verbatim
/// (proposal 001 section 6).
pub const VERIFIED_MEANS_SENTENCE: &str = "Verified means this identity signed this statement at this position in its chain. It is not proof that the statement is true, not proof of legal identity, and not proof of unique humanity.";

/// A 32-byte value rendered as lowercase base32: an identity id, a ledger id,
/// an event id, a public key or an endpoint id.
///
/// Parsing is case-insensitive and stores the lowercase form, so one value has
/// one spelling in every document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// Parses a rendered id, lowercasing it.
    ///
    /// Returns `None` unless the input is [`ID_LENGTH`] characters of the
    /// base32 alphabet.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.len() != ID_LENGTH {
            return None;
        }
        let lowercase = raw.to_ascii_lowercase();
        if lowercase
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
        {
            Some(Self(lowercase))
        } else {
            None
        }
    }

    /// The rendered form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| {
            D::Error::invalid_value(Unexpected::Str(&raw), &"52 characters of lowercase base32")
        })
    }
}

/// What an identity says it is (`contracts/README.md`, "Declared kind").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeclaredKind {
    /// A person ledger, the only kind `POST /api/identities` mints today.
    Person,
    /// An organization ledger.
    Organization,
    /// Reserved; a node that meets it answers code 70.
    Agent,
    /// Reserved; a node that meets it answers code 70.
    Service,
}

impl DeclaredKind {
    /// The four values, in the order the error message lists them.
    pub const ALL: [Self; 4] = [Self::Person, Self::Organization, Self::Agent, Self::Service];

    /// Parses the JSON spelling.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "person" => Some(Self::Person),
            "organization" => Some(Self::Organization),
            "agent" => Some(Self::Agent),
            "service" => Some(Self::Service),
            _ => None,
        }
    }

    /// The JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Agent => "agent",
            Self::Service => "service",
        }
    }

    /// Whether this build mints and folds the kind.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(self, Self::Person | Self::Organization)
    }
}

impl fmt::Display for DeclaredKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Which role a node runs (proposal 001 section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Holds identity keys and appends events.
    Wallet,
    /// Passive replica.
    Witness,
}

/// The relay setting of the Iroh endpoint, as `GET /api/node` renders it.
///
/// The api layer keeps its own copy of this enum so the HTTP documents do not
/// move when `node.json` changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Relay {
    /// The n0 default relay set.
    N0,
    /// No relays.
    Disabled,
}

/// `GET /api/node` on a wallet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletNode {
    /// Always [`Role::Wallet`].
    pub role: Role,
    /// This node's Iroh endpoint id.
    pub endpoint_id: Id,
    /// Where the HTTP API listens.
    pub http_bind: SocketAddr,
    /// Relay setting.
    pub relay: Relay,
    /// Witness endpoints this node pushes to by default.
    pub witnesses: Vec<Id>,
    /// Bytes of ledger data this node accepts before refusing more.
    pub storage_capacity: u64,
    /// Bytes currently stored.
    pub storage_used: u64,
    /// Identities in this node home.
    pub identity_count: u64,
    /// Build version.
    pub version: String,
}

/// `GET /api/node` on a witness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessNode {
    /// Always [`Role::Witness`].
    pub role: Role,
    /// This node's Iroh endpoint id.
    pub endpoint_id: Id,
    /// Where the HTTP API listens.
    pub http_bind: SocketAddr,
    /// Relay setting.
    pub relay: Relay,
    /// Empty on a witness, which pushes to nobody.
    pub witnesses: Vec<Id>,
    /// Bytes of ledger data this node accepts before refusing more.
    pub storage_capacity: u64,
    /// Bytes currently stored.
    pub storage_used: u64,
    /// Ledgers replicated here.
    pub ledger_count: u64,
    /// Fork records held.
    pub fork_count: u64,
    /// Build version.
    pub version: String,
}

/// One entry of an identity's `trust` array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustEntry {
    /// The attestation event.
    pub attestation_event: Id,
    /// Its sequence number.
    pub attestation_seq: u64,
    /// Who it names.
    pub subject: Id,
    /// Whether a later revocation targets it.
    pub revoked: bool,
    /// The revoking event, `null` when `revoked` is false.
    pub revocation_event: Option<Id>,
    /// Its sequence number, `null` when `revoked` is false.
    pub revocation_seq: Option<u64>,
}

/// Where a ledger's signing authority came from (proposal 002 section 2).
///
/// This is what proposal 001 called the ledger kind. The declared kind is a
/// separate, advisory field and gates nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RootName {
    /// The ledger keys itself.
    Raw,
    /// One founding identity keys it.
    Identity,
}

impl RootName {
    /// The JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Identity => "identity",
        }
    }
}

impl fmt::Display for RootName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// What a principal or an invitation may do (proposal 002 section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleName {
    /// Recorded data with no signing authority.
    Member,
    /// May append to the ledger.
    Controller,
}

impl RoleName {
    /// Both values, in the order an error message lists them.
    pub const ALL: [Self; 2] = [Self::Member, Self::Controller];

    /// Parses the JSON spelling.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "member" => Some(Self::Member),
            "controller" => Some(Self::Controller),
            _ => None,
        }
    }

    /// The JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Controller => "controller",
        }
    }
}

impl fmt::Display for RoleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// What became of an invitation (proposal 002 section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for StatusName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One entry of a ledger's `principals` array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// The identity document (`contracts/README.md`, "Shared documents").
///
/// `active_key` and `reserve_commit` are absent, not null, on an identity that
/// holds no key of its own: the fixtures show an organization without them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// The ledger id, which is also the inception event id.
    pub identity_id: Id,
    /// What the identity says it is.
    pub declared_kind: DeclaredKind,
    /// Local label, never part of the ledger.
    pub alias: String,
    /// Timestamp of the inception event.
    pub created_at_ms: u64,
    /// Sequence number of the head event.
    pub head_seq: u64,
    /// Id of the head event.
    pub head_event: Id,
    /// Events in the ledger, `head_seq + 1`.
    pub event_count: u64,
    /// Witness endpoints from the latest witness config.
    pub witnesses: Vec<Id>,
    /// Attestations this identity issued, revoked ones included.
    pub trust: Vec<TrustEntry>,
    /// The folded principal set, by ascending identity id (proposal 002
    /// section 1). Every ledger has one, raw-rooted or identity-rooted.
    pub principals: Vec<PrincipalEntry>,
    /// Invitations this ledger issued that are still `open`.
    pub open_invitation_count: u64,
    /// The signing key, on a raw root only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_key: Option<Id>,
    /// The reserve-key commitment, on a person only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve_commit: Option<Id>,
}

/// The event document (`contracts/README.md`, "Shared documents").
///
/// `payload_kind` stays a string and `payload` a JSON object: the seven names
/// and their keys are frozen in `contracts/README.md`, but one Rust enum per
/// payload would duplicate `ledger.proto` in a layer that only renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// Digest of the signed event.
    pub event_id: Id,
    /// Position in the chain, 0-based.
    pub seq: u64,
    /// The ledger, `null` in a seq-0 event.
    pub ledger_id: Option<Id>,
    /// The previous event id, `null` in a seq-0 event.
    pub prev: Option<Id>,
    /// When the author signed it.
    pub timestamp_ms: u64,
    /// The key the signature verifies under.
    pub author_key: Id,
    /// The `oneof` tag name from `ledger.proto`, in snake_case.
    pub payload_kind: String,
    /// That variant's fields, under the same names.
    pub payload: Value,
}

/// `POST /api/identities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatedIdentity {
    /// The identity as `GET /api/identities/:identity_id` would return it.
    pub identity: Identity,
    /// The inception event id, which equals `identity.identity_id`.
    pub inception_event: Id,
}

/// `GET /api/identities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityList {
    /// Sorted by ascending `identity_id`, organizations included.
    pub identities: Vec<Identity>,
}

/// `GET /api/identities/:identity_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityView {
    /// The identity.
    pub identity: Identity,
}

/// One page of events, from `GET /api/identities/:identity_id/ledger` and from
/// `GET /api/ledgers/:ledger_id/events`. Both routes return this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerPage {
    /// The ledger these events belong to.
    pub ledger_id: Id,
    /// What the ledger says it is.
    pub declared_kind: DeclaredKind,
    /// The `since` that produced this page, echoed back. Inclusive.
    pub since: u64,
    /// The effective limit after clamping, echoed back.
    pub limit: u32,
    /// Sequence number of the head event.
    pub head_seq: u64,
    /// Id of the head event.
    pub head_event: Id,
    /// Events in the ledger.
    pub event_count: u64,
    /// Whether events past this page exist.
    pub more: bool,
    /// Ascending by `seq`, starting at `seq == since`.
    pub events: Vec<Event>,
}

/// The answer to an append: `POST /api/trust` and
/// `POST /api/identities/:identity_id/witnesses`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appended {
    /// The ledger the event landed in.
    pub ledger_id: Id,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event id.
    pub head_event: Id,
    /// The event that was appended.
    pub event: Event,
}

/// `POST /api/trust/:event_id/revoke`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revoked {
    /// The ledger the revocation landed in.
    pub ledger_id: Id,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event id.
    pub head_event: Id,
    /// The attestation that is now revoked.
    pub revoked_attestation: Id,
    /// Where that attestation sits in the chain.
    pub revoked_attestation_seq: u64,
    /// The revocation event.
    pub event: Event,
}

/// `GET /api/identities/{identity_id}/memberships`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipView {
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

/// `POST /api/identities/{identity_id}/memberships/invitations`.
///
/// `invitation_bundle_base64` is the artifact the invitee needs: the ledger's
/// events `0..=invitation`, base64 of the same bytes `mabel membership invite
/// --out` writes (`contracts/README.md`, "Artifacts over JSON").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// The invitation event that was appended.
    pub event: Event,
    /// The `InvitationBundle` to hand the invitee.
    pub invitation_bundle_base64: String,
    /// Events in that bundle, which are the ledger's `0..=invitation`.
    pub event_count: u64,
}

/// `POST /api/identities/{identity_id}/memberships/acceptances`: the accept
/// surface the node signed under, and the file it signed.
///
/// The surface is what proposal 002 section 4 requires a person to see before
/// anything is signed. The browser holds no keys, so the node signs and
/// answers with both (proposal 001 section 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
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
    /// The identity invited, which is the path parameter.
    pub invitee: Id,
    /// That identity's active key.
    pub invitee_key: Id,
    /// The role offered.
    pub role: RoleName,
    /// Whether accepting means signing as the ledger's own identity.
    pub controller_on_raw_root: bool,
    /// The warning that flag carries, `null` when it is false.
    pub warning: Option<String>,
    /// The `AcceptanceFile` to hand a controller of the ledger.
    pub acceptance_base64: String,
}

/// `POST /api/identities/{identity_id}/memberships/admissions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// The acceptance event that was appended.
    pub event: Event,
}

/// `POST /api/identities/{identity_id}/memberships/removals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// The removal event that was appended.
    pub event: Event,
}

/// What one witness did with a push.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PushStatus {
    /// The witness stored the events, or already held them.
    Accepted,
    /// The witness answered a rejection.
    Rejected,
    /// No answer from the endpoint.
    Unreachable,
}

/// One endpoint's outcome inside a push report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushResult {
    /// The witness endpoint.
    pub endpoint: Id,
    /// What it did.
    pub status: PushStatus,
    /// The head it reports, `null` when it did not answer.
    pub head_seq: Option<u64>,
    /// Events it stored from this push.
    pub stored: u64,
    /// The `Reject.code` name, `null` unless `status` is `rejected`.
    pub reject_code: Option<String>,
    /// Where it rejected, `null` unless `status` is `rejected`.
    pub at_seq: Option<u64>,
    /// One line for a human, `null` when there is nothing to say.
    pub message: Option<String>,
}

/// `POST /api/sync/push`.
///
/// A push where at least one witness accepted answers 200 with the failures
/// listed per endpoint; a push where every witness failed answers 502 with
/// code 30 (`contracts/README.md`, "Decisions taken here").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pushed {
    /// The ledger that was pushed.
    pub ledger_id: Id,
    /// The head this node holds.
    pub head_seq: u64,
    /// The head event this node holds.
    pub head_event: Id,
    /// One entry per endpoint, in the order they were tried.
    pub results: Vec<PushResult>,
}

/// Which report `POST /api/verify` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyKind {
    /// Trust from an issuer to a subject.
    Trust,
    /// One ledger's chain.
    Ledger,
}

/// Whether the verifier could reach the subject's own ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectResolution {
    /// A queried source holds the subject ledger.
    Resolved,
    /// No queried source holds it, which still exits 0.
    Unresolved,
}

/// One revoked attestation inside a trust report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokedAttestation {
    /// The attestation.
    pub attestation_event: Id,
    /// Its sequence number.
    pub attestation_seq: u64,
    /// The event that revoked it.
    pub revocation_event: Id,
    /// That event's sequence number.
    pub revocation_seq: u64,
}

/// The sentence a trust report carries when no queried source holds the
/// subject's own ledger (`contracts/cli/verify-trust.json`).
pub const UNRESOLVED_SUBJECT_NOTE: &str = "subject: unresolved (not held by any queried source)";

/// Who signed the attestation a trust report answers with.
///
/// The `author_key` and the principal identity it matched, so a delegate's
/// signature is never attributed to the ledger subject (proposal 002
/// section 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SigningPrincipal {
    /// The identity whose principal the `author_key` matched.
    pub identity: Id,
    /// The key the event names in `author_key`.
    pub key: Id,
}

/// The trust verification report (`contracts/README.md`, "Verification
/// reports").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustReport {
    /// Always [`VerifyKind::Trust`].
    pub kind: VerifyKind,
    /// Whether an unrevoked attestation exists in `0..=head_seq`.
    pub trusted: bool,
    /// The issuer ledger.
    pub issuer: Id,
    /// The subject the attestation names.
    pub subject: Id,
    /// Whether the subject ledger was reachable.
    pub subject_resolution: SubjectResolution,
    /// The unresolved-subject sentence, `null` when resolved.
    pub subject_note: Option<String>,
    /// The unrevoked attestation, `null` when `trusted` is false.
    pub attestation_event: Option<Id>,
    /// Its sequence number, `null` when `trusted` is false.
    pub attestation_seq: Option<u64>,
    /// Who signed that attestation, `null` when `trusted` is false.
    pub signing_principal: Option<SigningPrincipal>,
    /// How many attestations for this subject were revoked.
    pub revoked_count: u64,
    /// Those attestations.
    pub revoked_attestations: Vec<RevokedAttestation>,
    /// The source the report is as of.
    pub source: Id,
    /// Every source that was asked.
    pub sources_queried: Vec<Id>,
    /// The head the source served.
    pub head_seq: u64,
    /// The head event the source served.
    pub head_event: Id,
    /// When it was fetched.
    pub fetched_at_ms: u64,
    /// The rendered sentence, with the revocation clause.
    pub statement: String,
    /// [`SUBJECT_CONTROL_SENTENCE`], verbatim.
    pub subject_control: String,
    /// [`VERIFIED_MEANS_SENTENCE`], verbatim.
    pub verified_means: String,
}

/// The ledger verification report (`contracts/cli/verify-ledger.json`).
///
/// A chain that breaks part way is a failure, not a report: it answers code 20
/// with these fields inside `details` (`contracts/README.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerReport {
    /// Always [`VerifyKind::Ledger`].
    pub kind: VerifyKind,
    /// The ledger that was verified.
    pub ledger_id: Id,
    /// What the ledger says it is.
    pub declared_kind: DeclaredKind,
    /// Whether the whole chain verified.
    pub valid: bool,
    /// The last sequence number that verified.
    pub valid_to_seq: u64,
    /// Where it broke, `null` when `valid` is true.
    pub failed_at_seq: Option<u64>,
    /// Events in the ledger.
    pub event_count: u64,
    /// The source the report is as of.
    pub source: Id,
    /// Every source that was asked.
    pub sources_queried: Vec<Id>,
    /// The head the source served.
    pub head_seq: u64,
    /// The head event the source served.
    pub head_event: Id,
    /// When it was fetched.
    pub fetched_at_ms: u64,
    /// The rendered sentence, with no revocation clause.
    pub statement: String,
    /// [`VERIFIED_MEANS_SENTENCE`], verbatim.
    pub verified_means: String,
}

/// Either report, discriminated by its own `kind` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VerificationReport {
    /// Trust from an issuer to a subject.
    Trust(TrustReport),
    /// One ledger's chain.
    Ledger(LedgerReport),
}

/// One row of the witness ledger list.
///
/// `LedgerSummary.ledger` from `sync.proto` renders as `ledger_id` and
/// `LedgerSummary.kind` as `declared_kind`; `source_endpoint` comes from
/// `ledgers/<id>/meta.json` (`contracts/README.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerEntry {
    /// The ledger.
    pub ledger_id: Id,
    /// What it says it is.
    pub declared_kind: DeclaredKind,
    /// Sequence number of the head event.
    pub head_seq: u64,
    /// Id of the head event.
    pub head_event: Id,
    /// Events held.
    pub event_count: u64,
    /// When this witness first stored the ledger.
    pub first_seen_ms: u64,
    /// When it last changed.
    pub updated_ms: u64,
    /// Fork records held for it.
    pub fork_count: u64,
    /// Whether fork records were dropped after the cap.
    pub forks_truncated: bool,
    /// The endpoint the events arrived from.
    pub source_endpoint: Id,
}

/// `GET /api/ledgers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerList {
    /// The offset that produced this page, echoed back.
    pub offset: u32,
    /// The effective limit after clamping, echoed back.
    pub limit: u32,
    /// Whether entries past this page exist.
    pub more: bool,
    /// Sorted by ascending `ledger_id`.
    pub entries: Vec<LedgerEntry>,
}

/// `GET /api/ledgers/:ledger_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerView {
    /// The summary row.
    pub entry: LedgerEntry,
    /// Witness endpoints from the ledger's latest witness config.
    pub witnesses: Vec<Id>,
}

/// One fork record: two validly signed events at the same sequence number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkRecord {
    /// The ledger that forked.
    pub ledger_id: Id,
    /// Where it forked.
    pub seq: u64,
    /// When this witness saw the conflict.
    pub observed_ms: u64,
    /// The endpoint the conflicting event arrived from.
    pub source_endpoint: Id,
    /// The event this witness kept.
    pub kept: Event,
    /// The event it recorded instead of storing.
    pub conflicting: Event,
    /// The rendered sentence for a human.
    pub statement: String,
}

/// `GET /api/forks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkList {
    /// The offset that produced this page, echoed back.
    pub offset: u32,
    /// The effective limit after clamping, echoed back.
    pub limit: u32,
    /// Whether records past this page exist.
    pub more: bool,
    /// The fork records.
    pub entries: Vec<ForkRecord>,
}

#[cfg(test)]
mod tests {
    use super::{DeclaredKind, ID_LENGTH, Id};

    #[test]
    fn an_id_parses_case_insensitively_and_renders_lowercase() {
        let lowercase = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
        assert_eq!(lowercase.len(), ID_LENGTH);
        let parsed = Id::parse(&lowercase.to_ascii_uppercase()).expect("52 base32 characters");
        assert_eq!(parsed.as_str(), lowercase);
    }

    #[test]
    fn an_id_of_the_wrong_length_or_alphabet_does_not_parse() {
        assert!(Id::parse("alice").is_none());
        assert!(Id::parse("sfttwjzd").is_none());
        assert!(
            Id::parse(&"1".repeat(ID_LENGTH)).is_none(),
            "1 is not base32"
        );
        assert!(Id::parse(&"a".repeat(ID_LENGTH + 1)).is_none());
    }

    #[test]
    fn declared_kind_round_trips_through_its_json_spelling() {
        for kind in DeclaredKind::ALL {
            assert_eq!(DeclaredKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(DeclaredKind::parse("human"), None);
        assert!(!DeclaredKind::Agent.is_implemented());
        assert!(DeclaredKind::Organization.is_implemented());
    }
}

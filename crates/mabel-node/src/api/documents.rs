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

pub use crate::bindings::Binding;
pub use crate::graph::{Equivocation, TruncatedBy};
pub use crate::verification::VerificationStatus;

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

/// One `node.json.witness_for` entry as `GET /api/node` reports it (proposal
/// 006 sections 4 and 4.1).
///
/// `advertised` is the advertisement invariant: false means the latest local
/// copy of that identity does not name this home's endpoint, so the entry
/// admits no ledger this home does not already store, and `reason` says which
/// of the three ways it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessForRow {
    /// The witness identity `node.json` names.
    pub identity: Id,
    /// Whether that identity's ledger advertises this home.
    pub advertised: bool,
    /// Why it does not, `null` when it does.
    pub reason: Option<String>,
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
    /// The witness identities this home witnesses for, each with whether it
    /// admits a ledger this home does not store (proposal 006 section 4.1).
    pub witness_for: Vec<WitnessForRow>,
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

/// The folded profile of a ledger (proposal 003 section 1, `email` from
/// proposal 005).
///
/// Each `ProfileUpdate` replaces the whole document, so a `null` field is one
/// the last update cleared rather than one it left alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// The name the ledger publishes, `null` when the last update omitted it.
    pub display_name: Option<String>,
    /// The hostname the ledger claims, `null` when the last update omitted
    /// it. Unverified here: the DNS check is [`Verification`].
    pub hostname: Option<String>,
    /// The email the ledger publishes, `null` when the last update omitted it.
    /// Nothing checks that it is deliverable: it is a claim like the rest of
    /// the profile.
    pub email: Option<String>,
    /// Who signed the update, which is not always the ledger's own identity.
    pub signing_principal: SigningPrincipal,
    /// The `ProfileUpdate` event.
    pub event: Id,
    /// Its position in the ledger.
    pub seq: u64,
}

/// A re-check that did not answer, kept beside the decisive result it could
/// not refresh (proposal 003 section 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailedCheck {
    /// When the failed re-check ran.
    pub checked_at_ms: u64,
    /// Why it did not answer.
    pub detail: String,
}

/// The advisory hostname verdict (proposal 003 section 2).
///
/// Always present on an identity document. It never gates ledger validity
/// (decision 015), and `status` is `unclaimed` with every other key `null`
/// when the profile names no hostname.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    /// The hostname the verdict is about, `null` when none is claimed.
    pub hostname: Option<String>,
    /// The verdict.
    pub status: VerificationStatus,
    /// When the lookup behind `status` ran, `null` when none has.
    pub checked_at_ms: Option<u64>,
    /// When this hostname last verified, kept across later verdicts.
    pub last_verified_at_ms: Option<u64>,
    /// Whether the result is over 24 hours old, or was never taken.
    pub stale: bool,
    /// One sentence naming what was queried and what came back.
    pub detail: Option<String>,
    /// The last failed re-check, `null` when the result stands on its own.
    pub unreachable: Option<FailedCheck>,
}

impl Verification {
    /// The verdict for a profile that names no hostname.
    #[must_use]
    pub const fn unclaimed() -> Self {
        Self {
            hostname: None,
            status: VerificationStatus::Unclaimed,
            checked_at_ms: None,
            last_verified_at_ms: None,
            stale: false,
            detail: None,
            unreachable: None,
        }
    }

    /// The verdict for a hostname this node has never checked.
    #[must_use]
    pub fn unchecked(hostname: &str) -> Self {
        Self {
            hostname: Some(hostname.to_owned()),
            status: VerificationStatus::Unverified,
            checked_at_ms: None,
            last_verified_at_ms: None,
            stale: true,
            detail: Some(format!("{hostname} has not been checked on this node")),
            unreachable: None,
        }
    }
}

/// The local private note on one identity (proposal 003 section 1).
///
/// Never signed, never synced, and valid for a foreign identity as well as
/// this node's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    /// A private name for this identity, at most 64 bytes.
    pub nickname: Option<String>,
    /// A private note, at most 512 bytes.
    pub note: Option<String>,
    /// When this node last wrote the file.
    pub updated_at_ms: u64,
}

/// Where a rendered name came from (proposal 003 section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// The ledger's own profile display name.
    Profile,
    /// A local alias or contact nickname, which never left this node.
    Alias,
    /// Nothing but the id.
    None,
}

/// One foreign identity, as every surface renders it (proposal 003 section
/// 4).
///
/// The id is always beside the name, because a name is a claim: the UI never
/// sorts, matches or deduplicates on one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedIdentity {
    /// The identity this row is about.
    pub identity_id: Id,
    /// The name its profile publishes, `null` when it carries none.
    pub display_name: Option<String>,
    /// The email its profile publishes, `null` when it carries none. It comes
    /// from the same source as `display_name`, so a card can show a public
    /// email without a second round trip (proposal 005).
    pub email: Option<String>,
    /// The local alias or contact nickname, `null` when this node records
    /// neither.
    pub alias: Option<String>,
    /// The hostname its profile claims, `null` when it claims none.
    pub hostname: Option<String>,
    /// The advisory verdict on that hostname.
    pub verification_status: VerificationStatus,
    /// Which of the three sources the label came from.
    pub provenance: Provenance,
}

impl ResolvedIdentity {
    /// The row for an identity nothing is known about beyond its id.
    #[must_use]
    pub fn bare(identity_id: Id) -> Self {
        Self {
            identity_id,
            display_name: None,
            email: None,
            alias: None,
            hostname: None,
            verification_status: VerificationStatus::Unclaimed,
            provenance: Provenance::None,
        }
    }
}

/// The identity document (`contracts/README.md`, "Shared documents").
///
/// `active_key` and `reserve_commit` are absent, not null, on an identity that
/// holds no key of its own: the fixtures show an organization without them.
/// Every other key is present with an explicit `null`, so `GET /api/identities`
/// and `GET /api/identities/:identity_id` parse into one type.
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
    /// The identities the latest `WitnessSet` names, which are the identities
    /// that may keep this ledger (proposal 006 section 1).
    pub witnesses: Vec<Id>,
    /// Attestations this identity issued, revoked ones included.
    pub trust: Vec<TrustEntry>,
    /// The folded principal set, by ascending identity id (proposal 002
    /// section 1). Every ledger has one, raw-rooted or identity-rooted.
    pub principals: Vec<PrincipalEntry>,
    /// Invitations this ledger issued that are still `open`.
    pub open_invitation_count: u64,
    /// The fold of the latest `ProfileUpdate`, `null` on a ledger that
    /// carries none.
    pub profile: Option<Profile>,
    /// The advisory verdict on the hostname the profile claims.
    pub verification: Verification,
    /// The local private note, `null` when this node records none.
    pub contact: Option<Contact>,
    /// The signing key, on a raw root only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_key: Option<Id>,
    /// The reserve-key commitment, on a person only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve_commit: Option<Id>,
}

/// The event document (`contracts/README.md`, "Shared documents").
///
/// `payload_kind` stays a string and `payload` a JSON object: the payload names
/// and their keys are frozen in `contracts/README.md`, but one Rust enum per
/// payload would duplicate `ledger.proto` in a layer that only renders. The ten
/// names are the `oneof payload` tags of tags 10 to 19, `witness_set` and
/// `endpoint_advertisement` among them (proposal 006 section 3).
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

/// One identity this home has a local record of and does not control
/// (`contracts/README.md`, "Known identities").
///
/// The first six fields are the [`ResolvedIdentity`] fields minus
/// `provenance`, filled in by the same resolver the lookup route uses. The
/// last five say what this home holds about the identity, and every one of
/// them can be absent: `declared_kind` and `head_seq` need a stored copy, and
/// `degrees` needs a crawl that reached it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownIdentity {
    /// The identity this row is about.
    pub identity_id: Id,
    /// The name its profile publishes, `null` when this home holds none.
    pub display_name: Option<String>,
    /// The local alias or contact nickname, `null` when this home records
    /// neither.
    pub alias: Option<String>,
    /// The email its profile publishes, `null` when this home holds none.
    pub email: Option<String>,
    /// The hostname its profile claims, `null` when it claims none.
    pub hostname: Option<String>,
    /// The advisory verdict on that hostname, read from the cache.
    pub verification_status: VerificationStatus,
    /// What the stored copy's inception declares, `null` when this home stores
    /// no copy.
    pub declared_kind: Option<DeclaredKind>,
    /// Whether `ledgers/<identity_id>/` holds a copy of the ledger.
    pub stored: bool,
    /// Whether any identity in this home holds an unrevoked attestation naming
    /// this one.
    pub trusted: bool,
    /// Edges from the nearest root of the stored crawl generation, `null` when
    /// no crawl reached this identity. `null` is "not in my crawl", never "no
    /// relationship".
    pub degrees: Option<u64>,
    /// The last position of the stored copy, `null` when this home stores
    /// none.
    pub head_seq: Option<u64>,
}

/// `GET /api/identities/known`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownIdentityList {
    /// Sorted by ascending `identity_id`.
    pub identities: Vec<KnownIdentity>,
}

/// `GET /api/identities/:identity_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityView {
    /// The identity.
    pub identity: Identity,
}

/// `GET /api/identities/:identity_id/keys`: both secret keys of one identity,
/// for a person to write down or copy into a password manager.
///
/// Only an identity that holds a key of its own has keys to hand back. A
/// keyless identity-rooted ledger answers code 20 with reason `no_keys_held`
/// instead, so every field here is present on a 200 and none is nullable.
///
/// All four key values are lowercase RFC 4648 base32 without padding, 52
/// characters for 32 bytes, the one spelling every byte field in this module
/// uses (`contracts/README.md`, "Ids and byte fields"). The two secrets are
/// the raw 32 secret-key bytes in that encoding. On disk
/// `identities/<id>/active.key` holds the same bytes as lowercase hex; this
/// document matches the other documents, not the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityKeys {
    /// The identity these keys belong to.
    pub identity_id: Id,
    /// The 32 secret bytes of the key that signs this identity's events.
    pub active_secret_key: Id,
    /// The 32 secret bytes of the key the inception committed to and the POC
    /// never uses.
    pub reserve_secret_key: Id,
    /// The public key of `active_secret_key`, as the ledger's events carry it.
    pub active_key: Id,
    /// The commitment the inception froze for the reserve key.
    pub reserve_commit: Id,
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

/// The answer to an append: `POST /api/trust`,
/// `POST /api/identities/:identity_id/witnesses` and
/// `POST /api/identities/:identity_id/endpoints`.
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
    /// Whether a witness identity's own ledger names this endpoint, on
    /// evidence that did not come from the endpoint itself (proposal 006
    /// section 4.2). `hinted` is never a refusal: the push happened anyway.
    pub binding: Binding,
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

/// Which report `mabel verify` returns.
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
    /// The identities the ledger's latest `WitnessSet` names (proposal 006
    /// section 1).
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

/// The profile a replacement overwrote, every field as the fold reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviousProfile {
    /// The name that was published before, `null` when there was none.
    pub display_name: Option<String>,
    /// The hostname that was claimed before, `null` when there was none.
    pub hostname: Option<String>,
    /// The email that was published before, `null` when there was none.
    pub email: Option<String>,
}

/// `POST /api/identities/{identity_id}/profile`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileReplaced {
    /// The ledger the update landed in.
    pub ledger_id: Id,
    /// The profile the ledger now folds to.
    pub profile: Profile,
    /// What it replaced, which is what the CLI diff prints.
    pub previous: PreviousProfile,
    /// The new head sequence number.
    pub head_seq: u64,
    /// The new head event id.
    pub head_event: Id,
    /// The `ProfileUpdate` that was appended.
    pub event: Event,
}

/// `POST /api/identities/{identity_id}/verification`, which forces a check
/// and waits for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationChecked {
    /// The identity that was checked.
    pub identity_id: Id,
    /// The verdict the check produced, after the cache merged it.
    pub verification: Verification,
}

/// `GET` and `PUT /api/identities/{identity_id}/contact`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactView {
    /// The identity the note is about, local or foreign.
    pub identity_id: Id,
    /// The note, `null` when this node records none.
    pub contact: Option<Contact>,
}

/// One step of a lookup path: who attested, to whom, and how fresh the node
/// it reaches is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupHop {
    /// The ledger that signed the attestation.
    pub from: ResolvedIdentity,
    /// The identity it names.
    pub to: ResolvedIdentity,
    /// The attestation event.
    pub attestation_event: Id,
    /// When the crawl read `to`, `null` when no source served it.
    pub fetched_at_ms: Option<u64>,
    /// Whether `to` was read over 24 hours ago, or not at all.
    pub stale: bool,
    /// Two sources that disagreed about `to`, `null` when they agreed.
    pub equivocation: Option<Equivocation>,
}

/// One path from the root to the target, shortest in this crawl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupPath {
    /// The hops in order, from the root outward.
    pub hops: Vec<LookupHop>,
}

/// One attestation the target currently makes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupTrust {
    /// The identity the attestation names.
    pub subject: ResolvedIdentity,
    /// The attestation event.
    pub attestation_event: Id,
    /// Its position in the target's ledger.
    pub seq: u64,
}

/// One identity in this crawl that attests to the target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupReverseEdge {
    /// The ledger that signed the attestation.
    pub identity: ResolvedIdentity,
    /// The attestation event.
    pub attestation_event: Id,
    /// Its position in that ledger.
    pub seq: u64,
}

/// Who, in this crawl, attests to the target.
///
/// Always labelled: this is who the node happened to read, never who trusts
/// the target in the world (proposal 003 section 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupReverse {
    /// Always `true`.
    pub best_effort: bool,
    /// The attesting identities, ascending by id.
    pub entries: Vec<LookupReverseEdge>,
}

/// `GET /api/lookup/{identity_id}?from=`.
///
/// `degrees: null` means no path was found **within the caps of this crawl**,
/// which is not the same statement as "no relationship" and must never be
/// rendered as one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lookup {
    /// The identity that was looked up.
    pub identity: ResolvedIdentity,
    /// The local root the answer is relative to.
    pub from: ResolvedIdentity,
    /// Edges on the shortest path, `null` when this crawl found none.
    pub degrees: Option<u64>,
    /// Up to three shortest paths, empty when there are none.
    pub paths: Vec<LookupPath>,
    /// The target's own outgoing attestations.
    pub trust: Vec<LookupTrust>,
    /// Who in this crawl attests to the target.
    pub reverse: LookupReverse,
    /// Two sources that disagreed about the target, `null` when they agreed.
    pub equivocation: Option<Equivocation>,
    /// When the crawl read the target, `null` when it did not.
    pub fetched_at_ms: Option<u64>,
    /// Whether the target was read over 24 hours ago, or not at all.
    pub stale: bool,
    /// The generation this answer came from, `null` when no crawl has run.
    pub sync_id: Option<String>,
    /// When that crawl started, `null` when no crawl has run.
    pub last_sync_ms: Option<u64>,
    /// Whether the crawl ran over 24 hours ago, or has never run.
    pub graph_stale: bool,
    /// Whether a cap stopped the crawl short.
    pub graph_truncated: bool,
    /// Which cap, `null` when nothing was cut.
    pub truncated_by: Option<TruncatedBy>,
}

/// One crawl, as `GET /api/graph` and `POST /api/graph/sync` report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphStatus {
    /// The generation the pointer names.
    pub sync_id: String,
    /// When the crawl started, which is what staleness counts from.
    pub last_sync_ms: u64,
    /// The depth the run used, after the 1 to 4 bound.
    pub depth: u32,
    /// The local identities the crawl started from, ascending by id.
    pub roots: Vec<ResolvedIdentity>,
    /// Nodes in the generation.
    pub node_count: u64,
    /// Edges over all nodes.
    pub edge_count: u64,
    /// Ledgers the run asked a fetcher for.
    pub fetch_count: u64,
    /// Whether a cap stopped the walk.
    pub truncated: bool,
    /// Which cap, `null` when nothing was cut.
    pub truncated_by: Option<TruncatedBy>,
    /// Every identity whose sources disagreed, ascending.
    pub equivocations: Vec<Id>,
    /// Whether the crawl ran over 24 hours ago.
    pub stale: bool,
}

/// `GET /api/graph`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphView {
    /// The live generation, `null` when no crawl has run in this home.
    pub graph: Option<GraphStatus>,
}

/// `POST /api/graph/sync`, which always leaves a generation behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphSynced {
    /// The generation this sync wrote.
    pub graph: GraphStatus,
}

/// One witness endpoint this wallet knows, and where it knows it from
/// (proposal 004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessEntry {
    /// The witness endpoint.
    pub endpoint_id: Id,
    /// Every stored ledger whose folded witness config names it, ascending by
    /// id. Empty when only `node.json` names it.
    pub named_by: Vec<Id>,
    /// Whether `node.json` lists it as a node-wide default.
    pub is_node_default: bool,
}

/// `GET /api/witnesses`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessList {
    /// Sorted by ascending `endpoint_id`.
    pub witnesses: Vec<WitnessEntry>,
}

/// One row of a witness's ledger list, as the `List` request serves it.
///
/// This is [`LedgerEntry`] minus the three fields only the witness's own
/// `ledgers/<id>/meta.json` holds: no peer sends `source_endpoint`,
/// `first_seen_ms` or `forks_truncated` (`contracts/README.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessLedgerEntry {
    /// The ledger.
    pub ledger_id: Id,
    /// What it says it is.
    pub declared_kind: DeclaredKind,
    /// Sequence number of the head event.
    pub head_seq: u64,
    /// Id of the head event.
    pub head_event: Id,
    /// Events the witness holds.
    pub event_count: u64,
    /// Fork records it holds for the ledger.
    pub fork_count: u64,
}

/// `GET /api/witnesses/{endpoint_id}/ledgers`, a live proxy of the witness's
/// own ledger list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessLedgers {
    /// The witness that answered.
    pub endpoint_id: Id,
    /// The offset that produced this page, echoed back.
    pub offset: u32,
    /// The effective limit after clamping, echoed back.
    pub limit: u32,
    /// Whether entries past this page exist, as the witness reported it.
    pub more: bool,
    /// Sorted by ascending `ledger_id`, which is what `List` guarantees.
    pub ledgers: Vec<WitnessLedgerEntry>,
}

/// What one TXT lookup of `_mabel.<hostname>.` found (proposal 004).
///
/// A separate vocabulary from [`VerificationStatus`]: this answers "which
/// identity should the wallet navigate to", not "does this ledger's claim
/// hold".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveStatus {
    /// A `mabel=` record at the label carries an identity id that parses.
    Resolved,
    /// The label carries no `mabel=` record.
    NoRecord,
    /// The label carries `mabel=` records and none of them parses as an
    /// identity id.
    MismatchedRecords,
    /// The lookup did not answer.
    Unreachable,
}

/// Which of the three input kinds `?input=` carried (proposal 006 section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveInputKind {
    /// A bare identity id, which needs no lookup.
    Identity,
    /// A hostname, which is looked up once.
    Hostname,
    /// A `mabel://` link, whose endpoints come back as hints.
    Link,
}

impl ResolveInputKind {
    /// The wire spelling, the one `contracts/` freezes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Hostname => "hostname",
            Self::Link => "link",
        }
    }
}

/// `GET /api/resolve?input=`.
///
/// One route for the three things a search box takes: an identity id, a
/// hostname or a link (proposal 006 section 7). `status` reports what DNS said
/// and is `null` on the two kinds that query nothing, since a lookup that never
/// ran has no verdict; the four `ResolveStatus` values stay four.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolved {
    /// What the input was read as.
    pub input_kind: ResolveInputKind,
    /// The identity the input named or the record resolved to, `null` when a
    /// hostname resolved to none.
    pub identity_id: Option<Id>,
    /// The hostname that was queried, `null` on the other two kinds.
    pub hostname: Option<String>,
    /// The machines to ask for that identity: the link's hints, or the
    /// `mabel-endpoints=` records at the same label (proposal 006 section 6).
    /// Empty when the input carried none.
    pub endpoints: Vec<Id>,
    /// What the lookup found, `null` when nothing was queried.
    pub status: Option<ResolveStatus>,
}

/// `POST /api/identities/{identity_id}/fetch`.
///
/// The same document `mabel sync fetch --json` prints
/// (`contracts/cli/sync-fetch.json`): one operation over one wallet core, one
/// shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchedLedger {
    /// The ledger that was fetched.
    pub ledger_id: Id,
    /// The endpoint that served it.
    pub source: Id,
    /// Events the source served.
    pub event_count: u64,
    /// Events this fetch newly stored.
    pub stored: u64,
    /// The head after storing.
    pub head_seq: u64,
    /// The head event after storing.
    pub head_event: Id,
    /// When the source answered.
    pub fetched_at_ms: u64,
    /// The local identity whose key signs for this ledger, `null` when the
    /// chain names none of this home's keys a controller.
    pub controlled_by: Option<Id>,
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

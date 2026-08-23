//! The three file artifacts of proposal 001 section 3.8, as amended by
//! proposal 002 section 7.
//!
//! ```text
//! InvitationBundle   { repeated SignedEvent ledger_prefix }  <= 1 MiB
//! AcceptanceFile     { bytes acceptance; bytes signature }   <= 4 KiB
//! IdentityDescriptor { SignedEvent inception;
//!                      repeated bytes witnesses }            <= 64 KiB
//! ```
//!
//! A file is peer input: each artifact registers a
//! [`MessageDescriptor`](crate::validate::MessageDescriptor) and every read
//! runs the same wire-format validator and field table an event from the
//! network runs. The cap lives in the descriptor, so an oversize input is
//! rejected on its length before the scanner reads a record and before
//! anything allocates in proportion to it (pitfall 7). A caller reading a file
//! from disk compares the file length with [`MAX_INVITATION_BUNDLE_BYTES`],
//! [`MAX_ACCEPTANCE_FILE_BYTES`] or [`MAX_IDENTITY_DESCRIPTOR_BYTES`] before
//! reading the bytes in.
//!
//! Embedded events are carried verbatim in and out: a read takes the byte
//! slice the scanner saw, never a re-encoding of a decoded message, because
//! the event id and the signature cover those exact bytes (pitfall 1).

use iroh_base::{EndpointId, PublicKey};
use mabel_proto::prost::Message;
use mabel_proto::v0 as pb;

use crate::encoding::encode;
use crate::fold::{LedgerRoot, LedgerState, SigningPrincipal, Violation, fold};
use crate::id::{EventId, IdentityId, LedgerId};
use crate::sign::DetachedAcceptance;
use crate::validate::{
    self, ACCEPTANCE, Cardinality, FieldDescriptor, FieldKind, MessageDescriptor, SIGNED_EVENT,
    Scanned, WireError,
};
use crate::{
    ID_BYTES, MAX_ACCEPTANCE_BYTES, MAX_ACCEPTANCE_FILE_BYTES, MAX_BUNDLE_EVENTS,
    MAX_IDENTITY_DESCRIPTOR_BYTES, MAX_INVITATION_BUNDLE_BYTES, MAX_WITNESSES, SIG_BYTES,
};

/// A 32-byte identity id, event id, public key or endpoint id.
const ID: FieldKind = FieldKind::Bytes {
    exact: Some(ID_BYTES),
    max: ID_BYTES,
};

/// A 64-byte ed25519 signature.
const SIG: FieldKind = FieldKind::Bytes {
    exact: Some(SIG_BYTES),
    max: SIG_BYTES,
};

/// A `SignedEvent` submessage, capped at 4096 bytes by its own descriptor.
/// Detached: event bytes stand alone outside the bundle, so their nesting
/// budget is their own, not the bundle's.
const EVENT: FieldKind = FieldKind::Detached {
    descriptor: &SIGNED_EVENT,
};

/// `InvitationBundle`: a ledger's events `0..=invitation`.
pub static INVITATION_BUNDLE: MessageDescriptor = MessageDescriptor {
    name: "InvitationBundle",
    max_bytes: MAX_INVITATION_BUNDLE_BYTES,
    fields: &[FieldDescriptor {
        number: 1,
        name: "ledger_prefix",
        cardinality: Cardinality::Repeated {
            min: 1,
            max: MAX_BUNDLE_EVENTS,
            distinct: false,
        },
        kind: EVENT,
    }],
    oneof: None,
    check: None,
};

/// `AcceptanceFile`: the blob an invitee signed and their signature over it.
///
/// The field is `signature`, not `sig` (proposal 002 section 7), and it is
/// checked here under the same rule `MembershipAcceptance` runs, so a file
/// whose signature does not verify never reaches the fold.
pub static ACCEPTANCE_FILE: MessageDescriptor = MessageDescriptor {
    name: "AcceptanceFile",
    max_bytes: MAX_ACCEPTANCE_FILE_BYTES,
    fields: &[
        FieldDescriptor {
            number: 1,
            name: "acceptance",
            cardinality: Cardinality::Required,
            kind: FieldKind::Nested {
                descriptor: &ACCEPTANCE,
                max: MAX_ACCEPTANCE_BYTES,
            },
        },
        FieldDescriptor {
            number: 2,
            name: "signature",
            cardinality: Cardinality::Required,
            kind: SIG,
        },
    ],
    oneof: None,
    check: Some(check_acceptance_file),
};

/// `IdentityDescriptor`: an identity's seq-0 event and its witness endpoints.
///
/// That the inception is a valid seq-0 event is the fold's answer, not the
/// field table's, so [`IdentityDescriptor::read`] folds it.
pub static IDENTITY_DESCRIPTOR: MessageDescriptor = MessageDescriptor {
    name: "IdentityDescriptor",
    max_bytes: MAX_IDENTITY_DESCRIPTOR_BYTES,
    fields: &[
        FieldDescriptor {
            number: 1,
            name: "inception",
            cardinality: Cardinality::Required,
            kind: EVENT,
        },
        FieldDescriptor {
            number: 2,
            name: "witnesses",
            cardinality: Cardinality::Repeated {
                min: 0,
                max: MAX_WITNESSES,
                distinct: true,
            },
            kind: ID,
        },
    ],
    oneof: None,
    check: None,
};

fn check_acceptance_file(scanned: &Scanned<'_>) -> Result<(), WireError> {
    let blob = scanned.bytes(1).expect("acceptance is required");
    let signature = scanned.bytes(2).expect("signature is required");
    validate::detached_acceptance(blob, signature, "AcceptanceFile")
}

/// Why a byte string is not the artifact it claims to be.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ArtifactError {
    /// The bytes failed the size cap, the wire-format validator or the
    /// stateless field table.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// An `InvitationBundle` carried no events.
    #[error("InvitationBundle.ledger_prefix is empty")]
    EmptyPrefix,
    /// An `InvitationBundle`'s events do not fold into a valid ledger.
    #[error("InvitationBundle.ledger_prefix does not verify: {0}")]
    Prefix(Violation),
    /// An `InvitationBundle`'s last event is not the invitation the bundle
    /// exists to carry.
    #[error("the last event of InvitationBundle.ledger_prefix is not a MembershipInvitation")]
    InvitationNotLast,
    /// An `IdentityDescriptor`'s event is not a valid inception.
    #[error("IdentityDescriptor.inception does not verify: {0}")]
    Inception(Violation),
}

impl ArtifactError {
    /// A stable snake-case name for this rejection class, which the CLI maps
    /// to its exit code.
    ///
    /// A prefix or inception that does not verify keeps its own code here, so
    /// a caller separates a malformed file from a file holding a well-formed
    /// but invalid ledger; the [`Violation`] carries why.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Wire(error) => error.code(),
            Self::EmptyPrefix => "empty_prefix",
            Self::Prefix(_) => "invalid_prefix",
            Self::InvitationNotLast => "invitation_not_last",
            Self::Inception(_) => "invalid_inception",
        }
    }
}

/// The events of a ledger from its inception up to and including one
/// `MembershipInvitation`, the file an inviter hands an invitee.
///
/// The bytes of each event are the ones the ledger holds; nothing in this type
/// re-signs or rewrites them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvitationBundle {
    events: Vec<Vec<u8>>,
}

impl InvitationBundle {
    /// Builds a bundle from a ledger's events `0..=invitation`.
    ///
    /// Every event runs the validator and the whole artifact runs its cap, so
    /// a bundle that exists can be written and read back.
    pub fn new(events: Vec<Vec<u8>>) -> Result<Self, ArtifactError> {
        if events.is_empty() {
            return Err(ArtifactError::EmptyPrefix);
        }
        if events.len() > MAX_BUNDLE_EVENTS {
            return Err(WireError::RepeatedCount {
                message: INVITATION_BUNDLE.name,
                field: "ledger_prefix",
                count: events.len(),
                min: 1,
                max: MAX_BUNDLE_EVENTS,
            }
            .into());
        }
        for event in &events {
            validate::signed_event(event)?;
        }
        let bundle = Self { events };
        cap(&INVITATION_BUNDLE, bundle.write().len())?;
        Ok(bundle)
    }

    /// Reads an encoded `InvitationBundle`.
    ///
    /// The cap is checked on the input's length before the scan, and each
    /// event is taken from the input verbatim.
    pub fn read(bytes: &[u8]) -> Result<Self, ArtifactError> {
        let scanned = validate::message_fields(&INVITATION_BUNDLE, bytes)?;
        Ok(Self {
            events: scanned.repeated_bytes(1).map(<[u8]>::to_vec).collect(),
        })
    }

    /// Encodes the artifact.
    ///
    /// Each event's `body` and `signature` cross into the file untouched, so
    /// [`InvitationBundle::read`] hands back the same event bytes this bundle
    /// holds.
    pub fn write(&self) -> Vec<u8> {
        encode(&pb::InvitationBundle {
            ledger_prefix: self
                .events
                .iter()
                .map(|event| pb::SignedEvent::decode(&event[..]).expect("a checked event decodes"))
                .collect(),
        })
    }

    /// The events, in ledger order, exactly as the ledger holds them.
    pub fn events(&self) -> &[Vec<u8>] {
        &self.events
    }

    /// Folds the prefix from inception and reports what the accept surface
    /// must show before anyone signs (proposal 002 section 4).
    ///
    /// The invitation is the bundle's last event: the prefix runs `0..=`
    /// that event, so the controllers in the summary are the ones in force
    /// where the invitation sits.
    pub fn summary(&self) -> Result<InvitationSummary, ArtifactError> {
        let (state, violation) = fold(&self.events);
        if let Some(violation) = violation {
            return Err(ArtifactError::Prefix(violation));
        }
        let head = state.head().ok_or(ArtifactError::EmptyPrefix)?;
        let invitation = state
            .invitation(&head.event_id)
            .ok_or(ArtifactError::InvitationNotLast)?;
        Ok(InvitationSummary {
            ledger: state.ledger().expect("a folded ledger has an id"),
            declared_kind: state
                .declared_kind()
                .expect("a folded ledger has a declared kind"),
            root: state.root().expect("a folded ledger has a root"),
            controllers: controllers(&state),
            invitation_event: head.event_id,
            invitee: invitation.invitee,
            invitee_key: invitation.invitee_key,
            role: invitation.role,
        })
    }
}

/// What `mabel membership accept` and the wallet screen show before signing
/// (proposal 002 section 4, accept surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvitationSummary {
    /// The ledger the invitation admits into.
    pub ledger: LedgerId,
    /// What that ledger's inception declares it is. Advisory: it gates
    /// nothing (proposal 002 section 3).
    pub declared_kind: pb::DeclaredKind,
    /// Where the ledger's signing authority came from.
    pub root: LedgerRoot,
    /// Every identity that may currently append, with the key it signs under.
    pub controllers: Vec<SigningPrincipal>,
    /// The `event_id` of the invitation, which the acceptance names.
    pub invitation_event: EventId,
    /// The identity invited.
    pub invitee: IdentityId,
    /// That identity's active key.
    pub invitee_key: PublicKey,
    /// The role offered.
    pub role: pb::Role,
}

impl InvitationSummary {
    /// Whether accepting means signing as the ledger's own identity, which the
    /// accept surface must warn about explicitly.
    ///
    /// A `CONTROLLER` on a raw-rooted ledger appends events that ledger's
    /// subject is answerable for, because the ledger id and the identity are
    /// the same thing there (proposal 002 section 4).
    pub const fn controller_on_raw_root(&self) -> bool {
        matches!(self.role, pb::Role::Controller) && self.root.is_raw()
    }
}

/// An invitee's detached acceptance as it crosses machines: the `Acceptance`
/// blob and the invitee's signature over `accept_input` of it.
///
/// Reading one proves the signature; a controller then embeds these same bytes
/// in a `MembershipAcceptance` (proposal 001 section 3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceFile {
    acceptance: Vec<u8>,
    signature: [u8; SIG_BYTES],
    ledger: LedgerId,
    invitation_event: EventId,
    invitee: IdentityId,
    invitee_key: PublicKey,
}

impl AcceptanceFile {
    /// Builds the file from what [`crate::build_acceptance`] returned.
    pub fn new(accepted: &DetachedAcceptance) -> Result<Self, ArtifactError> {
        Self::read(&encode(&pb::AcceptanceFile {
            acceptance: accepted.acceptance.clone(),
            signature: accepted.signature.to_vec(),
        }))
    }

    /// Reads an encoded `AcceptanceFile`, signature included.
    pub fn read(bytes: &[u8]) -> Result<Self, ArtifactError> {
        let scanned = validate::message_fields(&ACCEPTANCE_FILE, bytes)?;
        let acceptance = scanned.bytes(1).expect("acceptance is required");
        let signature = scanned.bytes(2).expect("signature is required");
        let blob = pb::Acceptance::decode(acceptance).expect("a validated Acceptance decodes");
        let invitee_key: [u8; ID_BYTES] = blob.invitee_key[..]
            .try_into()
            .expect("invitee_key is 32 bytes");
        Ok(Self {
            acceptance: acceptance.to_vec(),
            signature: signature
                .try_into()
                .expect("a validated signature is 64 bytes"),
            ledger: id(&blob.ledger),
            invitation_event: EventId::from_slice(&blob.invitation_event)
                .expect("invitation_event is 32 bytes"),
            invitee: id(&blob.invitee),
            // The signature check the descriptor ran verified under this key,
            // so it is a curve point.
            invitee_key: PublicKey::from_bytes(&invitee_key).expect("a checked key is a point"),
        })
    }

    /// Encodes the artifact, carrying the signed blob verbatim.
    pub fn write(&self) -> Vec<u8> {
        encode(&pb::AcceptanceFile {
            acceptance: self.acceptance.clone(),
            signature: self.signature.to_vec(),
        })
    }

    /// The blob and signature a `MembershipAcceptance` embeds.
    pub fn detached(&self) -> DetachedAcceptance {
        DetachedAcceptance {
            acceptance: self.acceptance.clone(),
            signature: self.signature,
        }
    }

    /// The ledger the acceptance is for.
    pub const fn ledger(&self) -> LedgerId {
        self.ledger
    }

    /// The invitation the acceptance consumes.
    pub const fn invitation_event(&self) -> EventId {
        self.invitation_event
    }

    /// The identity accepting.
    pub const fn invitee(&self) -> IdentityId {
        self.invitee
    }

    /// The key that signed the blob, which is the invitee's active key.
    pub const fn invitee_key(&self) -> PublicKey {
        self.invitee_key
    }
}

/// An identity's seq-0 event and the witnesses that serve it, the artifact
/// `identity export` writes and `membership invite` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityDescriptor {
    inception: Vec<u8>,
    witnesses: Vec<EndpointId>,
    identity: IdentityId,
    declared_kind: pb::DeclaredKind,
    root: LedgerRoot,
}

impl IdentityDescriptor {
    /// Builds a descriptor around an identity's seq-0 `SignedEvent` bytes.
    pub fn new(inception: &[u8], witnesses: &[EndpointId]) -> Result<Self, ArtifactError> {
        if witnesses.len() > MAX_WITNESSES {
            return Err(WireError::RepeatedCount {
                message: IDENTITY_DESCRIPTOR.name,
                field: "witnesses",
                count: witnesses.len(),
                min: 0,
                max: MAX_WITNESSES,
            }
            .into());
        }
        Self::from_parts(inception.to_vec(), witnesses.to_vec())
    }

    /// Reads an encoded `IdentityDescriptor` and folds its inception.
    pub fn read(bytes: &[u8]) -> Result<Self, ArtifactError> {
        let scanned = validate::message_fields(&IDENTITY_DESCRIPTOR, bytes)?;
        let inception = scanned.bytes(1).expect("inception is required").to_vec();
        let mut witnesses = Vec::new();
        for witness in scanned.repeated_bytes(2) {
            let bytes: [u8; ID_BYTES] = witness.try_into().expect("a witness is 32 bytes");
            witnesses.push(EndpointId::from_bytes(&bytes).map_err(|_| {
                WireError::InvalidPublicKey {
                    message: IDENTITY_DESCRIPTOR.name,
                    field: "witnesses",
                }
            })?);
        }
        Self::from_parts(inception, witnesses)
    }

    /// Encodes the artifact, carrying the inception verbatim.
    pub fn write(&self) -> Vec<u8> {
        encode(&pb::IdentityDescriptor {
            inception: Some(
                pb::SignedEvent::decode(&self.inception[..]).expect("a checked event decodes"),
            ),
            witnesses: self
                .witnesses
                .iter()
                .map(|witness| witness.as_bytes().to_vec())
                .collect(),
        })
    }

    /// The seq-0 `SignedEvent` bytes, which an invitation embeds as they are.
    pub fn inception(&self) -> &[u8] {
        &self.inception
    }

    /// The identity the inception creates.
    pub const fn identity(&self) -> IdentityId {
        self.identity
    }

    /// What the inception declares this identity is. Advisory (proposal 002
    /// section 3).
    pub const fn declared_kind(&self) -> pb::DeclaredKind {
        self.declared_kind
    }

    /// Where the identity's ledger takes its signing authority from.
    pub const fn root(&self) -> LedgerRoot {
        self.root
    }

    /// The endpoints that serve this identity's ledger.
    pub fn witnesses(&self) -> &[EndpointId] {
        &self.witnesses
    }

    /// The key this identity signs under, which only a raw root records.
    ///
    /// An identity-rooted ledger holds no key of its own, so it cannot be
    /// invited anywhere: an invitation's embedded inception must be raw-rooted
    /// (proposal 002 section 8).
    pub const fn active_key(&self) -> Option<PublicKey> {
        match self.root {
            LedgerRoot::Raw { active_key, .. } => Some(active_key),
            LedgerRoot::Identity { .. } => None,
        }
    }

    fn from_parts(inception: Vec<u8>, witnesses: Vec<EndpointId>) -> Result<Self, ArtifactError> {
        let (state, violation) = fold([&inception]);
        if let Some(violation) = violation {
            return Err(ArtifactError::Inception(violation));
        }
        let descriptor = Self {
            inception,
            witnesses,
            identity: state.ledger().expect("a folded ledger has an id"),
            declared_kind: state
                .declared_kind()
                .expect("a folded ledger has a declared kind"),
            root: state.root().expect("a folded ledger has a root"),
        };
        cap(&IDENTITY_DESCRIPTOR, descriptor.write().len())?;
        Ok(descriptor)
    }
}

/// Every identity that may currently append, with the key it signs under.
fn controllers(state: &LedgerState) -> Vec<SigningPrincipal> {
    state
        .principals()
        .iter()
        .filter(|(_, principal)| principal.role == pb::Role::Controller)
        .map(|(identity, principal)| SigningPrincipal {
            identity: *identity,
            key: principal.active_key,
        })
        .collect()
}

/// Rejects an artifact this crate is about to write that would not fit its
/// cap, so nothing writes a file no reader would accept.
fn cap(descriptor: &'static MessageDescriptor, len: usize) -> Result<(), WireError> {
    if len > descriptor.max_bytes {
        return Err(WireError::MessageTooLarge {
            message: descriptor.name,
            len,
            cap: descriptor.max_bytes,
        });
    }
    Ok(())
}

/// Reads a 32-byte id the field table already length-checked.
fn id(bytes: &[u8]) -> IdentityId {
    IdentityId::from_slice(bytes).expect("an id is 32 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::Reason;
    use crate::sign::{
        BuiltEvent, Position, Root, build_acceptance, build_inception, build_membership_acceptance,
        build_membership_invitation, build_trust_attestation,
    };
    use crate::{MAX_EVENT_BYTES, NONCE_BYTES};
    use iroh_base::SecretKey;
    use pb::{DeclaredKind, Role};

    const T0: u64 = 1_700_000_000_000;

    fn secret(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    /// A raw-rooted ledger keyed by `secret(seed)`.
    fn raw_rooted(seed: u8, nonce: u8) -> BuiltEvent {
        build_inception(
            &secret(seed),
            DeclaredKind::Person,
            Root::Raw {
                reserve_key: &secret(seed.wrapping_add(1)).public(),
            },
            [nonce; NONCE_BYTES],
            T0,
        )
        .expect("builds")
    }

    /// An identity-rooted ledger founded by `founder`, signed by the
    /// founder's key.
    fn founded_by(signer: &SecretKey, founder: &BuiltEvent) -> BuiltEvent {
        build_inception(
            signer,
            DeclaredKind::Organization,
            Root::Identity {
                founder: founder.event_id.into(),
                founder_inception: &founder.signed_event,
            },
            [0xc1; NONCE_BYTES],
            T0,
        )
        .expect("builds")
    }

    fn after(event: &BuiltEvent, ledger: LedgerId, seq: u64) -> Position {
        Position {
            ledger,
            seq,
            prev: event.event_id,
            prev_timestamp_ms: T0,
        }
    }

    /// Alice's raw-rooted ledger with one invitation of Bob at seq 1.
    fn invited(role: Role) -> (BuiltEvent, BuiltEvent, BuiltEvent) {
        let alice = raw_rooted(1, 3);
        let bob = raw_rooted(7, 4);
        let ledger = alice.event_id.into();
        let invitation = build_membership_invitation(
            &secret(1),
            &after(&alice, ledger, 1),
            bob.event_id.into(),
            &secret(7).public(),
            role,
            &bob.signed_event,
            T0,
        )
        .expect("builds");
        (alice, bob, invitation)
    }

    fn bundle_of(events: &[&BuiltEvent]) -> InvitationBundle {
        InvitationBundle::new(events.iter().map(|e| e.signed_event.clone()).collect())
            .expect("the events build a bundle")
    }

    #[test]
    fn a_bundle_round_trips_and_carries_its_events_verbatim() {
        let (alice, _, invitation) = invited(Role::Controller);
        let bundle = bundle_of(&[&alice, &invitation]);
        let bytes = bundle.write();

        let read = InvitationBundle::read(&bytes).expect("reads");
        assert_eq!(read, bundle);
        assert_eq!(
            read.events(),
            [alice.signed_event.clone(), invitation.signed_event.clone()]
        );
        assert_eq!(read.write(), bytes);
    }

    #[test]
    fn a_controller_offer_on_a_raw_rooted_ledger_warns() {
        let (alice, bob, invitation) = invited(Role::Controller);
        let summary = bundle_of(&[&alice, &invitation])
            .summary()
            .expect("the prefix folds");

        assert_eq!(summary.ledger, alice.event_id.into());
        assert_eq!(summary.declared_kind, DeclaredKind::Person);
        assert_eq!(
            summary.root,
            LedgerRoot::Raw {
                active_key: secret(1).public(),
                reserve_commit: crate::digest::reserve_commit(&secret(2).public()),
            }
        );
        assert_eq!(
            summary.controllers,
            [SigningPrincipal {
                identity: alice.event_id.into(),
                key: secret(1).public(),
            }]
        );
        assert_eq!(summary.invitation_event, invitation.event_id);
        assert_eq!(summary.invitee, bob.event_id.into());
        assert_eq!(summary.invitee_key, secret(7).public());
        assert_eq!(summary.role, Role::Controller);
        // Accepting here means signing as Alice.
        assert!(summary.controller_on_raw_root());
    }

    #[test]
    fn a_member_offer_on_a_raw_rooted_ledger_does_not_warn() {
        let (alice, _, invitation) = invited(Role::Member);
        let summary = bundle_of(&[&alice, &invitation])
            .summary()
            .expect("the prefix folds");
        assert_eq!(summary.role, Role::Member);
        assert!(!summary.controller_on_raw_root());
    }

    #[test]
    fn a_controller_offer_on_an_identity_rooted_ledger_does_not_warn() {
        let alice = raw_rooted(1, 3);
        let bob = raw_rooted(7, 4);
        let org = founded_by(&secret(1), &alice);
        let ledger: LedgerId = org.event_id.into();
        let invitation = build_membership_invitation(
            &secret(1),
            &after(&org, ledger, 1),
            bob.event_id.into(),
            &secret(7).public(),
            Role::Controller,
            &bob.signed_event,
            T0,
        )
        .expect("builds");

        let summary = bundle_of(&[&org, &invitation])
            .summary()
            .expect("the prefix folds");
        assert_eq!(summary.ledger, ledger);
        assert_eq!(summary.declared_kind, DeclaredKind::Organization);
        assert_eq!(
            summary.root,
            LedgerRoot::Identity {
                founder: alice.event_id.into(),
                founder_key: secret(1).public(),
            }
        );
        // The founder is an ordinary controller principal of the org.
        assert_eq!(
            summary.controllers,
            [SigningPrincipal {
                identity: alice.event_id.into(),
                key: secret(1).public(),
            }]
        );
        assert!(!summary.controller_on_raw_root());
    }

    #[test]
    fn an_oversize_bundle_is_rejected_on_its_length() {
        let error = InvitationBundle::read(&vec![0u8; MAX_INVITATION_BUNDLE_BYTES + 1])
            .expect_err("the cap is checked before the scan");
        assert_eq!(
            error,
            WireError::MessageTooLarge {
                message: "InvitationBundle",
                len: MAX_INVITATION_BUNDLE_BYTES + 1,
                cap: MAX_INVITATION_BUNDLE_BYTES,
            }
            .into()
        );
        assert_eq!(error.code(), "message_too_large");
    }

    #[test]
    fn a_truncated_bundle_is_rejected() {
        let (alice, _, invitation) = invited(Role::Controller);
        let mut bytes = bundle_of(&[&alice, &invitation]).write();
        bytes.truncate(bytes.len() - 1);
        assert_eq!(
            InvitationBundle::read(&bytes)
                .expect_err("a truncated file is not a bundle")
                .code(),
            "truncated"
        );
    }

    #[test]
    fn a_bundle_holding_a_malformed_event_is_rejected() {
        let bytes = encode(&pb::InvitationBundle {
            ledger_prefix: vec![pb::SignedEvent {
                body: vec![0xff],
                signature: vec![0u8; SIG_BYTES],
            }],
        });
        assert_eq!(
            InvitationBundle::read(&bytes)
                .expect_err("the event runs the same field table as network input")
                .code(),
            "truncated"
        );
    }

    #[test]
    fn an_empty_bundle_is_rejected() {
        let bytes = encode(&pb::InvitationBundle {
            ledger_prefix: Vec::new(),
        });
        assert_eq!(
            InvitationBundle::read(&bytes).expect_err("a bundle holds at least the inception"),
            WireError::RepeatedCount {
                message: "InvitationBundle",
                field: "ledger_prefix",
                count: 0,
                min: 1,
                max: MAX_BUNDLE_EVENTS,
            }
            .into()
        );
        assert_eq!(
            InvitationBundle::new(Vec::new()),
            Err(ArtifactError::EmptyPrefix)
        );
    }

    #[test]
    fn a_gapped_prefix_reports_the_violation_rather_than_a_summary() {
        let (alice, bob, _) = invited(Role::Controller);
        let ledger: LedgerId = alice.event_id.into();
        // An invitation at seq 2 with nothing at seq 1.
        let gapped = build_membership_invitation(
            &secret(1),
            &after(&alice, ledger, 2),
            bob.event_id.into(),
            &secret(7).public(),
            Role::Controller,
            &bob.signed_event,
            T0,
        )
        .expect("builds");

        assert_eq!(
            bundle_of(&[&alice, &gapped]).summary(),
            Err(ArtifactError::Prefix(Violation {
                seq: 1,
                reason: Reason::WrongSeq {
                    expected: 1,
                    found: 2,
                },
            })),
        );
    }

    #[test]
    fn a_prefix_that_does_not_end_at_the_invitation_is_rejected() {
        let (alice, _, invitation) = invited(Role::Controller);
        let ledger: LedgerId = alice.event_id.into();
        let attestation = build_trust_attestation(
            &secret(1),
            &after(&invitation, ledger, 2),
            IdentityId::from_bytes([9u8; ID_BYTES]),
            T0,
        )
        .expect("builds");

        assert_eq!(
            bundle_of(&[&alice, &invitation, &attestation]).summary(),
            Err(ArtifactError::InvitationNotLast),
        );
        // A prefix holding only the inception names no invitation either.
        assert_eq!(
            bundle_of(&[&alice]).summary(),
            Err(ArtifactError::InvitationNotLast),
        );
    }

    #[test]
    fn a_bundle_whose_events_are_not_a_ledger_is_rejected_at_build_time() {
        let mut broken = raw_rooted(1, 3).signed_event;
        broken.truncate(broken.len() - 1);
        assert_eq!(
            InvitationBundle::new(vec![broken]).expect_err("every event runs the validator"),
            WireError::Truncated {
                message: "SignedEvent"
            }
            .into()
        );
    }

    #[test]
    fn an_acceptance_file_round_trips_and_admits_the_invitee() {
        let (alice, bob, invitation) = invited(Role::Controller);
        let ledger: LedgerId = alice.event_id.into();
        let accepted =
            build_acceptance(&secret(7), ledger, invitation.event_id, bob.event_id.into());
        let file = AcceptanceFile::new(&accepted).expect("the signature verifies");

        let read = AcceptanceFile::read(&file.write()).expect("reads");
        assert_eq!(read, file);
        assert_eq!(read.detached(), accepted);
        assert_eq!(read.ledger(), ledger);
        assert_eq!(read.invitation_event(), invitation.event_id);
        assert_eq!(read.invitee(), bob.event_id.into());
        assert_eq!(read.invitee_key(), secret(7).public());

        // The file is what a controller needs to admit the invitee.
        let admission = build_membership_acceptance(
            &secret(1),
            &after(&invitation, ledger, 2),
            &read.detached(),
            T0,
        )
        .expect("builds");
        let (state, violation) = fold([
            &alice.signed_event,
            &invitation.signed_event,
            &admission.signed_event,
        ]);
        assert_eq!(violation, None);
        assert!(state.principal(&bob.event_id.into()).is_some());
    }

    #[test]
    fn an_oversize_acceptance_file_is_rejected_on_its_length() {
        let error = AcceptanceFile::read(&vec![0u8; MAX_ACCEPTANCE_FILE_BYTES + 1])
            .expect_err("the cap is checked before the scan");
        assert_eq!(
            error,
            WireError::MessageTooLarge {
                message: "AcceptanceFile",
                len: MAX_ACCEPTANCE_FILE_BYTES + 1,
                cap: MAX_ACCEPTANCE_FILE_BYTES,
            }
            .into()
        );
    }

    #[test]
    fn an_acceptance_file_whose_signature_does_not_verify_is_rejected() {
        let (alice, bob, invitation) = invited(Role::Controller);
        let mut accepted = build_acceptance(
            &secret(7),
            alice.event_id.into(),
            invitation.event_id,
            bob.event_id.into(),
        );
        accepted.signature[0] ^= 0x01;
        assert_eq!(
            AcceptanceFile::new(&accepted).expect_err("the signature is checked on read"),
            WireError::BadSignature {
                message: "AcceptanceFile",
                field: "signature",
            }
            .into()
        );
    }

    #[test]
    fn a_truncated_acceptance_file_is_rejected() {
        let (alice, bob, invitation) = invited(Role::Controller);
        let accepted = build_acceptance(
            &secret(7),
            alice.event_id.into(),
            invitation.event_id,
            bob.event_id.into(),
        );
        let mut bytes = AcceptanceFile::new(&accepted).expect("builds").write();
        bytes.truncate(bytes.len() - 1);
        assert_eq!(
            AcceptanceFile::read(&bytes)
                .expect_err("a truncated file is not an acceptance")
                .code(),
            "truncated"
        );
    }

    #[test]
    fn a_descriptor_round_trips_and_names_the_identity_and_its_key() {
        let alice = raw_rooted(1, 3);
        let witnesses = [secret(4).public(), secret(5).public()];
        let descriptor =
            IdentityDescriptor::new(&alice.signed_event, &witnesses).expect("the inception folds");

        let read = IdentityDescriptor::read(&descriptor.write()).expect("reads");
        assert_eq!(read, descriptor);
        assert_eq!(read.inception(), alice.signed_event);
        assert_eq!(read.identity(), alice.event_id.into());
        assert_eq!(read.declared_kind(), DeclaredKind::Person);
        assert_eq!(read.active_key(), Some(secret(1).public()));
        assert_eq!(read.witnesses(), witnesses);
        // An invitation embeds the inception the file carried, and the field
        // table's embedded-inception rule accepts it as it stands.
        let bob = raw_rooted(7, 4);
        let ledger: LedgerId = bob.event_id.into();
        let invitation = build_membership_invitation(
            &secret(7),
            &after(&bob, ledger, 1),
            read.identity(),
            &read.active_key().expect("a raw root records a key"),
            Role::Controller,
            read.inception(),
            T0,
        )
        .expect("builds");
        let (state, violation) = fold([&bob.signed_event, &invitation.signed_event]);
        assert_eq!(violation, None);
        assert!(state.invitation(&invitation.event_id).is_some());
    }

    #[test]
    fn an_identity_rooted_descriptor_records_no_key_of_its_own() {
        let alice = raw_rooted(1, 3);
        let org = founded_by(&secret(1), &alice);
        let descriptor = IdentityDescriptor::new(&org.signed_event, &[]).expect("the org folds");
        assert_eq!(descriptor.identity(), org.event_id.into());
        assert_eq!(descriptor.declared_kind(), DeclaredKind::Organization);
        assert_eq!(descriptor.active_key(), None);
        assert_eq!(
            descriptor.root(),
            LedgerRoot::Identity {
                founder: alice.event_id.into(),
                founder_key: secret(1).public(),
            }
        );
        assert!(descriptor.witnesses().is_empty());
    }

    #[test]
    fn an_oversize_descriptor_is_rejected_on_its_length() {
        let error = IdentityDescriptor::read(&vec![0u8; MAX_IDENTITY_DESCRIPTOR_BYTES + 1])
            .expect_err("the cap is checked before the scan");
        assert_eq!(
            error,
            WireError::MessageTooLarge {
                message: "IdentityDescriptor",
                len: MAX_IDENTITY_DESCRIPTOR_BYTES + 1,
                cap: MAX_IDENTITY_DESCRIPTOR_BYTES,
            }
            .into()
        );
    }

    #[test]
    fn a_descriptor_with_too_many_or_repeated_witnesses_is_rejected() {
        let alice = raw_rooted(1, 3);
        let many: Vec<PublicKey> = (0..=MAX_WITNESSES as u8)
            .map(|seed| secret(seed.wrapping_add(20)).public())
            .collect();
        assert_eq!(
            IdentityDescriptor::new(&alice.signed_event, &many)
                .expect_err("16 witnesses is the cap")
                .code(),
            "repeated_count"
        );

        let witness = secret(4).public();
        let repeated = encode(&pb::IdentityDescriptor {
            inception: Some(pb::SignedEvent::decode(&alice.signed_event[..]).expect("decodes")),
            witnesses: vec![witness.as_bytes().to_vec(), witness.as_bytes().to_vec()],
        });
        assert_eq!(
            IdentityDescriptor::read(&repeated)
                .expect_err("a witness list holds distinct endpoints")
                .code(),
            "repeated_duplicate"
        );
    }

    #[test]
    fn a_descriptor_whose_event_is_not_an_inception_is_rejected() {
        let alice = raw_rooted(1, 3);
        let ledger: LedgerId = alice.event_id.into();
        let attestation = build_trust_attestation(
            &secret(1),
            &after(&alice, ledger, 1),
            IdentityId::from_bytes([9u8; ID_BYTES]),
            T0,
        )
        .expect("builds");

        assert_eq!(
            IdentityDescriptor::new(&attestation.signed_event, &[]),
            Err(ArtifactError::Inception(Violation {
                seq: 0,
                reason: Reason::WrongSeq {
                    expected: 0,
                    found: 1,
                },
            })),
        );
    }

    #[test]
    fn the_caps_are_the_ones_proposal_001_section_3_8_states() {
        assert_eq!(INVITATION_BUNDLE.max_bytes, 1024 * 1024);
        assert_eq!(ACCEPTANCE_FILE.max_bytes, 4096);
        assert_eq!(IDENTITY_DESCRIPTOR.max_bytes, 64 * 1024);
        // A descriptor holds one event and 16 endpoints, well inside its cap.
        assert!(MAX_EVENT_BYTES + MAX_WITNESSES * (ID_BYTES + 2) < IDENTITY_DESCRIPTOR.max_bytes);
    }
}

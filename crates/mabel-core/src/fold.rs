//! The one fold of proposal 001 section 3.6, over the one ledger type of
//! proposal 002: an event sequence in, a state and at most one violation out.
//!
//! The fold reads no local state, touches no disk and never consults the
//! verifier's clock, so verifying from nothing is the same code path as
//! verifying an event on arrival (pitfall 5).
//!
//! **State boundary.** Event `i` is checked against [`LedgerState`] folded
//! from events `0..=i-1`, the state *before* the event, and its payload is
//! applied only after every check has passed (pitfall 3). [`LedgerState::apply`]
//! runs the checks first and mutates once at the end, so a rejected event
//! leaves the state untouched.
//!
//! The order of checks at each position, from section 3.6:
//!
//! 1. [`validate::signed_event`] on the received bytes, which also validates
//!    the embedded `EventBody` and every stateless row of the field table.
//! 2. The chain rules: `seq` equals the position, and past seq 0 `ledger`
//!    equals the ledger id, `prev` equals the previous event's id and
//!    `timestamp_ms` does not fall below the previous event's.
//! 3. The payload sits at a position that accepts it: an inception at seq 0
//!    and nothing else there.
//! 4. `author_key` is the key of a `CONTROLLER` principal in the state from
//!    `0..=i-1`; seq 0 is authorized by its own root.
//! 5. The signature verifies over `sign_input` of the *received* body bytes.
//! 6. The payload's semantic rules, then the payload is applied.
//!
//! There is one ledger type and one set of rules. Declared kind is advisory
//! and gates nothing (proposal 002 section 3); what a ledger's inception
//! decided is its [`LedgerRoot`], and everything else is the principal set.
//!
//! Two seams keep the roots from spreading through the fold: the seq-0
//! payload seeds the state in [`LedgerState::seed_from_inception`], and every
//! authorization question goes through
//! [`LedgerState::signing_principal`].

use std::collections::BTreeMap;
use std::fmt;

use iroh_base::{EndpointId, PublicKey, Signature};
use mabel_proto::prost::Message;
use mabel_proto::v0::{
    Acceptance, DeclaredKind, EventBody, Role, SignedEvent, event_body::Payload, inception,
};

use crate::digest::{event_id, sign_input};
use crate::id::{EventId, IdentityId, LedgerId};
use crate::validate::{self, WireError};
use crate::{ID_BYTES, SIG_BYTES};

/// The lowercase name a declared kind carries in output and in artifacts.
///
/// Callers say "declared kind", never "kind", so nobody reads it as a checked
/// claim (proposal 002 section 3).
pub const fn declared_kind_name(kind: DeclaredKind) -> &'static str {
    match kind {
        DeclaredKind::KindUnspecified => "unspecified",
        DeclaredKind::Person => "person",
        DeclaredKind::Organization => "organization",
        DeclaredKind::Agent => "agent",
        DeclaredKind::Service => "service",
    }
}

/// The one cryptographic root a ledger's inception fixed (proposal 002
/// section 2).
///
/// This, and not the declared kind, is what proposal 001 called the ledger
/// kind. It decides where the first signing authority came from and nothing
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerRoot {
    /// A self-keyed ledger. `active_key` is a permanent `CONTROLLER`
    /// principal whose identity id is the ledger's own id, and it is not
    /// removable in this POC: delegation must never become a way to take a
    /// ledger from the identity it names.
    Raw {
        /// The key the inception recorded, authoritative for life (rotation is
        /// out of scope, decision 008).
        active_key: PublicKey,
        /// `reserve_commit(reserve_key)`; the reserve key itself is never
        /// recorded.
        reserve_commit: [u8; ID_BYTES],
    },
    /// A ledger controlled by one founding identity and holding no key of its
    /// own. The founder is an ordinary `CONTROLLER` principal and may be
    /// removed once another controller exists.
    Identity {
        /// The founding identity.
        founder: IdentityId,
        /// That identity's active key, proven by the inception embedded
        /// beside it.
        founder_key: PublicKey,
    },
}

impl LedgerRoot {
    /// Whether this root keys the ledger itself.
    pub const fn is_raw(&self) -> bool {
        matches!(self, Self::Raw { .. })
    }

    /// The name this root carries in output.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Raw { .. } => "raw",
            Self::Identity { .. } => "identity",
        }
    }
}

impl fmt::Display for LedgerRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The last event the fold accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    /// Its position, which equals its `seq`.
    pub seq: u64,
    /// Its `event_id`.
    pub event_id: EventId,
    /// Its `timestamp_ms`, the floor for the next event.
    pub timestamp_ms: u64,
}

/// An identity the ledger has recorded, and what it may do.
///
/// Every ledger holds a principal set: the root principal its inception
/// seeded, plus everyone an accepted invitation admitted. Authorization reads
/// nothing else (proposal 002 section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Principal {
    /// `CONTROLLER` may append to this ledger; `MEMBER` is recorded data only
    /// (proposal 001 section 3.4).
    pub role: Role,
    /// The active key this ledger recorded for the identity, proven by the
    /// inception that named it.
    pub active_key: PublicKey,
}

/// Who signed an event: the `author_key` and the principal it matched.
///
/// Verification output names this on every event and every trust answer, so a
/// delegate's signature is never silently attributed to the ledger's subject
/// (proposal 002 section 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigningPrincipal {
    /// The identity whose principal the `author_key` matched.
    pub identity: IdentityId,
    /// The key the event names in `author_key`.
    pub key: PublicKey,
}

impl fmt::Display for SigningPrincipal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.identity, self.key)
    }
}

/// Where a `MembershipInvitation` stands (proposal 002 section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationStatus {
    /// Issued and neither accepted nor cancelled.
    Open,
    /// A `MembershipAcceptance` consumed it.
    Accepted,
    /// A `MembershipRemoval` cancelled it.
    Cancelled,
}

impl InvitationStatus {
    /// The lowercase name this status carries in output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for InvitationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One `MembershipInvitation` and what became of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invitation {
    /// The identity invited.
    pub invitee: IdentityId,
    /// That identity's active key.
    pub invitee_key: PublicKey,
    /// The role the invitation offers.
    pub role: Role,
    /// Whether the invitation is still open.
    pub status: InvitationStatus,
}

/// One `TrustAttestation` and its revocation status.
///
/// Nothing is ever deleted (decisions/003-trust): a revoked attestation stays
/// in the map with the revoking event recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attestation {
    /// The identity the attestation names.
    pub subject: IdentityId,
    /// Who signed the attestation, which is not always the ledger's own
    /// identity (proposal 002 section 5).
    pub signing_principal: SigningPrincipal,
    /// The `event_id` of the `TrustRevocation` that revoked it, if one did.
    pub revoked_by: Option<EventId>,
}

impl Attestation {
    /// Whether a later `TrustRevocation` named this attestation.
    pub const fn is_revoked(&self) -> bool {
        self.revoked_by.is_some()
    }
}

/// The ledger's current display name and hostname (proposal 003 section 1).
///
/// Each `ProfileUpdate` replaces this whole record, so an absent field is a
/// name the last update cleared rather than one it left alone. The profile is
/// legal on every ledger, and `signing_principal` records who set it, which is
/// not always the ledger's own identity: any current `CONTROLLER` may rename
/// the ledger (proposal 002 section 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The name the ledger publishes, unset when the last update omitted it.
    pub display_name: Option<String>,
    /// The hostname the ledger claims, unset when the last update omitted it.
    /// The claim is unverified here: DNS is proposal 003 section 2 and never
    /// gates ledger validity.
    pub hostname: Option<String>,
    /// Who signed the `ProfileUpdate`.
    pub signing_principal: SigningPrincipal,
    /// The `event_id` of that `ProfileUpdate`.
    pub event: EventId,
    /// Its position in the ledger.
    pub seq: u64,
}

/// The fold of a valid event prefix (proposal 001 section 3.6).
///
/// A default `LedgerState` is the state before any event: no ledger id, no
/// root, no head.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LedgerState {
    declared_kind: Option<DeclaredKind>,
    root: Option<LedgerRoot>,
    ledger: Option<LedgerId>,
    head: Option<Head>,
    principals: BTreeMap<IdentityId, Principal>,
    invitations: BTreeMap<EventId, Invitation>,
    witnesses: Vec<EndpointId>,
    trust: BTreeMap<EventId, Attestation>,
    profile: Option<Profile>,
}

impl LedgerState {
    /// Whether no event has been applied yet.
    pub const fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// What the inception says this identity is. Advisory: it gates no rule in
    /// this file (proposal 002 section 3).
    pub const fn declared_kind(&self) -> Option<DeclaredKind> {
        self.declared_kind
    }

    /// The root the seq-0 event fixed, which is where signing authority came
    /// from.
    pub const fn root(&self) -> Option<LedgerRoot> {
        self.root
    }

    /// The identity of the root principal: the ledger itself under a raw root,
    /// the founder under an identity root.
    pub fn root_identity(&self) -> Option<IdentityId> {
        match self.root? {
            // `LedgerId` is an `IdentityId`: a self-keyed ledger is its own
            // root principal.
            LedgerRoot::Raw { .. } => self.ledger,
            LedgerRoot::Identity { founder, .. } => Some(founder),
        }
    }

    /// The ledger id, which is the `event_id` of its seq-0 event.
    pub const fn ledger(&self) -> Option<LedgerId> {
        self.ledger
    }

    /// The last event applied.
    pub const fn head(&self) -> Option<Head> {
        self.head
    }

    /// The position the next event must occupy.
    pub const fn next_seq(&self) -> u64 {
        match self.head {
            Some(head) => head.seq + 1,
            None => 0,
        }
    }

    /// Every identity this ledger records, with its role and active key.
    pub const fn principals(&self) -> &BTreeMap<IdentityId, Principal> {
        &self.principals
    }

    /// The principal recorded for `identity`, if the ledger records one.
    pub fn principal(&self, identity: &IdentityId) -> Option<&Principal> {
        self.principals.get(identity)
    }

    /// Every invitation this ledger has issued, by invitation `event_id`,
    /// whatever became of it.
    pub const fn invitations(&self) -> &BTreeMap<EventId, Invitation> {
        &self.invitations
    }

    /// The invitation with this `event_id`, open or not.
    pub fn invitation(&self, event: &EventId) -> Option<&Invitation> {
        self.invitations.get(event)
    }

    /// The current witness set, in the order the last `WitnessConfig` listed.
    pub fn witnesses(&self) -> &[EndpointId] {
        &self.witnesses
    }

    /// The profile the last `ProfileUpdate` left, or `None` on a ledger that
    /// has never carried one (proposal 003 section 1).
    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// Every attestation this ledger has issued, by attestation `event_id`,
    /// revoked ones included.
    pub const fn trust(&self) -> &BTreeMap<EventId, Attestation> {
        &self.trust
    }

    /// The attestation with this `event_id`, revoked or not.
    pub fn attestation(&self, event: &EventId) -> Option<&Attestation> {
        self.trust.get(event)
    }

    /// Whether this ledger currently attests to `subject`, which is what
    /// "does A trust B" means (proposal 001 section 3.4).
    pub fn trusts(&self, subject: IdentityId) -> bool {
        self.unrevoked(subject).is_some()
    }

    /// The principal `key` signs as, if any may sign the next event.
    ///
    /// This is the fold's only authorization question and the only place the
    /// answer is computed: a key is authorized when the principal set holds a
    /// `CONTROLLER` with that active key. The rule is the same for every
    /// ledger and every event (proposal 002 section 5).
    pub fn signing_principal(&self, key: &PublicKey) -> Option<SigningPrincipal> {
        self.principals
            .iter()
            .find(|(_, principal)| {
                principal.role == Role::Controller && &principal.active_key == key
            })
            .map(|(identity, _)| SigningPrincipal {
                identity: *identity,
                key: *key,
            })
    }

    /// Whether `key` may sign the next event.
    pub fn authorized_signer(&self, key: &PublicKey) -> bool {
        self.signing_principal(key).is_some()
    }

    /// Every distinct key that may currently append.
    pub fn controller_keys(&self) -> Vec<PublicKey> {
        let mut keys: Vec<PublicKey> = Vec::new();
        for principal in self.principals.values() {
            if principal.role == Role::Controller && !keys.contains(&principal.active_key) {
                keys.push(principal.active_key);
            }
        }
        keys
    }

    /// Checks one received `SignedEvent` against this state and, if every
    /// check passes, applies it.
    ///
    /// The event's position is [`LedgerState::next_seq`]. On `Err` the state
    /// is unchanged, so a caller may keep the valid prefix and stop.
    pub fn apply(&mut self, event: &[u8]) -> Result<(), Reason> {
        let seq = self.next_seq();

        // 1. The stateless gate, over the received bytes.
        validate::signed_event(event)?;
        // Validation passed, so both messages decode and every field the field
        // table requires is present and the right length. `signed.body` is a
        // verbatim copy of the received body bytes, never a re-encoding, so it
        // is what the id and the signature cover (pitfall 1).
        let signed = SignedEvent::decode(event).expect("a validated SignedEvent decodes");
        let body = EventBody::decode(&signed.body[..]).expect("a validated EventBody decodes");
        let payload = body.payload.as_ref().expect("payload is required");
        let id = event_id(&signed.body);

        // 2. The chain rules.
        if body.seq != seq {
            return Err(Reason::WrongSeq {
                expected: seq,
                found: body.seq,
            });
        }
        if let Some(head) = self.head {
            let ledger = identity(&body.ledger);
            let expected = self.ledger.expect("a ledger with a head has an id");
            if ledger != expected {
                return Err(Reason::WrongLedger {
                    expected,
                    found: ledger,
                });
            }
            let prev = EventId::from_slice(&body.prev).expect("prev is 32 bytes");
            if prev != head.event_id {
                return Err(Reason::BrokenPrevLink {
                    expected: head.event_id,
                    found: prev,
                });
            }
            if body.timestamp_ms < head.timestamp_ms {
                return Err(Reason::BackwardsTimestamp {
                    previous: head.timestamp_ms,
                    found: body.timestamp_ms,
                });
            }
        }

        // 3. The payload sits where it may.
        check_position(seq, payload)?;

        // 4. Authorization against the state from `0..=i-1`. Seq 0 is
        //    authorized by its own root: the field table already tied
        //    `author_key` to `RawRoot.active_key` or
        //    `IdentityRoot.founder_key`.
        let author_key = public_key(&body.author_key, "EventBody.author_key")?;
        let signer = if seq == 0 {
            None
        } else {
            Some(
                self.signing_principal(&author_key)
                    .ok_or(Reason::UnauthorizedSigner { key: author_key })?,
            )
        };

        // 5. The signature, over the received body bytes.
        verify(&author_key, &sign_input(&signed.body), &signed.signature)?;

        // 6. The semantic rules, then the payload.
        let effect = self.check_semantics(id, payload, signer)?;
        self.commit(seq, id, body.timestamp_ms, effect);
        Ok(())
    }

    /// The unrevoked attestation for `subject`, if this ledger has one.
    fn unrevoked(&self, subject: IdentityId) -> Option<(EventId, &Attestation)> {
        self.trust
            .iter()
            .find(|(_, attestation)| attestation.subject == subject && !attestation.is_revoked())
            .map(|(event, attestation)| (*event, attestation))
    }

    /// The one place a seq-0 payload becomes state: it names the root, the
    /// first principal and the advisory declared kind.
    fn seed_from_inception(
        &self,
        id: EventId,
        inception: &mabel_proto::v0::Inception,
    ) -> Result<Effect, Reason> {
        let declared_kind = DeclaredKind::try_from(inception.kind)
            .expect("a validated inception carries a defined kind");
        let (root, identity, active_key) = match inception.root.as_ref() {
            Some(inception::Root::RawRoot(raw)) => {
                let active_key = public_key(&raw.active_key, "RawRoot.active_key")?;
                let root = LedgerRoot::Raw {
                    active_key,
                    reserve_commit: raw
                        .reserve_commit
                        .as_slice()
                        .try_into()
                        .expect("reserve_commit is 32 bytes"),
                };
                // A self-keyed ledger's root principal is the ledger itself,
                // whose identity id is the id of this very event.
                (root, IdentityId::from(id), active_key)
            }
            Some(inception::Root::IdentityRoot(identity_root)) => {
                let founder = identity(&identity_root.founder);
                let founder_key =
                    public_key(&identity_root.founder_key, "IdentityRoot.founder_key")?;
                // The field table proved the embedded inception hashes to
                // `founder`, records `founder_key` and carries a raw root, so
                // the founder becomes a controller with no cross-ledger
                // lookup (proposal 002 section 8).
                (
                    LedgerRoot::Identity {
                        founder,
                        founder_key,
                    },
                    founder,
                    founder_key,
                )
            }
            None => {
                return Err(WireError::MissingOneof {
                    message: "Inception",
                    oneof: "root",
                }
                .into());
            }
        };
        let mut principals = BTreeMap::new();
        principals.insert(
            identity,
            Principal {
                role: Role::Controller,
                active_key,
            },
        );
        Ok(Effect::Seed(Box::new(Seed {
            declared_kind,
            root,
            principals,
        })))
    }

    /// The semantic rules of proposal 001 section 3.4 and proposal 002
    /// section 4, run against the state from `0..=i-1`. Returns what to apply;
    /// nothing is mutated here.
    ///
    /// `signer` is the principal the `author_key` matched, absent only at
    /// seq 0, where the root authorizes the event.
    fn check_semantics(
        &self,
        id: EventId,
        payload: &Payload,
        signer: Option<SigningPrincipal>,
    ) -> Result<Effect, Reason> {
        match payload {
            Payload::Inception(inception) => self.seed_from_inception(id, inception),
            // A witness config replaces the whole set.
            Payload::WitnessConfig(config) => {
                let mut witnesses = Vec::with_capacity(config.witnesses.len());
                for witness in &config.witnesses {
                    witnesses.push(public_key(witness, "WitnessConfig.witnesses")?);
                }
                Ok(Effect::Witnesses(witnesses))
            }
            // One unrevoked attestation per subject, so "does A trust B" has
            // one answer.
            Payload::TrustAttestation(attestation) => {
                let subject = identity(&attestation.subject);
                if Some(subject) == self.ledger {
                    return Err(Reason::SelfAttestation(subject));
                }
                if let Some((event, _)) = self.unrevoked(subject) {
                    return Err(Reason::DuplicateAttestation {
                        subject,
                        attestation: event,
                    });
                }
                Ok(Effect::Attest {
                    subject,
                    signing_principal: signer.expect("an attestation sits past seq 0"),
                })
            }
            // The target must be an unrevoked attestation earlier in this
            // ledger.
            Payload::TrustRevocation(revocation) => {
                let target = EventId::from_slice(&revocation.target).expect("target is 32 bytes");
                match self.trust.get(&target) {
                    None => Err(Reason::UnknownRevocationTarget(target)),
                    Some(attestation) => match attestation.revoked_by {
                        Some(revoked_by) => Err(Reason::AlreadyRevoked { target, revoked_by }),
                        None => Ok(Effect::Revoke(target)),
                    },
                }
            }
            Payload::MembershipInvitation(invitation) => self.check_invitation(invitation),
            Payload::MembershipAcceptance(acceptance) => self.check_acceptance(acceptance),
            Payload::MembershipRemoval(removal) => self.check_removal(identity(&removal.target)),
            // Latest wins, whole document: the update replaces the profile
            // rather than patching it, and an omitted field clears that name
            // (proposal 003 section 1). There is no rule to break here. An
            // update whose effect equals the current profile is refused by the
            // node before signing, never by the fold, which must accept
            // whatever a valid chain holds.
            Payload::ProfileUpdate(profile) => Ok(Effect::Profile {
                display_name: set_name(&profile.display_name),
                hostname: set_name(&profile.hostname),
                signing_principal: signer.expect("a profile update sits past seq 0"),
            }),
        }
    }

    /// The invitation rules of proposal 002 section 4.
    ///
    /// The field table already proved the embedded inception, the role and
    /// that `invitee` differs from the ledger id, which is what stops an
    /// ordinary principal from shadowing a raw root.
    fn check_invitation(
        &self,
        invitation: &mabel_proto::v0::MembershipInvitation,
    ) -> Result<Effect, Reason> {
        let invitee = identity(&invitation.invitee);
        let invitee_key = public_key(&invitation.invitee_key, "MembershipInvitation.invitee_key")?;
        let role = Role::try_from(invitation.role).expect("a validated invitation names a role");

        if let Some((event, _)) = self.open_invitation_of(invitee) {
            return Err(Reason::DuplicateInvitation {
                invitee,
                invitation: event,
            });
        }
        // Promotion is the one way to name an existing principal, and it must
        // carry that principal's current key: a new key for a known identity
        // would be a rotation, which is out of scope.
        if let Some(principal) = self.principals.get(&invitee)
            && principal.active_key != invitee_key
        {
            return Err(Reason::PrincipalKeyMismatch {
                identity: invitee,
                expected: principal.active_key,
                found: invitee_key,
            });
        }
        Ok(Effect::Invite(Invitation {
            invitee,
            invitee_key,
            role,
            status: InvitationStatus::Open,
        }))
    }

    /// The admission rules of proposal 002 section 4.
    ///
    /// The admitted principal is read from the invitation the acceptance
    /// names, never from the blob, which only has to match it. The blob's
    /// signature was already checked under its own `invitee_key`, so the four
    /// equality rules below are what stop a valid acceptance from being
    /// transplanted onto another ledger, invitation, identity or key.
    fn check_acceptance(
        &self,
        acceptance: &mabel_proto::v0::MembershipAcceptance,
    ) -> Result<Effect, Reason> {
        let blob =
            Acceptance::decode(&acceptance.acceptance[..]).expect("a validated Acceptance decodes");
        let ledger = self.ledger.ok_or(WireError::NonInceptionAtSeqZero)?;
        let named_ledger = identity(&blob.ledger);
        if named_ledger != ledger {
            return Err(Reason::AcceptanceForAnotherLedger {
                named: named_ledger,
                expected: ledger,
            });
        }
        let event = EventId::from_slice(&blob.invitation_event).expect("the event id is 32 bytes");
        let invitation = self
            .invitations
            .get(&event)
            .ok_or(Reason::UnknownInvitation(event))?;
        if invitation.status != InvitationStatus::Open {
            return Err(Reason::InvitationNotOpen {
                invitation: event,
                status: invitation.status,
            });
        }
        let named_invitee = identity(&blob.invitee);
        if named_invitee != invitation.invitee {
            return Err(Reason::AcceptanceInviteeMismatch {
                named: named_invitee,
                invited: invitation.invitee,
            });
        }
        let named_key = public_key(&blob.invitee_key, "Acceptance.invitee_key")?;
        if named_key != invitation.invitee_key {
            return Err(Reason::AcceptanceInviteeKeyMismatch {
                named: named_key,
                invited: invitation.invitee_key,
            });
        }
        // Duplicate keys are rejected where the principal is added. A new
        // identity presenting a key a principal already holds would be a
        // second name for one signer; promotion of the same identity is the
        // exception and keeps its key.
        if let Some((held_by, _)) = self.principals.iter().find(|(id, principal)| {
            principal.active_key == invitation.invitee_key && **id != invitation.invitee
        }) {
            return Err(Reason::DuplicatePrincipalKey {
                key: invitation.invitee_key,
                held_by: *held_by,
            });
        }
        // An acceptance that lowers a CONTROLLER to MEMBER takes a controller
        // away exactly as a removal does, so it answers to the same two rules
        // (proposal 002, clarifications of 2026-08-25).
        if self
            .principals
            .get(&invitation.invitee)
            .is_some_and(|held| held.role == Role::Controller)
            && invitation.role != Role::Controller
        {
            self.check_demotion(invitation.invitee)?;
        }
        Ok(Effect::Admit {
            invitation: event,
            invitee: invitation.invitee,
            principal: Principal {
                role: invitation.role,
                active_key: invitation.invitee_key,
            },
        })
    }

    /// The removal rules of proposal 002 section 4: cancel the target's open
    /// invitation and remove its membership, whichever exist.
    fn check_removal(&self, target: IdentityId) -> Result<Effect, Reason> {
        // The raw root is never removable: a controller able to remove it
        // could take the ledger from the identity it names.
        if self.is_raw_root(target) {
            return Err(Reason::RootNotRemovable(target));
        }
        let invitation = self.open_invitation_of(target).map(|(event, _)| event);
        let principal = self.principals.get(&target).copied();
        if invitation.is_none() && principal.is_none() {
            return Err(Reason::UnknownRemovalTarget(target));
        }
        // A removal must leave at least one controller, counted over distinct
        // keys. On a raw-rooted ledger the root counts toward that minimum, so
        // this can only bite an identity-rooted ledger.
        if principal.is_some_and(|principal| principal.role == Role::Controller)
            && self.controller_keys_without(target).is_empty()
        {
            return Err(Reason::LastController(target));
        }
        Ok(Effect::Remove {
            target,
            invitation,
            was_principal: principal.is_some(),
        })
    }

    /// The rules a demotion shares with a removal (proposal 002,
    /// clarifications of 2026-08-25).
    ///
    /// Lowering a `CONTROLLER` to `MEMBER` withdraws the same signing
    /// authority a removal withdraws, so the raw root is never demoted and a
    /// demotion must leave at least one controller behind. Without this an
    /// identity-rooted ledger's sole founder could self-invite as `MEMBER` and
    /// leave the ledger with nobody who may append.
    fn check_demotion(&self, target: IdentityId) -> Result<(), Reason> {
        if self.is_raw_root(target) {
            return Err(Reason::RootNotDemotable(target));
        }
        if self.controller_keys_without(target).is_empty() {
            return Err(Reason::DemotesLastController(target));
        }
        Ok(())
    }

    /// Whether `target` is the root principal of a raw-rooted ledger, which is
    /// the ledger's own identity.
    fn is_raw_root(&self, target: IdentityId) -> bool {
        self.root.is_some_and(|root| root.is_raw()) && Some(target) == self.root_identity()
    }

    /// Every distinct controller key the ledger would still hold if `target`
    /// stopped being a controller.
    fn controller_keys_without(&self, target: IdentityId) -> Vec<PublicKey> {
        let mut remaining: Vec<PublicKey> = Vec::new();
        for (id, principal) in &self.principals {
            if *id != target
                && principal.role == Role::Controller
                && !remaining.contains(&principal.active_key)
            {
                remaining.push(principal.active_key);
            }
        }
        remaining
    }

    /// The open invitation naming `invitee`, if the ledger holds one.
    fn open_invitation_of(&self, invitee: IdentityId) -> Option<(EventId, &Invitation)> {
        self.invitations
            .iter()
            .find(|(_, invitation)| {
                invitation.invitee == invitee && invitation.status == InvitationStatus::Open
            })
            .map(|(event, invitation)| (*event, invitation))
    }

    /// Applies a checked event. Every mutation the fold makes happens here,
    /// after every check has passed (pitfall 3).
    fn commit(&mut self, seq: u64, id: EventId, timestamp_ms: u64, effect: Effect) {
        match effect {
            Effect::Seed(seed) => {
                let Seed {
                    declared_kind,
                    root,
                    principals,
                } = *seed;
                self.declared_kind = Some(declared_kind);
                self.root = Some(root);
                self.ledger = Some(id.into());
                self.principals = principals;
            }
            Effect::Witnesses(witnesses) => self.witnesses = witnesses,
            Effect::Attest {
                subject,
                signing_principal,
            } => {
                self.trust.insert(
                    id,
                    Attestation {
                        subject,
                        signing_principal,
                        revoked_by: None,
                    },
                );
            }
            Effect::Revoke(target) => {
                let attestation = self
                    .trust
                    .get_mut(&target)
                    .expect("the revocation check found the target");
                attestation.revoked_by = Some(id);
            }
            Effect::Invite(invitation) => {
                self.invitations.insert(id, invitation);
            }
            Effect::Admit {
                invitation,
                invitee,
                principal,
            } => {
                self.invitations
                    .get_mut(&invitation)
                    .expect("the acceptance check found the invitation")
                    .status = InvitationStatus::Accepted;
                self.principals.insert(invitee, principal);
            }
            Effect::Remove {
                target,
                invitation,
                was_principal,
            } => {
                if let Some(invitation) = invitation {
                    self.invitations
                        .get_mut(&invitation)
                        .expect("the removal check found the invitation")
                        .status = InvitationStatus::Cancelled;
                }
                if was_principal {
                    self.principals.remove(&target);
                }
            }
            Effect::Profile {
                display_name,
                hostname,
                signing_principal,
            } => {
                self.profile = Some(Profile {
                    display_name,
                    hostname,
                    signing_principal,
                    event: id,
                    seq,
                });
            }
        }
        self.head = Some(Head {
            seq,
            event_id: id,
            timestamp_ms,
        });
    }
}

/// The position rule: seq 0 holds an inception and nothing else does.
///
/// A defensive guard. The field table pins an inception to a body with no
/// `seq`, and the chain rule pins that body to position 0, so no received
/// event reaches this.
fn check_position(seq: u64, payload: &Payload) -> Result<(), Reason> {
    let is_inception = matches!(payload, Payload::Inception(_));
    if is_inception == (seq == 0) {
        return Ok(());
    }
    Err(Reason::PayloadNotAllowed {
        seq,
        payload: payload_name(payload),
    })
}

/// What a checked event changes, produced only once every check has passed.
enum Effect {
    Seed(Box<Seed>),
    Witnesses(Vec<EndpointId>),
    Attest {
        subject: IdentityId,
        signing_principal: SigningPrincipal,
    },
    Revoke(EventId),
    Invite(Invitation),
    Admit {
        invitation: EventId,
        invitee: IdentityId,
        principal: Principal,
    },
    Remove {
        target: IdentityId,
        invitation: Option<EventId>,
        was_principal: bool,
    },
    Profile {
        display_name: Option<String>,
        hostname: Option<String>,
        signing_principal: SigningPrincipal,
    },
}

/// What a seq-0 payload seeds.
struct Seed {
    declared_kind: DeclaredKind,
    root: LedgerRoot,
    principals: BTreeMap<IdentityId, Principal>,
}

/// Why an event was rejected.
///
/// [`Reason::Wire`] carries the stateless rejection of `validate`; every other
/// variant needs the folded state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Reason {
    /// The bytes failed the wire-format validator or the stateless field
    /// table.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// `seq` did not equal the event's position, which is what rejects a
    /// duplicated, reordered or missing event.
    #[error("the event at position {expected} declares seq {found}")]
    WrongSeq {
        /// The position in the sequence.
        expected: u64,
        /// The `seq` the event declares.
        found: u64,
    },
    /// `ledger` named another ledger.
    #[error("EventBody.ledger names {found}, not this ledger {expected}")]
    WrongLedger {
        /// This ledger's id.
        expected: LedgerId,
        /// The id the event names.
        found: LedgerId,
    },
    /// `prev` did not name the previous event.
    #[error("EventBody.prev names {found}, not the previous event {expected}")]
    BrokenPrevLink {
        /// The previous event's id.
        expected: EventId,
        /// The id the event names.
        found: EventId,
    },
    /// `timestamp_ms` fell below the previous event's.
    #[error("timestamp_ms {found} is below the previous event's {previous}")]
    BackwardsTimestamp {
        /// The previous event's `timestamp_ms`.
        previous: u64,
        /// The `timestamp_ms` the event carries.
        found: u64,
    },
    /// The payload does not belong at this position: only an inception sits at
    /// seq 0, and only at seq 0.
    #[error("a {payload} payload does not belong at seq {seq}")]
    PayloadNotAllowed {
        /// The position the event claimed.
        seq: u64,
        /// The payload's message name.
        payload: &'static str,
    },
    /// A 32-byte field that must be an ed25519 public key was not a curve
    /// point.
    #[error("{field} is not a valid ed25519 public key")]
    InvalidPublicKey {
        /// The field, qualified by its message type.
        field: &'static str,
    },
    /// `author_key` is not the key of a `CONTROLLER` principal in the state
    /// before this event.
    #[error("author_key {key} may not sign this event")]
    UnauthorizedSigner {
        /// The key the event names.
        key: PublicKey,
    },
    /// The signature did not verify under `author_key`.
    #[error("SignedEvent.signature does not verify under author_key")]
    BadSignature,
    /// An unrevoked attestation for the same subject already exists.
    #[error("{subject} already has an unrevoked attestation, {attestation}")]
    DuplicateAttestation {
        /// The subject attested twice.
        subject: IdentityId,
        /// The `event_id` of the attestation still standing.
        attestation: EventId,
    },
    /// A `TrustAttestation` named this ledger's own identity.
    #[error("TrustAttestation.subject is this ledger's own id {0}")]
    SelfAttestation(IdentityId),
    /// `TrustRevocation.target` named no attestation earlier in this ledger.
    #[error("TrustRevocation.target {0} names no attestation earlier in this ledger")]
    UnknownRevocationTarget(EventId),
    /// `TrustRevocation.target` named an attestation already revoked.
    #[error("attestation {target} was already revoked by {revoked_by}")]
    AlreadyRevoked {
        /// The attestation the revocation names.
        target: EventId,
        /// The `event_id` of the revocation that got there first.
        revoked_by: EventId,
    },
    /// The invitee already has an open invitation on this ledger.
    #[error("{invitee} already has an open invitation, {invitation}")]
    DuplicateInvitation {
        /// The identity invited twice.
        invitee: IdentityId,
        /// The `event_id` of the invitation still open.
        invitation: EventId,
    },
    /// An invitation naming an existing principal carried a key other than
    /// that principal's, so it is a rotation rather than a promotion.
    #[error("{identity} is a principal with key {expected}, not {found}")]
    PrincipalKeyMismatch {
        /// The identity the invitation names.
        identity: IdentityId,
        /// The key the principal set records.
        expected: PublicKey,
        /// The key the invitation carries.
        found: PublicKey,
    },
    /// The acceptance blob named a ledger other than this one.
    #[error("Acceptance.ledger names {named}, not this ledger {expected}")]
    AcceptanceForAnotherLedger {
        /// The ledger the blob names.
        named: IdentityId,
        /// This ledger's id.
        expected: LedgerId,
    },
    /// `Acceptance.invitation_event` named no invitation in this ledger.
    #[error("Acceptance.invitation_event {0} names no invitation in this ledger")]
    UnknownInvitation(EventId),
    /// The invitation the acceptance names was already accepted or cancelled,
    /// which is what makes an invitation single use on this branch.
    #[error("invitation {invitation} is {status}, not open")]
    InvitationNotOpen {
        /// The invitation the acceptance names.
        invitation: EventId,
        /// What became of it.
        status: InvitationStatus,
    },
    /// The acceptance blob named an identity other than the invitee.
    #[error("Acceptance.invitee names {named}, but the invitation invited {invited}")]
    AcceptanceInviteeMismatch {
        /// The identity the blob names.
        named: IdentityId,
        /// The identity the invitation invited.
        invited: IdentityId,
    },
    /// The acceptance blob was signed by a key other than the invitee's.
    #[error("Acceptance.invitee_key is {named}, but the invitation names {invited}")]
    AcceptanceInviteeKeyMismatch {
        /// The key the blob names, which signed it.
        named: PublicKey,
        /// The key the invitation records.
        invited: PublicKey,
    },
    /// Admitting this principal would give one key two identities.
    #[error("key {key} is already held by principal {held_by}")]
    DuplicatePrincipalKey {
        /// The key the invitation carries.
        key: PublicKey,
        /// The principal that already holds it.
        held_by: IdentityId,
    },
    /// A removal named the raw root, which no controller may remove.
    #[error("{0} is this ledger's raw root and is not removable")]
    RootNotRemovable(IdentityId),
    /// A removal named an identity this ledger neither records nor has
    /// invited.
    #[error("MembershipRemoval.target {0} is neither a principal nor an open invitee")]
    UnknownRemovalTarget(IdentityId),
    /// A removal would leave the ledger with no controller.
    #[error("removing {0} would leave this ledger with no controller")]
    LastController(IdentityId),
    /// An acceptance named the raw root at a role below `CONTROLLER`, which no
    /// controller may do.
    #[error("{0} is this ledger's raw root and is not demotable")]
    RootNotDemotable(IdentityId),
    /// An acceptance lowering this controller to `MEMBER` would leave the
    /// ledger with no controller.
    #[error("demoting {0} would leave this ledger with no controller")]
    DemotesLastController(IdentityId),
}

impl Reason {
    /// A stable snake-case name for this rejection class, which the CLI maps
    /// to its exit code and another implementation can assert without
    /// matching English prose.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Wire(error) => error.code(),
            Self::WrongSeq { .. } => "wrong_seq",
            Self::WrongLedger { .. } => "wrong_ledger",
            Self::BrokenPrevLink { .. } => "broken_prev_link",
            Self::BackwardsTimestamp { .. } => "backwards_timestamp",
            Self::PayloadNotAllowed { .. } => "payload_not_allowed",
            Self::InvalidPublicKey { .. } => "invalid_public_key",
            Self::UnauthorizedSigner { .. } => "unauthorized_signer",
            Self::BadSignature => "bad_signature",
            Self::DuplicateAttestation { .. } => "duplicate_attestation",
            Self::SelfAttestation(_) => "self_attestation",
            Self::UnknownRevocationTarget(_) => "unknown_revocation_target",
            Self::AlreadyRevoked { .. } => "already_revoked",
            Self::DuplicateInvitation { .. } => "duplicate_invitation",
            Self::PrincipalKeyMismatch { .. } => "principal_key_mismatch",
            Self::AcceptanceForAnotherLedger { .. } => "acceptance_for_another_ledger",
            Self::UnknownInvitation(_) => "unknown_invitation",
            Self::InvitationNotOpen { .. } => "invitation_not_open",
            Self::AcceptanceInviteeMismatch { .. } => "acceptance_invitee_mismatch",
            Self::AcceptanceInviteeKeyMismatch { .. } => "acceptance_invitee_key_mismatch",
            Self::DuplicatePrincipalKey { .. } => "duplicate_principal_key",
            Self::RootNotRemovable(_) => "root_not_removable",
            Self::UnknownRemovalTarget(_) => "unknown_removal_target",
            Self::LastController(_) => "last_controller",
            Self::RootNotDemotable(_) => "root_not_demotable",
            Self::DemotesLastController(_) => "demotes_last_controller",
        }
    }
}

/// The first event a fold rejected, and why.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("seq {seq}: {reason}")]
pub struct Violation {
    /// The position of the failing event, which is the `seq` it should have
    /// occupied.
    pub seq: u64,
    /// Why it failed.
    pub reason: Reason,
}

impl Violation {
    /// The stable code of the reason.
    pub const fn code(&self) -> &'static str {
        self.reason.code()
    }
}

/// Folds a sequence of received `SignedEvent` byte strings into a state.
///
/// The state is the fold of the valid prefix and the violation, if any, names
/// the first event that failed: a ledger valid to seq N with a bad event at M
/// returns the state at N and a violation at M. Partial validity is reported,
/// never accepted (proposal 001 section 3.6).
pub fn fold<I>(events: I) -> (LedgerState, Option<Violation>)
where
    I: IntoIterator,
    I::Item: AsRef<[u8]>,
{
    let mut state = LedgerState::default();
    for event in events {
        let seq = state.next_seq();
        if let Err(reason) = state.apply(event.as_ref()) {
            return (state, Some(Violation { seq, reason }));
        }
    }
    (state, None)
}

/// The message name of a payload variant, for an error a person reads.
const fn payload_name(payload: &Payload) -> &'static str {
    match payload {
        Payload::Inception(_) => "Inception",
        Payload::WitnessConfig(_) => "WitnessConfig",
        Payload::TrustAttestation(_) => "TrustAttestation",
        Payload::TrustRevocation(_) => "TrustRevocation",
        Payload::MembershipInvitation(_) => "MembershipInvitation",
        Payload::MembershipAcceptance(_) => "MembershipAcceptance",
        Payload::MembershipRemoval(_) => "MembershipRemoval",
        Payload::ProfileUpdate(_) => "ProfileUpdate",
    }
}

/// Reads a 32-byte identity id the field table already length-checked.
/// A profile name as the fold reports it: the canonical encoding omits an
/// unset string, so a decoded empty string is an absent field, which means
/// the update cleared that name (proposal 003 section 1).
fn set_name(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn identity(bytes: &[u8]) -> IdentityId {
    IdentityId::from_slice(bytes).expect("an identity id is 32 bytes")
}

/// Reads a 32-byte field the field table length-checked and requires it to be
/// an ed25519 curve point, which only the key types need.
fn public_key(bytes: &[u8], field: &'static str) -> Result<PublicKey, Reason> {
    let bytes: [u8; ID_BYTES] = bytes.try_into().expect("a key field is 32 bytes");
    PublicKey::from_bytes(&bytes).map_err(|_| Reason::InvalidPublicKey { field })
}

fn verify(key: &PublicKey, input: &[u8], signature: &[u8]) -> Result<(), Reason> {
    let signature: [u8; SIG_BYTES] = signature
        .try_into()
        .expect("a validated signature is 64 bytes");
    key.verify(input, &Signature::from_bytes(&signature))
        .map_err(|_| Reason::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::encode;
    use crate::sign::{
        BuiltEvent, DetachedAcceptance, Position, Root, build_acceptance, build_inception,
        build_membership_acceptance, build_membership_invitation, build_membership_removal,
        build_profile_update, build_trust_attestation, build_trust_revocation,
        build_witness_config,
    };
    use crate::{MAX_TIMESTAMP_MS, NONCE_BYTES};
    use iroh_base::SecretKey;
    use mabel_proto::v0::TrustRevocation;

    const T0: u64 = 1_700_000_000_000;
    const STEP: u64 = 60_000;

    fn secret(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    /// A raw-rooted inception signed by `secret(seed)`, so `seed` and `nonce`
    /// together pick an identity.
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

    /// Alice, the raw-rooted ledger most tests build on.
    fn alice() -> BuiltEvent {
        raw_rooted(1, 3)
    }

    /// Bob, a second identity to invite.
    fn bob() -> BuiltEvent {
        raw_rooted(7, 4)
    }

    /// Carol, a third identity for the cases that need two invitees.
    fn carol() -> BuiltEvent {
        raw_rooted(9, 5)
    }

    /// An identity-rooted ledger founded by `founder`, which holds no key of
    /// its own.
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

    /// A ledger under construction: the events so far and where the next one
    /// goes.
    struct Chain {
        ledger: LedgerId,
        events: Vec<Vec<u8>>,
        prev: EventId,
        seq: u64,
        timestamp_ms: u64,
    }

    impl Chain {
        fn start(inception: &BuiltEvent) -> Self {
            Self {
                ledger: inception.event_id.into(),
                events: vec![inception.signed_event.clone()],
                prev: inception.event_id,
                seq: 1,
                timestamp_ms: T0,
            }
        }

        /// Where the next event goes.
        fn at(&self) -> Position {
            Position {
                ledger: self.ledger,
                seq: self.seq,
                prev: self.prev,
                prev_timestamp_ms: self.timestamp_ms,
            }
        }

        /// The timestamp the next event should carry.
        fn now(&self) -> u64 {
            T0 + self.seq * STEP
        }

        fn push(&mut self, built: BuiltEvent) -> EventId {
            self.events.push(built.signed_event);
            self.prev = built.event_id;
            self.seq += 1;
            self.timestamp_ms = T0 + (self.seq - 1) * STEP;
            built.event_id
        }

        fn fold(&self) -> (LedgerState, Option<Violation>) {
            fold(&self.events)
        }

        /// The state, asserting the whole chain is valid.
        fn state(&self) -> LedgerState {
            let (state, violation) = self.fold();
            assert_eq!(violation, None, "the chain is valid");
            state
        }

        /// The violation the chain reports.
        fn violation(&self) -> Violation {
            self.fold().1.expect("the fold reports a violation")
        }
    }

    /// Invites `invitee` at the chain's next position, signed by `signer`.
    fn invite(
        chain: &Chain,
        signer: &SecretKey,
        invitee: &BuiltEvent,
        invitee_key: &SecretKey,
        role: Role,
    ) -> BuiltEvent {
        build_membership_invitation(
            signer,
            &chain.at(),
            invitee.event_id.into(),
            &invitee_key.public(),
            role,
            &invitee.signed_event,
            chain.now(),
        )
        .expect("builds")
    }

    /// Admits the invitee of `invitation`, with the acceptance they signed.
    fn admit(
        chain: &Chain,
        signer: &SecretKey,
        invitee_key: &SecretKey,
        invitee: IdentityId,
        invitation: EventId,
    ) -> BuiltEvent {
        let accepted = build_acceptance(invitee_key, chain.ledger, invitation, invitee);
        build_membership_acceptance(signer, &chain.at(), &accepted, chain.now()).expect("builds")
    }

    fn remove(chain: &Chain, signer: &SecretKey, target: IdentityId) -> BuiltEvent {
        build_membership_removal(signer, &chain.at(), target, chain.now()).expect("builds")
    }

    fn attest(chain: &Chain, signer: &SecretKey, subject: IdentityId) -> BuiltEvent {
        build_trust_attestation(signer, &chain.at(), subject, chain.now()).expect("builds")
    }

    /// Replaces the chain's profile, signed by `signer`.
    fn set_profile(
        chain: &Chain,
        signer: &SecretKey,
        display_name: Option<&str>,
        hostname: Option<&str>,
    ) -> BuiltEvent {
        build_profile_update(signer, &chain.at(), display_name, hostname, chain.now())
            .expect("builds")
    }

    fn subject(seed: u8) -> IdentityId {
        IdentityId::from_bytes([seed; ID_BYTES])
    }

    /// Signs an arbitrary body, for the cases no builder will produce.
    fn seal(signer: &SecretKey, body: &EventBody) -> Vec<u8> {
        let body = encode(body);
        let signature = signer.sign(&sign_input(&body));
        encode(&SignedEvent {
            body,
            signature: signature.to_bytes().to_vec(),
        })
    }

    #[test]
    fn an_empty_sequence_folds_to_the_empty_state() {
        let (state, violation) = fold(Vec::<Vec<u8>>::new());
        assert!(state.is_empty());
        assert_eq!(state.next_seq(), 0);
        assert_eq!(state.declared_kind(), None);
        assert_eq!(state.root(), None);
        assert_eq!(state.ledger(), None);
        assert_eq!(violation, None);
    }

    #[test]
    fn a_raw_rooted_ledger_folds_inception_witnesses_attestation_and_revocation() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(
            build_witness_config(
                &secret(1),
                &chain.at(),
                &[secret(4).public(), secret(5).public()],
                chain.now(),
            )
            .expect("builds"),
        );
        let attestation = chain.push(attest(&chain, &secret(1), subject(9)));
        let revocation = chain.push(
            build_trust_revocation(&secret(1), &chain.at(), attestation, chain.now())
                .expect("builds"),
        );
        // The same subject may be attested again once the first attestation is
        // revoked.
        let again = chain.push(attest(&chain, &secret(1), subject(9)));

        let state = chain.state();
        assert_eq!(state.declared_kind(), Some(DeclaredKind::Person));
        assert_eq!(state.ledger(), Some(root.event_id.into()));
        let head = state.head().expect("a folded ledger has a head");
        assert_eq!(head.seq, 4);
        assert_eq!(head.event_id, again);
        assert_eq!(head.timestamp_ms, T0 + 4 * STEP);
        assert_eq!(state.next_seq(), 5);

        assert_eq!(
            state.root(),
            Some(LedgerRoot::Raw {
                active_key: secret(1).public(),
                reserve_commit: crate::digest::reserve_commit(&secret(2).public()),
            })
        );
        assert!(state.root().expect("a root").is_raw());
        assert_eq!(state.root_identity(), Some(root.event_id.into()));

        // A raw-rooted ledger starts with one principal, itself, and that is
        // what authorizes the signer.
        assert_eq!(state.principals().len(), 1);
        let principal = state
            .principal(&IdentityId::from(root.event_id))
            .expect("the root is its own principal");
        assert_eq!(principal.role, Role::Controller);
        assert_eq!(principal.active_key, secret(1).public());
        assert!(state.authorized_signer(&secret(1).public()));
        assert!(!state.authorized_signer(&secret(6).public()));
        assert_eq!(state.controller_keys(), [secret(1).public()]);

        assert_eq!(state.witnesses(), [secret(4).public(), secret(5).public()]);

        assert_eq!(state.trust().len(), 2);
        let revoked = state.attestation(&attestation).expect("recorded");
        assert_eq!(revoked.subject, subject(9));
        assert_eq!(revoked.revoked_by, Some(revocation));
        assert!(revoked.is_revoked());
        let live = state.attestation(&again).expect("recorded");
        assert_eq!(live.revoked_by, None);
        // Every attestation names who signed it.
        assert_eq!(
            live.signing_principal,
            SigningPrincipal {
                identity: root.event_id.into(),
                key: secret(1).public(),
            }
        );
        assert!(state.trusts(subject(9)));
        assert!(!state.trusts(subject(8)));
        assert!(state.invitations().is_empty());
    }

    #[test]
    fn a_witness_config_replaces_the_whole_set() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(
            build_witness_config(
                &secret(1),
                &chain.at(),
                &[secret(4).public(), secret(5).public()],
                chain.now(),
            )
            .expect("builds"),
        );
        chain.push(
            build_witness_config(&secret(1), &chain.at(), &[secret(6).public()], chain.now())
                .expect("builds"),
        );
        assert_eq!(chain.state().witnesses(), [secret(6).public()]);
    }

    #[test]
    fn position_zero_requires_an_inception() {
        let root = alice();
        let chain = Chain::start(&root);
        let attestation = attest(&chain, &secret(1), subject(9));
        let (_, violation) = fold(vec![attestation.signed_event]);
        assert_eq!(
            violation,
            Some(Violation {
                seq: 0,
                reason: Reason::WrongSeq {
                    expected: 0,
                    found: 1,
                },
            })
        );
    }

    #[test]
    fn a_broken_prev_link_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let mut at = chain.at();
        at.prev = EventId::from_bytes([0xaa; ID_BYTES]);
        chain.push(build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds"));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::BrokenPrevLink {
                    expected: root.event_id,
                    found: EventId::from_bytes([0xaa; ID_BYTES]),
                },
            }
        );
    }

    #[test]
    fn the_same_event_twice_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let attestation = attest(&chain, &secret(1), subject(9));
        chain.events.push(attestation.signed_event.clone());
        chain.events.push(attestation.signed_event);
        let found = chain.violation();
        assert_eq!(
            found,
            Violation {
                seq: 2,
                reason: Reason::WrongSeq {
                    expected: 2,
                    found: 1,
                },
            }
        );
        assert_eq!(found.code(), "wrong_seq");
    }

    #[test]
    fn a_gap_in_the_sequence_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let mut at = chain.at();
        at.seq = 2;
        chain.push(build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds"));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::WrongSeq {
                    expected: 1,
                    found: 2,
                },
            }
        );
    }

    #[test]
    fn a_wrong_ledger_id_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let mut at = chain.at();
        at.ledger = subject(0xbb);
        chain.push(build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds"));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::WrongLedger {
                    expected: root.event_id.into(),
                    found: subject(0xbb),
                },
            }
        );
    }

    #[test]
    fn a_backwards_timestamp_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        // The builder clamps to `prev_timestamp_ms`, so the position has to
        // understate the head's timestamp for the event to go backwards.
        let mut at = chain.at();
        at.prev_timestamp_ms = 0;
        chain.push(build_trust_attestation(&secret(1), &at, subject(9), T0 - 1).expect("builds"));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::BackwardsTimestamp {
                    previous: T0,
                    found: T0 - 1,
                },
            }
        );
    }

    #[test]
    fn a_timestamp_past_the_year_2100_bound_is_rejected() {
        let root = alice();
        // No builder emits this, so the body is hand-built and signed.
        let body = EventBody {
            version: 0,
            ledger: root.event_id.to_vec(),
            seq: 1,
            prev: root.event_id.to_vec(),
            timestamp_ms: MAX_TIMESTAMP_MS + 1,
            author_key: secret(1).public().as_bytes().to_vec(),
            payload: Some(Payload::TrustRevocation(TrustRevocation {
                target: vec![9u8; ID_BYTES],
            })),
        };
        let event = seal(&secret(1), &body);
        let (state, violation) = fold(vec![root.signed_event.clone(), event]);
        let violation = violation.expect("the fold reports a violation");
        assert_eq!(violation.seq, 1);
        assert_eq!(violation.code(), "value_out_of_range");
        assert_eq!(state.head().expect("head").seq, 0);
    }

    #[test]
    fn an_unauthorized_signer_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        // secret(6) is not this ledger's active key.
        chain.push(attest(&chain, &secret(6), subject(9)));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::UnauthorizedSigner {
                    key: secret(6).public(),
                },
            }
        );
    }

    #[test]
    fn a_signature_over_other_bytes_is_rejected() {
        let root = alice();
        let chain = Chain::start(&root);
        let attestation = attest(&chain, &secret(1), subject(9));
        // The body of one event carried with the signature of another: the
        // author is authorized, the signature is not over these bytes.
        let other = attest(&chain, &secret(1), subject(10));
        let mixed = encode(&SignedEvent {
            body: attestation.body,
            signature: SignedEvent::decode(&other.signed_event[..])
                .expect("decodes")
                .signature,
        });
        let (_, violation) = fold(vec![root.signed_event, mixed]);
        assert_eq!(
            violation,
            Some(Violation {
                seq: 1,
                reason: Reason::BadSignature,
            })
        );
    }

    #[test]
    fn a_self_attestation_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(attest(&chain, &secret(1), root.event_id.into()));
        // The field table catches this statelessly, comparing `subject` with
        // the `ledger` the event names; the chain rule ties that to the real
        // ledger id.
        assert_eq!(chain.violation().code(), "fields_must_differ");
    }

    #[test]
    fn a_witness_that_is_not_a_public_key_is_rejected() {
        let root = alice();
        let body = EventBody {
            version: 0,
            ledger: root.event_id.to_vec(),
            seq: 1,
            prev: root.event_id.to_vec(),
            timestamp_ms: T0,
            author_key: secret(1).public().as_bytes().to_vec(),
            // 32 bytes of 0x02 decompress to no ed25519 point.
            payload: Some(Payload::WitnessConfig(mabel_proto::v0::WitnessConfig {
                witnesses: vec![vec![0x02; ID_BYTES]],
            })),
        };
        let event = seal(&secret(1), &body);
        let (_, violation) = fold(vec![root.signed_event, event]);
        assert_eq!(
            violation,
            Some(Violation {
                seq: 1,
                reason: Reason::InvalidPublicKey {
                    field: "WitnessConfig.witnesses",
                },
            })
        );
    }

    #[test]
    fn an_attestation_duplicating_an_unrevoked_subject_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let first = chain.push(attest(&chain, &secret(1), subject(9)));
        chain.push(attest(&chain, &secret(1), subject(9)));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 2,
                reason: Reason::DuplicateAttestation {
                    subject: subject(9),
                    attestation: first,
                },
            }
        );
    }

    #[test]
    fn revoking_an_unknown_attestation_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let unknown = EventId::from_bytes([0xcd; ID_BYTES]);
        chain.push(
            build_trust_revocation(&secret(1), &chain.at(), unknown, chain.now()).expect("builds"),
        );
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::UnknownRevocationTarget(unknown),
            }
        );
    }

    #[test]
    fn revoking_an_already_revoked_attestation_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        let attestation = chain.push(attest(&chain, &secret(1), subject(9)));
        let revocation = chain.push(
            build_trust_revocation(&secret(1), &chain.at(), attestation, chain.now())
                .expect("builds"),
        );
        chain.push(
            build_trust_revocation(&secret(1), &chain.at(), attestation, chain.now())
                .expect("builds"),
        );
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 3,
                reason: Reason::AlreadyRevoked {
                    target: attestation,
                    revoked_by: revocation,
                },
            }
        );
    }

    #[test]
    fn a_ledger_valid_to_n_folds_to_n_and_reports_the_failure_at_m() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(
            build_witness_config(&secret(1), &chain.at(), &[secret(4).public()], chain.now())
                .expect("builds"),
        );
        let attestation = chain.push(attest(&chain, &secret(1), subject(9)));
        // Seq 3 is signed by a key this ledger never authorized.
        chain.push(attest(&chain, &secret(6), subject(10)));
        // Seq 4 would be valid on its own; the fold never reaches it.
        chain.push(attest(&chain, &secret(1), subject(11)));

        let (state, violation) = chain.fold();
        let violation = violation.expect("the fold reports a violation");
        assert_eq!(violation.seq, 3);
        assert_eq!(violation.code(), "unauthorized_signer");

        let head = state.head().expect("the valid prefix has a head");
        assert_eq!(head.seq, 2);
        assert_eq!(head.event_id, attestation);
        assert_eq!(state.witnesses(), [secret(4).public()]);
        assert_eq!(state.trust().len(), 1);
        assert!(state.trusts(subject(9)));
        assert!(!state.trusts(subject(10)));
    }

    #[test]
    fn a_rejected_event_leaves_the_state_untouched() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(attest(&chain, &secret(1), subject(9)));
        let duplicate = attest(&chain, &secret(1), subject(9));

        let mut state = chain.state();
        let before = state.clone();
        assert!(state.apply(&duplicate.signed_event).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn an_identity_root_seeds_the_founder_as_a_controller() {
        let founder = alice();
        let organization = founded_by(&secret(1), &founder);

        let (state, violation) = fold(vec![organization.signed_event.clone()]);
        assert_eq!(violation, None);
        assert_eq!(state.declared_kind(), Some(DeclaredKind::Organization));
        assert_eq!(state.ledger(), Some(organization.event_id.into()));
        assert_eq!(
            state.root(),
            Some(LedgerRoot::Identity {
                founder: founder.event_id.into(),
                founder_key: secret(1).public(),
            })
        );
        assert!(!state.root().expect("a root").is_raw());
        assert_eq!(state.root_identity(), Some(founder.event_id.into()));
        assert_eq!(state.principals().len(), 1);
        let principal = state
            .principal(&IdentityId::from(founder.event_id))
            .expect("the founder is a principal");
        assert_eq!(principal.role, Role::Controller);
        assert_eq!(principal.active_key, secret(1).public());
        assert!(state.authorized_signer(&secret(1).public()));
        assert!(!state.authorized_signer(&secret(6).public()));
    }

    #[test]
    fn a_controller_may_attest_on_an_identity_rooted_ledger() {
        let founder = alice();
        let organization = founded_by(&secret(1), &founder);
        let mut chain = Chain::start(&organization);
        let attestation = chain.push(attest(&chain, &secret(1), subject(9)));

        let state = chain.state();
        assert!(state.trusts(subject(9)));
        // The signer is the founder, not the ledger, and the state says so.
        assert_eq!(
            state
                .attestation(&attestation)
                .expect("recorded")
                .signing_principal,
            SigningPrincipal {
                identity: founder.event_id.into(),
                key: secret(1).public(),
            }
        );
    }

    // Membership on every ledger (proposal 002 section 4).

    /// The delegation the unified ledger exists for: a raw-rooted ledger
    /// admits a second controller, who then signs for it.
    #[test]
    fn a_raw_rooted_ledger_delegates_signing_to_a_second_controller() {
        let root = alice();
        let delegate = bob();
        let delegate_id: IdentityId = delegate.event_id.into();
        let mut chain = Chain::start(&root);

        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &delegate,
            &secret(7),
            Role::Controller,
        ));
        // The invitation alone admits nobody.
        let offered = chain.state();
        assert_eq!(offered.principals().len(), 1);
        assert_eq!(
            offered.invitation(&invitation).expect("recorded").status,
            InvitationStatus::Open
        );
        assert!(!offered.authorized_signer(&secret(7).public()));

        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            delegate_id,
            invitation,
        ));
        let attestation = chain.push(attest(&chain, &secret(7), subject(9)));

        let state = chain.state();
        assert_eq!(
            state.invitation(&invitation).expect("recorded").status,
            InvitationStatus::Accepted
        );
        assert_eq!(state.principals().len(), 2);
        assert_eq!(
            state.principal(&delegate_id),
            Some(&Principal {
                role: Role::Controller,
                active_key: secret(7).public(),
            })
        );
        assert!(state.authorized_signer(&secret(7).public()));
        assert_eq!(
            state.controller_keys(),
            [secret(1).public(), secret(7).public()]
        );
        // The delegate's signature is attributed to the delegate, never to the
        // ledger's own identity.
        assert_eq!(
            state
                .attestation(&attestation)
                .expect("recorded")
                .signing_principal,
            SigningPrincipal {
                identity: delegate_id,
                key: secret(7).public(),
            }
        );
        assert!(state.trusts(subject(9)));
    }

    /// Latest wins, whole document: each update replaces both names, and an
    /// omitted field clears that name (proposal 003 section 1).
    #[test]
    fn a_profile_update_replaces_the_whole_profile() {
        let root = alice();
        let root_id: IdentityId = root.event_id.into();
        let mut chain = Chain::start(&root);
        assert_eq!(chain.state().profile(), None, "no update, no profile");

        let first = chain.push(set_profile(
            &chain,
            &secret(1),
            Some("Alice Ashworth"),
            Some("alice.example"),
        ));
        assert_eq!(
            chain.state().profile(),
            Some(&Profile {
                display_name: Some("Alice Ashworth".to_owned()),
                hostname: Some("alice.example".to_owned()),
                signing_principal: SigningPrincipal {
                    identity: root_id,
                    key: secret(1).public(),
                },
                event: first,
                seq: 1,
            })
        );

        // A second update carrying only a hostname drops the display name: the
        // payload is the whole document, not a patch.
        let second = chain.push(set_profile(
            &chain,
            &secret(1),
            None,
            Some("ashworth.example"),
        ));
        let profile = chain.state().profile().expect("recorded").clone();
        assert_eq!(profile.display_name, None);
        assert_eq!(profile.hostname.as_deref(), Some("ashworth.example"));
        assert_eq!(profile.event, second);
        assert_eq!(profile.seq, 2);

        // A zero-length payload clears both and still records who cleared them.
        let third = chain.push(set_profile(&chain, &secret(1), None, None));
        let profile = chain.state().profile().expect("recorded").clone();
        assert_eq!(profile.display_name, None);
        assert_eq!(profile.hostname, None);
        assert_eq!(profile.event, third);
        assert_eq!(profile.seq, 3);
        assert_eq!(profile.signing_principal.identity, root_id);
    }

    /// A no-op update is a valid event: refusing one is a node-side guard, and
    /// the fold takes whatever a valid chain holds (proposal 003 section 1).
    #[test]
    fn a_profile_update_repeating_the_current_profile_is_accepted() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(set_profile(&chain, &secret(1), Some("Alice"), None));
        let repeat = chain.push(set_profile(&chain, &secret(1), Some("Alice"), None));

        let profile = chain.state().profile().expect("recorded").clone();
        assert_eq!(profile.display_name.as_deref(), Some("Alice"));
        assert_eq!(profile.event, repeat, "the later event owns the profile");
        assert_eq!(profile.seq, 2);
    }

    /// Any current `CONTROLLER` may rename the ledger, so the profile records
    /// which principal did (proposal 003 section 1).
    #[test]
    fn a_delegate_controller_may_set_the_profile_of_an_identity_rooted_ledger() {
        let founder = alice();
        let founder_id: IdentityId = founder.event_id.into();
        let delegate = bob();
        let delegate_id: IdentityId = delegate.event_id.into();
        let organization = founded_by(&secret(1), &founder);
        let mut chain = Chain::start(&organization);

        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &delegate,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            delegate_id,
            invitation,
        ));
        let event = chain.push(set_profile(
            &chain,
            &secret(7),
            Some("Ashworth Ltd"),
            Some("ashworth.example"),
        ));

        let profile = chain.state().profile().expect("recorded").clone();
        assert_eq!(profile.display_name.as_deref(), Some("Ashworth Ltd"));
        assert_eq!(profile.event, event);
        assert_eq!(
            profile.signing_principal,
            SigningPrincipal {
                identity: delegate_id,
                key: secret(7).public(),
            },
            "the delegate signed, not the founder"
        );
        assert_ne!(profile.signing_principal.identity, founder_id);
    }

    /// The profile is legal on a raw-rooted ledger too, and a non-controller
    /// cannot set one.
    #[test]
    fn a_profile_update_from_an_unauthorized_key_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain
            .events
            .push(set_profile(&chain, &secret(7), Some("Mallory"), None).signed_event);
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::UnauthorizedSigner {
                    key: secret(7).public(),
                },
            }
        );
    }

    #[test]
    fn a_member_is_recorded_and_may_not_sign() {
        let root = alice();
        let member = bob();
        let member_id: IdentityId = member.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Member,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, invitation));

        let state = chain.state();
        assert_eq!(
            state.principal(&member_id).expect("recorded").role,
            Role::Member
        );
        assert!(!state.authorized_signer(&secret(7).public()));

        chain.push(attest(&chain, &secret(7), subject(9)));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 3,
                reason: Reason::UnauthorizedSigner {
                    key: secret(7).public(),
                },
            }
        );
    }

    /// Promotion: an invitation naming an existing principal keeps its key and
    /// changes only the role.
    #[test]
    fn a_member_is_promoted_by_a_second_invitation_carrying_the_same_key() {
        let root = alice();
        let member = bob();
        let member_id: IdentityId = member.event_id.into();
        let mut chain = Chain::start(&root);
        let first = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Member,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, first));
        let second = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, second));

        let state = chain.state();
        assert_eq!(state.principals().len(), 2);
        assert_eq!(
            state.principal(&member_id),
            Some(&Principal {
                role: Role::Controller,
                active_key: secret(7).public(),
            })
        );
        assert_eq!(state.invitations().len(), 2);
    }

    /// Promotion is what gives a key authority, so the promoted key must be
    /// able to sign the next event.
    #[test]
    fn a_promoted_member_signs_the_next_event() {
        let root = alice();
        let member = bob();
        let member_id: IdentityId = member.event_id.into();
        let mut chain = Chain::start(&root);
        let joined = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Member,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, joined));

        // As a MEMBER, bob's key may not append.
        let mut refused = Chain {
            events: chain.events.clone(),
            ..chain
        };
        refused.push(attest(&refused, &secret(7), subject(9)));
        assert_eq!(
            refused.violation(),
            Violation {
                seq: 3,
                reason: Reason::UnauthorizedSigner {
                    key: secret(7).public(),
                },
            }
        );

        let promoted = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, promoted));
        let signed = chain.push(attest(&chain, &secret(7), subject(9)));

        let state = chain.state();
        assert_eq!(state.head().expect("a head").event_id, signed);
        assert_eq!(
            state
                .attestation(&signed)
                .expect("the attestation is folded")
                .signing_principal,
            SigningPrincipal {
                identity: member_id,
                key: secret(7).public(),
            },
            "the event is attributed to the promoted principal, not to the ledger"
        );
        assert!(state.authorized_signer(&secret(7).public()));
    }

    /// The acceptance blob is the invitee's; the event carrying it is an
    /// ordinary append, so a controller of this ledger must sign it.
    #[test]
    fn an_acceptance_signed_by_a_non_controller_is_rejected() {
        let root = alice();
        let invitee = bob();
        let invitee_id: IdentityId = invitee.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Controller,
        ));
        // Bob signs both the blob and the event that carries it, and bob holds
        // no principal on this ledger until the acceptance lands.
        chain.push(admit(
            &chain,
            &secret(7),
            &secret(7),
            invitee_id,
            invitation,
        ));

        assert_eq!(
            chain.violation(),
            Violation {
                seq: 2,
                reason: Reason::UnauthorizedSigner {
                    key: secret(7).public(),
                },
            }
        );
    }

    /// A key other than the principal's is a rotation, which is out of scope.
    #[test]
    fn an_invitation_naming_a_principal_with_another_key_is_rejected() {
        let root = alice();
        let member = bob();
        let member_id: IdentityId = member.event_id.into();
        // A second inception for the same identity id is impossible, so the
        // mismatch is built by hand: the invitation embeds Bob's inception but
        // records another key, which the field table catches first. The fold
        // rule is reached through a principal whose key the ledger recorded
        // from a different inception, so this case pins the field table.
        let mut chain = Chain::start(&root);
        let first = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Member,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, first));
        chain.push(
            build_membership_invitation(
                &secret(1),
                &chain.at(),
                member_id,
                &secret(9).public(),
                Role::Controller,
                &member.signed_event,
                chain.now(),
            )
            .expect("builds"),
        );
        assert_eq!(chain.violation().code(), "inception_key_mismatch");
    }

    #[test]
    fn a_second_open_invitation_for_the_same_invitee_is_rejected() {
        let root = alice();
        let invitee = bob();
        let mut chain = Chain::start(&root);
        let first = chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Member,
        ));
        chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Controller,
        ));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 2,
                reason: Reason::DuplicateInvitation {
                    invitee: invitee.event_id.into(),
                    invitation: first,
                },
            }
        );
    }

    #[test]
    fn an_invitation_naming_the_ledger_itself_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(invite(&chain, &secret(1), &root, &secret(1), Role::Member));
        // The field table compares `invitee` with the `ledger` the event
        // names, so no ordinary principal can shadow the root.
        assert_eq!(chain.violation().code(), "fields_must_differ");
    }

    #[test]
    fn an_invitation_is_single_use_on_this_branch() {
        let root = alice();
        let invitee = bob();
        let invitee_id: IdentityId = invitee.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Member,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            invitee_id,
            invitation,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            invitee_id,
            invitation,
        ));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 3,
                reason: Reason::InvitationNotOpen {
                    invitation,
                    status: InvitationStatus::Accepted,
                },
            }
        );
    }

    /// The four transplants of proposal 002 section 4: a valid acceptance
    /// blob, presented where it does not belong.
    #[test]
    fn a_transplanted_acceptance_is_rejected() {
        let root = alice();
        let invitee = bob();
        let invitee_id: IdentityId = invitee.event_id.into();
        let other = carol();
        let other_id: IdentityId = other.event_id.into();

        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Member,
        ));
        let at = chain.at();
        let now = chain.now();
        let ledger = chain.ledger;
        let transplant = |accepted: DetachedAcceptance| {
            let mut branch = Chain {
                ledger,
                events: chain.events.clone(),
                prev: at.prev,
                seq: at.seq,
                timestamp_ms: at.prev_timestamp_ms,
            };
            branch.push(
                build_membership_acceptance(&secret(1), &at, &accepted, now).expect("builds"),
            );
            branch.violation().reason
        };

        // Another ledger: the blob names an organization Bob was invited to.
        let organization = founded_by(&secret(1), &root);
        assert_eq!(
            transplant(build_acceptance(
                &secret(7),
                organization.event_id.into(),
                invitation,
                invitee_id
            )),
            Reason::AcceptanceForAnotherLedger {
                named: organization.event_id.into(),
                expected: ledger,
            }
        );

        // Another invitation: the blob names an event this ledger does not
        // hold.
        let unknown = EventId::from_bytes([0xee; ID_BYTES]);
        assert_eq!(
            transplant(build_acceptance(&secret(7), ledger, unknown, invitee_id)),
            Reason::UnknownInvitation(unknown)
        );

        // Another identity: Carol signs for the invitation that named Bob.
        assert_eq!(
            transplant(build_acceptance(&secret(9), ledger, invitation, other_id)),
            Reason::AcceptanceInviteeMismatch {
                named: other_id,
                invited: invitee_id,
            }
        );

        // Another key: the blob names Bob but was signed by a key the
        // invitation does not record.
        assert_eq!(
            transplant(build_acceptance(&secret(9), ledger, invitation, invitee_id)),
            Reason::AcceptanceInviteeKeyMismatch {
                named: secret(9).public(),
                invited: secret(7).public(),
            }
        );
    }

    /// Two inceptions may record one key under two identity ids. Admitting the
    /// second would give one signer two principals, so admission refuses it.
    #[test]
    fn a_second_identity_presenting_a_principal_key_is_rejected_at_admission() {
        let root = alice();
        let twin = raw_rooted(1, 0x5b);
        let twin_id: IdentityId = twin.event_id.into();
        assert_ne!(twin_id, IdentityId::from(root.event_id));

        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(&chain, &secret(1), &twin, &secret(1), Role::Member));
        chain.push(admit(&chain, &secret(1), &secret(1), twin_id, invitation));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 2,
                reason: Reason::DuplicatePrincipalKey {
                    key: secret(1).public(),
                    held_by: root.event_id.into(),
                },
            }
        );
    }

    #[test]
    fn a_removal_cancels_an_open_invitation() {
        let root = alice();
        let invitee = bob();
        let invitee_id: IdentityId = invitee.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Member,
        ));
        chain.push(remove(&chain, &secret(1), invitee_id));

        let state = chain.state();
        assert_eq!(
            state.invitation(&invitation).expect("recorded").status,
            InvitationStatus::Cancelled
        );
        assert!(state.principal(&invitee_id).is_none());

        // A cancelled invitation cannot then be accepted.
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            invitee_id,
            invitation,
        ));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 3,
                reason: Reason::InvitationNotOpen {
                    invitation,
                    status: InvitationStatus::Cancelled,
                },
            }
        );
    }

    #[test]
    fn a_removal_removes_a_principal_and_its_authority() {
        let root = alice();
        let delegate = bob();
        let delegate_id: IdentityId = delegate.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &delegate,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            delegate_id,
            invitation,
        ));
        chain.push(remove(&chain, &secret(1), delegate_id));

        let state = chain.state();
        assert_eq!(state.principals().len(), 1);
        assert!(!state.authorized_signer(&secret(7).public()));

        chain.push(attest(&chain, &secret(7), subject(9)));
        assert_eq!(chain.violation().code(), "unauthorized_signer");
    }

    #[test]
    fn a_removal_naming_nobody_is_rejected() {
        let root = alice();
        let mut chain = Chain::start(&root);
        chain.push(remove(&chain, &secret(1), subject(0x42)));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::UnknownRemovalTarget(subject(0x42)),
            }
        );
    }

    #[test]
    fn the_raw_root_is_never_removable() {
        let root = alice();
        let root_id: IdentityId = root.event_id.into();
        let delegate = bob();
        let delegate_id: IdentityId = delegate.event_id.into();
        let mut chain = Chain::start(&root);

        // Even with a second controller in place, the root stays.
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &delegate,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            delegate_id,
            invitation,
        ));
        chain.push(remove(&chain, &secret(7), root_id));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 3,
                reason: Reason::RootNotRemovable(root_id),
            }
        );
    }

    /// On a raw-rooted ledger the root counts toward the minimum, so removing
    /// the only other controller is legal.
    #[test]
    fn a_raw_root_keeps_the_ledger_signable_after_every_other_removal() {
        let root = alice();
        let delegate = bob();
        let delegate_id: IdentityId = delegate.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &delegate,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            delegate_id,
            invitation,
        ));
        // The delegate removes itself.
        chain.push(remove(&chain, &secret(7), delegate_id));
        let state = chain.state();
        assert_eq!(state.controller_keys(), [secret(1).public()]);
    }

    #[test]
    fn an_identity_rooted_ledger_refuses_to_lose_its_last_controller() {
        let founder = alice();
        let founder_id: IdentityId = founder.event_id.into();
        let organization = founded_by(&secret(1), &founder);
        let mut chain = Chain::start(&organization);
        chain.push(remove(&chain, &secret(1), founder_id));
        assert_eq!(
            chain.violation(),
            Violation {
                seq: 1,
                reason: Reason::LastController(founder_id),
            }
        );
    }

    /// Self-removal is legal once someone else can sign.
    #[test]
    fn a_founder_may_remove_itself_after_admitting_another_controller() {
        let founder = alice();
        let founder_id: IdentityId = founder.event_id.into();
        let successor = bob();
        let successor_id: IdentityId = successor.event_id.into();
        let organization = founded_by(&secret(1), &founder);
        let mut chain = Chain::start(&organization);

        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &successor,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            successor_id,
            invitation,
        ));
        chain.push(remove(&chain, &secret(1), founder_id));

        let state = chain.state();
        assert!(state.principal(&founder_id).is_none());
        assert_eq!(state.controller_keys(), [secret(7).public()]);
        // The founder is still the root of record; it is simply no longer a
        // principal.
        assert_eq!(state.root_identity(), Some(founder_id));
        assert!(!state.authorized_signer(&secret(1).public()));
    }

    /// The founder of an identity-rooted ledger invites itself back as a
    /// `MEMBER`. Admitting that would leave nobody who may append, so the fold
    /// refuses it (proposal 002, clarifications of 2026-08-25).
    #[test]
    fn an_acceptance_demoting_the_last_controller_is_rejected() {
        let founder = alice();
        let founder_id: IdentityId = founder.event_id.into();
        let organization = founded_by(&secret(1), &founder);
        let mut chain = Chain::start(&organization);

        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &founder,
            &secret(1),
            Role::Member,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(1),
            founder_id,
            invitation,
        ));

        assert_eq!(
            chain.violation(),
            Violation {
                seq: 2,
                reason: Reason::DemotesLastController(founder_id),
            }
        );
    }

    /// The same demotion is legal once someone else can sign.
    #[test]
    fn a_controller_may_be_demoted_once_another_controller_exists() {
        let founder = alice();
        let founder_id: IdentityId = founder.event_id.into();
        let successor = bob();
        let successor_id: IdentityId = successor.event_id.into();
        let organization = founded_by(&secret(1), &founder);
        let mut chain = Chain::start(&organization);

        let promotion = chain.push(invite(
            &chain,
            &secret(1),
            &successor,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            successor_id,
            promotion,
        ));
        let demotion = chain.push(invite(
            &chain,
            &secret(1),
            &founder,
            &secret(1),
            Role::Member,
        ));
        chain.push(admit(&chain, &secret(1), &secret(1), founder_id, demotion));

        let state = chain.state();
        assert_eq!(
            state
                .principal(&founder_id)
                .expect("still a principal")
                .role,
            Role::Member
        );
        assert_eq!(state.controller_keys(), [secret(7).public()]);
        assert!(!state.authorized_signer(&secret(1).public()));
    }

    /// The raw root is not demotable, whatever else the principal set holds.
    ///
    /// No invitation can reach this rule: the field table refuses an
    /// invitation whose invitee is the ledger id, which is what a raw root's
    /// identity is. The check is here for the same reason the removal check
    /// is, and is tested where it lives.
    #[test]
    fn the_raw_root_is_never_demoted() {
        let root = alice();
        let root_id: IdentityId = root.event_id.into();
        let delegate = bob();
        let delegate_id: IdentityId = delegate.event_id.into();
        let mut chain = Chain::start(&root);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &delegate,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            delegate_id,
            invitation,
        ));

        let state = chain.state();
        assert_eq!(
            state.check_demotion(root_id),
            Err(Reason::RootNotDemotable(root_id))
        );
        // The second controller may be demoted: it is not the root.
        assert_eq!(state.check_demotion(delegate_id), Ok(()));
    }

    /// A `MEMBER` carries no authority, so removing one never threatens the
    /// controller minimum.
    #[test]
    fn removing_a_member_never_trips_the_last_controller_rule() {
        let founder = alice();
        let organization = founded_by(&secret(1), &founder);
        let member = bob();
        let member_id: IdentityId = member.event_id.into();
        let mut chain = Chain::start(&organization);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &member,
            &secret(7),
            Role::Member,
        ));
        chain.push(admit(&chain, &secret(1), &secret(7), member_id, invitation));
        chain.push(remove(&chain, &secret(1), member_id));

        let state = chain.state();
        assert_eq!(state.principals().len(), 1);
        assert_eq!(state.controller_keys(), [secret(1).public()]);
    }

    #[test]
    fn malformed_bytes_are_reported_as_a_wire_violation() {
        let root = alice();
        let mut truncated = root.signed_event.clone();
        truncated.truncate(root.signed_event.len() - 1);
        let (state, violation) = fold(vec![truncated]);
        assert!(state.is_empty());
        let violation = violation.expect("the fold reports a violation");
        assert_eq!(violation.seq, 0);
        assert!(matches!(violation.reason, Reason::Wire(_)));
    }

    #[test]
    fn violation_codes_are_stable() {
        let key = secret(1).public();
        let other_key = secret(7).public();
        let event = |seed: u8| EventId::from_bytes([seed; ID_BYTES]);
        let cases = [
            (
                Reason::Wire(WireError::InceptionPastSeqZero),
                "inception_past_seq_zero",
            ),
            (
                Reason::WrongSeq {
                    expected: 1,
                    found: 2,
                },
                "wrong_seq",
            ),
            (
                Reason::WrongLedger {
                    expected: subject(1),
                    found: subject(2),
                },
                "wrong_ledger",
            ),
            (
                Reason::BrokenPrevLink {
                    expected: event(1),
                    found: event(2),
                },
                "broken_prev_link",
            ),
            (
                Reason::BackwardsTimestamp {
                    previous: 2,
                    found: 1,
                },
                "backwards_timestamp",
            ),
            (
                Reason::PayloadNotAllowed {
                    seq: 3,
                    payload: "Inception",
                },
                "payload_not_allowed",
            ),
            (
                Reason::InvalidPublicKey {
                    field: "EventBody.author_key",
                },
                "invalid_public_key",
            ),
            (Reason::UnauthorizedSigner { key }, "unauthorized_signer"),
            (Reason::BadSignature, "bad_signature"),
            (
                Reason::DuplicateAttestation {
                    subject: subject(1),
                    attestation: event(2),
                },
                "duplicate_attestation",
            ),
            (Reason::SelfAttestation(subject(1)), "self_attestation"),
            (
                Reason::UnknownRevocationTarget(event(1)),
                "unknown_revocation_target",
            ),
            (
                Reason::AlreadyRevoked {
                    target: event(1),
                    revoked_by: event(2),
                },
                "already_revoked",
            ),
            (
                Reason::DuplicateInvitation {
                    invitee: subject(1),
                    invitation: event(2),
                },
                "duplicate_invitation",
            ),
            (
                Reason::PrincipalKeyMismatch {
                    identity: subject(1),
                    expected: key,
                    found: other_key,
                },
                "principal_key_mismatch",
            ),
            (
                Reason::AcceptanceForAnotherLedger {
                    named: subject(1),
                    expected: subject(2),
                },
                "acceptance_for_another_ledger",
            ),
            (Reason::UnknownInvitation(event(1)), "unknown_invitation"),
            (
                Reason::InvitationNotOpen {
                    invitation: event(1),
                    status: InvitationStatus::Accepted,
                },
                "invitation_not_open",
            ),
            (
                Reason::AcceptanceInviteeMismatch {
                    named: subject(1),
                    invited: subject(2),
                },
                "acceptance_invitee_mismatch",
            ),
            (
                Reason::AcceptanceInviteeKeyMismatch {
                    named: key,
                    invited: other_key,
                },
                "acceptance_invitee_key_mismatch",
            ),
            (
                Reason::DuplicatePrincipalKey {
                    key,
                    held_by: subject(1),
                },
                "duplicate_principal_key",
            ),
            (Reason::RootNotRemovable(subject(1)), "root_not_removable"),
            (
                Reason::UnknownRemovalTarget(subject(1)),
                "unknown_removal_target",
            ),
            (Reason::LastController(subject(1)), "last_controller"),
            (Reason::RootNotDemotable(subject(1)), "root_not_demotable"),
            (
                Reason::DemotesLastController(subject(1)),
                "demotes_last_controller",
            ),
        ];
        for (reason, code) in cases {
            assert_eq!(reason.code(), code);
            let violation = Violation { seq: 3, reason };
            assert_eq!(violation.code(), code);
            assert!(violation.to_string().starts_with("seq 3: "));
        }
    }

    #[test]
    fn declared_kinds_render_in_full_words() {
        assert_eq!(declared_kind_name(DeclaredKind::Person), "person");
        assert_eq!(
            declared_kind_name(DeclaredKind::Organization),
            "organization"
        );
        assert_eq!(declared_kind_name(DeclaredKind::Agent), "agent");
        assert_eq!(declared_kind_name(DeclaredKind::Service), "service");
    }

    /// Declared kind gates nothing: an `AGENT` ledger runs the same rules.
    #[test]
    fn declared_kind_gates_no_payload() {
        let owner = alice();
        let agent = build_inception(
            &secret(1),
            DeclaredKind::Agent,
            Root::Identity {
                founder: owner.event_id.into(),
                founder_inception: &owner.signed_event,
            },
            [0xa9; NONCE_BYTES],
            T0,
        )
        .expect("builds");
        let invitee = bob();
        let invitee_id: IdentityId = invitee.event_id.into();
        let mut chain = Chain::start(&agent);
        let invitation = chain.push(invite(
            &chain,
            &secret(1),
            &invitee,
            &secret(7),
            Role::Controller,
        ));
        chain.push(admit(
            &chain,
            &secret(1),
            &secret(7),
            invitee_id,
            invitation,
        ));

        let state = chain.state();
        assert_eq!(state.declared_kind(), Some(DeclaredKind::Agent));
        assert_eq!(state.principals().len(), 2);
    }
}

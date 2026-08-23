//! The one fold of proposal 001 section 3.6: an event sequence in, a state
//! and at most one violation out.
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
//! 3. The payload is one the ledger's kind accepts.
//! 4. `author_key` is authorized by the state from `0..=i-1`; seq 0 is
//!    authorized by itself.
//! 5. The signature verifies over `sign_input` of the *received* body bytes.
//! 6. The payload's semantic rules, then the payload is applied.
//!
//! Person semantics are complete here. Org ledgers get their inception state
//! (the founder as `CONTROLLER`) and the kind table, and their membership
//! payloads report [`Reason::OrgSemanticsPending`] until ticket 005 lands.
//!
//! Two seams keep the kinds from spreading through the fold: every seq-0
//! payload seeds the state in [`LedgerState::seed_from_inception`], and every
//! authorization question goes through [`LedgerState::authorized_signer`],
//! which reads the principal set both kinds share.

use std::collections::BTreeMap;
use std::fmt;

use iroh_base::{EndpointId, PublicKey, Signature};
use mabel_proto::prost::Message;
use mabel_proto::v0::{EventBody, Role, SignedEvent, event_body::Payload};

use crate::digest::{event_id, sign_input};
use crate::id::{EventId, IdentityId, LedgerId};
use crate::validate::{self, WireError};
use crate::{ID_BYTES, SIG_BYTES};

/// What an identity's seq-0 event made it (proposal 001 section 3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LedgerKind {
    /// A person, incepted by a `PersonInception`.
    Person,
    /// An organization, incepted by an `OrgInception`.
    Org,
}

impl LedgerKind {
    /// The lowercase name this kind carries in output and in artifacts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Org => "org",
        }
    }
}

impl fmt::Display for LedgerKind {
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

/// What a `PersonInception` fixed for life (rotation is out of scope, so the
/// active key never changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonState {
    /// The key every event on this ledger is signed by.
    pub active_key: PublicKey,
    /// `reserve_commit(reserve_key)`; the reserve key itself is never
    /// recorded.
    pub reserve_commit: [u8; ID_BYTES],
}

/// An identity the ledger has recorded, and what it may do.
///
/// Both kinds use this: a person ledger holds exactly one principal, itself,
/// and an org ledger holds one per identity it has admitted. Authorization
/// reads nothing else, so [`LedgerState::authorized_signer`] is the same
/// function for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Principal {
    /// `CONTROLLER` may append to this ledger; `MEMBER` is recorded data only
    /// (proposal 001 section 3.4).
    pub role: Role,
    /// The active key this ledger recorded for the identity, proven by the
    /// inception that named it.
    pub active_key: PublicKey,
}

/// Where an `OrgInvite` stands (proposal 001 section 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteStatus {
    /// Issued and neither accepted nor cancelled.
    Open,
    /// An `OrgAcceptance` consumed it.
    Accepted,
    /// An `OrgRemoval` cancelled it.
    Cancelled,
}

/// One `OrgInvite` and what became of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invite {
    /// The identity invited.
    pub invitee: IdentityId,
    /// That identity's active key.
    pub invitee_key: PublicKey,
    /// The role the invite offers.
    pub role: Role,
    /// Whether the invite is still open.
    pub status: InviteStatus,
}

/// The org branch of the folded state.
///
/// Membership lives in the shared principal set, not here. Ticket 004 seeds
/// the founder from `OrgInception` and stops there; ticket 005 fills in the
/// invite lifecycle, acceptance and removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgState {
    /// The identity that founded the org, seeded as a `CONTROLLER`.
    pub founder: IdentityId,
    /// Every invite the ledger has issued, by the `event_id` of its
    /// `OrgInvite`.
    pub invites: BTreeMap<EventId, Invite>,
}

/// One `TrustAttestation` and its revocation status.
///
/// Nothing is ever deleted (decisions/003-trust): a revoked attestation stays
/// in the map with the revoking event recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attestation {
    /// The identity the attestation names.
    pub subject: IdentityId,
    /// The `event_id` of the `TrustRevocation` that revoked it, if one did.
    pub revoked_by: Option<EventId>,
}

impl Attestation {
    /// Whether a later `TrustRevocation` named this attestation.
    pub const fn is_revoked(&self) -> bool {
        self.revoked_by.is_some()
    }
}

/// The fold of a valid event prefix (proposal 001 section 3.6).
///
/// A default `LedgerState` is the state before any event: no ledger id, no
/// kind, no head.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LedgerState {
    kind: Option<LedgerKind>,
    ledger: Option<LedgerId>,
    head: Option<Head>,
    principals: BTreeMap<IdentityId, Principal>,
    person: Option<PersonState>,
    org: Option<OrgState>,
    witnesses: Vec<EndpointId>,
    trust: BTreeMap<EventId, Attestation>,
}

impl LedgerState {
    /// Whether no event has been applied yet.
    pub const fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// The ledger kind its seq-0 event set.
    pub const fn kind(&self) -> Option<LedgerKind> {
        self.kind
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
    ///
    /// A person ledger holds exactly one entry, itself; an org ledger holds
    /// its founder and everyone it has admitted.
    pub const fn principals(&self) -> &BTreeMap<IdentityId, Principal> {
        &self.principals
    }

    /// The principal recorded for `identity`, if the ledger records one.
    pub fn principal(&self, identity: &IdentityId) -> Option<&Principal> {
        self.principals.get(identity)
    }

    /// The person branch of the state, present on a person ledger.
    pub const fn person(&self) -> Option<&PersonState> {
        self.person.as_ref()
    }

    /// The org branch of the state, present on an org ledger.
    pub const fn org(&self) -> Option<&OrgState> {
        self.org.as_ref()
    }

    /// The current witness set, in the order the last `WitnessConfig` listed.
    pub fn witnesses(&self) -> &[EndpointId] {
        &self.witnesses
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

    /// Whether `key` may sign the next event.
    ///
    /// This is the fold's only authorization question and the only place the
    /// answer is computed: a key is authorized when the principal set holds a
    /// `CONTROLLER` with that active key. On a person ledger that set holds
    /// the person alone, so this is "the active key" (proposal 001 section
    /// 3.4).
    pub fn authorized_signer(&self, key: &PublicKey) -> bool {
        self.principals
            .values()
            .any(|principal| principal.role == Role::Controller && &principal.active_key == key)
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

        // 3. The payload is one this ledger kind accepts.
        self.check_kind(seq, payload)?;

        // 4. Authorization against the state from `0..=i-1`. Seq 0 is
        //    authorized by itself: the field table already tied `author_key`
        //    to the inception's `active_key` or `founder_key`.
        let author_key = public_key(&body.author_key, "EventBody.author_key")?;
        if seq > 0 && !self.authorized_signer(&author_key) {
            return Err(Reason::UnauthorizedSigner { key: author_key });
        }

        // 5. The signature, over the received body bytes.
        verify(&author_key, &sign_input(&signed.body), &signed.sig)?;

        // 6. The semantic rules, then the payload.
        let effect = self.check_semantics(id, payload)?;
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

    /// The valid-payloads-by-kind table of proposal 001 section 3.4.
    fn check_kind(&self, seq: u64, payload: &Payload) -> Result<(), Reason> {
        match payload {
            // An inception sets the kind; the field table already pinned it to
            // seq 0, so the guards below only ever fire on a corrupt state.
            Payload::PersonInception(_) | Payload::OrgInception(_) if seq == 0 => Ok(()),
            Payload::PersonInception(_) | Payload::OrgInception(_) => {
                Err(WireError::InceptionPastSeqZero.into())
            }
            // Every ledger kind holds these three.
            Payload::WitnessConfig(_)
            | Payload::TrustAttestation(_)
            | Payload::TrustRevocation(_) => {
                self.kind.ok_or(WireError::NonInceptionAtSeqZero)?;
                Ok(())
            }
            // The rest are org-only.
            _ => match self.kind.ok_or(WireError::NonInceptionAtSeqZero)? {
                LedgerKind::Org => Ok(()),
                kind => Err(Reason::PayloadNotAllowed {
                    kind,
                    payload: payload_name(payload),
                }),
            },
        }
    }

    /// The one place a seq-0 payload becomes state: it names the kind, the
    /// principal set and the kind-specific record.
    fn seed_from_inception(&self, id: EventId, payload: &Payload) -> Result<Effect, Reason> {
        let (kind, identity, active_key, person, org) = match payload {
            Payload::PersonInception(inception) => {
                let active_key = public_key(&inception.active_key, "PersonInception.active_key")?;
                let person = PersonState {
                    active_key,
                    reserve_commit: inception
                        .reserve_commit
                        .as_slice()
                        .try_into()
                        .expect("reserve_commit is 32 bytes"),
                };
                // A person's identity id is the id of this very event.
                (
                    LedgerKind::Person,
                    IdentityId::from(id),
                    active_key,
                    Some(person),
                    None,
                )
            }
            Payload::OrgInception(inception) => {
                let founder = identity(&inception.founder);
                let active_key = public_key(&inception.founder_key, "OrgInception.founder_key")?;
                // The field table proved the embedded inception hashes to
                // `founder` and records `founder_key`, so the founder becomes
                // a controller with no cross-ledger lookup (section 3.4).
                let org = OrgState {
                    founder,
                    invites: BTreeMap::new(),
                };
                (LedgerKind::Org, founder, active_key, None, Some(org))
            }
            _ => return Err(WireError::NonInceptionAtSeqZero.into()),
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
            kind,
            principals,
            person,
            org,
        })))
    }

    /// The semantic rules of proposal 001 section 3.4, run against the state
    /// from `0..=i-1`. Returns what to apply; nothing is mutated here.
    fn check_semantics(&self, id: EventId, payload: &Payload) -> Result<Effect, Reason> {
        match payload {
            Payload::PersonInception(_) | Payload::OrgInception(_) => {
                self.seed_from_inception(id, payload)
            }
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
                Ok(Effect::Attest(subject))
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
            // Ticket 005 replaces this arm with the invite lifecycle,
            // acceptance and removal semantics of sections 3.4 and 3.5.
            Payload::OrgInvite(_) | Payload::OrgAcceptance(_) | Payload::OrgRemoval(_) => {
                Err(Reason::OrgSemanticsPending {
                    payload: payload_name(payload),
                })
            }
        }
    }

    /// Applies a checked event. Every mutation the fold makes happens here,
    /// after every check has passed (pitfall 3).
    fn commit(&mut self, seq: u64, id: EventId, timestamp_ms: u64, effect: Effect) {
        match effect {
            Effect::Seed(seed) => {
                let Seed {
                    kind,
                    principals,
                    person,
                    org,
                } = *seed;
                self.kind = Some(kind);
                self.ledger = Some(id.into());
                self.principals = principals;
                self.person = person;
                self.org = org;
            }
            Effect::Witnesses(witnesses) => self.witnesses = witnesses,
            Effect::Attest(subject) => {
                self.trust.insert(
                    id,
                    Attestation {
                        subject,
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
        }
        self.head = Some(Head {
            seq,
            event_id: id,
            timestamp_ms,
        });
    }
}

/// What a checked event changes, produced only once every check has passed.
enum Effect {
    Seed(Box<Seed>),
    Witnesses(Vec<EndpointId>),
    Attest(IdentityId),
    Revoke(EventId),
}

/// What a seq-0 payload seeds.
struct Seed {
    kind: LedgerKind,
    principals: BTreeMap<IdentityId, Principal>,
    person: Option<PersonState>,
    org: Option<OrgState>,
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
    /// The payload is not one this ledger kind holds.
    #[error("a {kind} ledger does not hold a {payload} payload")]
    PayloadNotAllowed {
        /// The ledger's kind.
        kind: LedgerKind,
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
    /// `author_key` is not authorized by the state before this event.
    #[error("author_key {key} may not sign this event")]
    UnauthorizedSigner {
        /// The key the event names.
        key: PublicKey,
    },
    /// The signature did not verify under `author_key`.
    #[error("SignedEvent.sig does not verify under author_key")]
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
    /// An org membership payload reached the fold before ticket 005 gave it
    /// semantics.
    #[error("{payload} semantics are not implemented")]
    OrgSemanticsPending {
        /// The payload's message name.
        payload: &'static str,
    },
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
            Self::OrgSemanticsPending { .. } => "org_semantics_pending",
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
        Payload::PersonInception(_) => "PersonInception",
        Payload::OrgInception(_) => "OrgInception",
        Payload::WitnessConfig(_) => "WitnessConfig",
        Payload::TrustAttestation(_) => "TrustAttestation",
        Payload::TrustRevocation(_) => "TrustRevocation",
        Payload::OrgInvite(_) => "OrgInvite",
        Payload::OrgAcceptance(_) => "OrgAcceptance",
        Payload::OrgRemoval(_) => "OrgRemoval",
    }
}

/// Reads a 32-byte identity id the field table already length-checked.
fn identity(bytes: &[u8]) -> IdentityId {
    IdentityId::from_slice(bytes).expect("an identity id is 32 bytes")
}

/// Reads a 32-byte field the field table length-checked and requires it to be
/// an ed25519 curve point, which only the key types need.
fn public_key(bytes: &[u8], field: &'static str) -> Result<PublicKey, Reason> {
    let bytes: [u8; ID_BYTES] = bytes.try_into().expect("a key field is 32 bytes");
    PublicKey::from_bytes(&bytes).map_err(|_| Reason::InvalidPublicKey { field })
}

fn verify(key: &PublicKey, input: &[u8], sig: &[u8]) -> Result<(), Reason> {
    let sig: [u8; SIG_BYTES] = sig.try_into().expect("a validated signature is 64 bytes");
    key.verify(input, &Signature::from_bytes(&sig))
        .map_err(|_| Reason::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::encode;
    use crate::sign::{
        BuiltEvent, DetachedAcceptance, Position, build_acceptance, build_org_acceptance,
        build_org_inception, build_org_invite, build_org_removal, build_person_inception,
        build_trust_attestation, build_trust_revocation, build_witness_config,
    };
    use crate::{MAX_TIMESTAMP_MS, NONCE_BYTES};
    use iroh_base::SecretKey;
    use mabel_proto::v0::TrustRevocation;

    const T0: u64 = 1_700_000_000_000;
    const STEP: u64 = 60_000;

    fn secret(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    /// The person ledger every test builds on: active key `secret(1)`.
    fn inception() -> BuiltEvent {
        build_person_inception(&secret(1), &secret(2).public(), [3u8; NONCE_BYTES], T0)
            .expect("builds")
    }

    /// A second person, so an org payload has an invitee to name.
    fn other_person() -> BuiltEvent {
        build_person_inception(&secret(7), &secret(8).public(), [4u8; NONCE_BYTES], T0)
            .expect("builds")
    }

    /// The position right after `head`, at `seq`.
    fn after(ledger: &BuiltEvent, head: &BuiltEvent, seq: u64) -> Position {
        Position {
            ledger: ledger.event_id.into(),
            seq,
            prev: head.event_id,
            prev_timestamp_ms: T0,
        }
    }

    fn subject(seed: u8) -> IdentityId {
        IdentityId::from_bytes([seed; ID_BYTES])
    }

    /// A ledger of `events` as the fold takes them.
    fn bytes(events: &[&BuiltEvent]) -> Vec<Vec<u8>> {
        events.iter().map(|e| e.signed_event.clone()).collect()
    }

    fn violation(events: &[&BuiltEvent]) -> Violation {
        fold(bytes(events)).1.expect("the fold reports a violation")
    }

    /// Signs an arbitrary body, for the cases no builder will produce.
    fn seal(signer: &SecretKey, body: &EventBody) -> Vec<u8> {
        let body = encode(body);
        let sig = signer.sign(&sign_input(&body));
        encode(&SignedEvent {
            body,
            sig: sig.to_bytes().to_vec(),
        })
    }

    #[test]
    fn an_empty_sequence_folds_to_the_empty_state() {
        let (state, violation) = fold(Vec::<Vec<u8>>::new());
        assert!(state.is_empty());
        assert_eq!(state.next_seq(), 0);
        assert_eq!(state.kind(), None);
        assert_eq!(state.ledger(), None);
        assert_eq!(violation, None);
    }

    #[test]
    fn a_person_ledger_folds_inception_witnesses_attestation_and_revocation() {
        let root = inception();
        let witnesses = build_witness_config(
            &secret(1),
            &after(&root, &root, 1),
            &[secret(4).public(), secret(5).public()],
            T0 + STEP,
        )
        .expect("builds");
        let attest = build_trust_attestation(
            &secret(1),
            &after(&root, &witnesses, 2),
            subject(9),
            T0 + 2 * STEP,
        )
        .expect("builds");
        let revoke = build_trust_revocation(
            &secret(1),
            &after(&root, &attest, 3),
            attest.event_id,
            T0 + 3 * STEP,
        )
        .expect("builds");
        // The same subject may be attested again once the first attestation is
        // revoked.
        let again = build_trust_attestation(
            &secret(1),
            &after(&root, &revoke, 4),
            subject(9),
            T0 + 4 * STEP,
        )
        .expect("builds");

        let (state, violation) = fold(bytes(&[&root, &witnesses, &attest, &revoke, &again]));
        assert_eq!(violation, None);
        assert_eq!(state.kind(), Some(LedgerKind::Person));
        assert_eq!(state.ledger(), Some(root.event_id.into()));
        let head = state.head().expect("a folded ledger has a head");
        assert_eq!(head.seq, 4);
        assert_eq!(head.event_id, again.event_id);
        assert_eq!(head.timestamp_ms, T0 + 4 * STEP);
        assert_eq!(state.next_seq(), 5);

        let person = state.person().expect("a person ledger has person state");
        assert_eq!(person.active_key, secret(1).public());
        assert_eq!(
            person.reserve_commit,
            crate::digest::reserve_commit(&secret(2).public())
        );
        // A person ledger holds one principal, itself, and that is what
        // authorizes the signer.
        assert_eq!(state.principals().len(), 1);
        let principal = state
            .principal(&IdentityId::from(root.event_id))
            .expect("the person is its own principal");
        assert_eq!(principal.role, Role::Controller);
        assert_eq!(principal.active_key, secret(1).public());
        assert!(state.authorized_signer(&secret(1).public()));
        assert!(!state.authorized_signer(&secret(6).public()));

        assert_eq!(state.witnesses(), [secret(4).public(), secret(5).public()]);

        assert_eq!(state.trust().len(), 2);
        let revoked = state.attestation(&attest.event_id).expect("recorded");
        assert_eq!(revoked.subject, subject(9));
        assert_eq!(revoked.revoked_by, Some(revoke.event_id));
        assert!(revoked.is_revoked());
        let live = state.attestation(&again.event_id).expect("recorded");
        assert_eq!(live.revoked_by, None);
        assert!(state.trusts(subject(9)));
        assert!(!state.trusts(subject(8)));
        assert!(state.org().is_none());
    }

    #[test]
    fn a_witness_config_replaces_the_whole_set() {
        let root = inception();
        let first = build_witness_config(
            &secret(1),
            &after(&root, &root, 1),
            &[secret(4).public(), secret(5).public()],
            T0,
        )
        .expect("builds");
        let second = build_witness_config(
            &secret(1),
            &after(&root, &first, 2),
            &[secret(6).public()],
            T0,
        )
        .expect("builds");

        let (state, violation) = fold(bytes(&[&root, &first, &second]));
        assert_eq!(violation, None);
        assert_eq!(state.witnesses(), [secret(6).public()]);
    }

    #[test]
    fn position_zero_requires_an_inception() {
        let root = inception();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(9), T0).unwrap();
        assert_eq!(
            violation(&[&attest]),
            Violation {
                seq: 0,
                reason: Reason::WrongSeq {
                    expected: 0,
                    found: 1,
                },
            }
        );
    }

    #[test]
    fn a_broken_prev_link_is_rejected() {
        let root = inception();
        let mut at = after(&root, &root, 1);
        at.prev = EventId::from_bytes([0xaa; ID_BYTES]);
        let attest = build_trust_attestation(&secret(1), &at, subject(9), T0).unwrap();
        assert_eq!(
            violation(&[&root, &attest]),
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
        let root = inception();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(9), T0).unwrap();
        let found = violation(&[&root, &attest, &attest]);
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
        let root = inception();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 2), subject(9), T0).unwrap();
        assert_eq!(
            violation(&[&root, &attest]),
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
        let root = inception();
        let mut at = after(&root, &root, 1);
        at.ledger = subject(0xbb);
        let attest = build_trust_attestation(&secret(1), &at, subject(9), T0).unwrap();
        assert_eq!(
            violation(&[&root, &attest]),
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
        let root = inception();
        // The builder clamps to `prev_timestamp_ms`, so the position has to
        // understate the head's timestamp for the event to go backwards.
        let mut at = after(&root, &root, 1);
        at.prev_timestamp_ms = 0;
        let attest = build_trust_attestation(&secret(1), &at, subject(9), T0 - 1).unwrap();
        assert_eq!(
            violation(&[&root, &attest]),
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
        let root = inception();
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
        let root = inception();
        // secret(6) is not this ledger's active key.
        let attest =
            build_trust_attestation(&secret(6), &after(&root, &root, 1), subject(9), T0).unwrap();
        assert_eq!(
            violation(&[&root, &attest]),
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
        let root = inception();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(9), T0).unwrap();
        // The body of one event carried with the signature of another: the
        // author is authorized, the signature is not over these bytes.
        let other =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(10), T0).unwrap();
        let mixed = encode(&SignedEvent {
            body: attest.body.clone(),
            sig: SignedEvent::decode(&other.signed_event[..]).unwrap().sig,
        });
        let (_, violation) = fold(vec![root.signed_event.clone(), mixed]);
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
        let root = inception();
        let ledger: IdentityId = root.event_id.into();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), ledger, T0).unwrap();
        // The field table catches this statelessly, comparing `subject` with
        // the `ledger` the event names; the chain rule ties that to the real
        // ledger id.
        assert_eq!(violation(&[&root, &attest]).code(), "fields_must_differ");
    }

    #[test]
    fn a_witness_that_is_not_a_public_key_is_rejected() {
        let root = inception();
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
        let (_, violation) = fold(vec![root.signed_event.clone(), event]);
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
    fn org_payloads_are_rejected_on_a_person_ledger() {
        let root = inception();
        let at = after(&root, &root, 1);
        let invitee = other_person();
        let invitee_id: IdentityId = invitee.event_id.into();

        let invite = build_org_invite(
            &secret(1),
            &at,
            invitee_id,
            &secret(7).public(),
            Role::Member,
            &invitee.signed_event,
            T0,
        )
        .expect("builds");
        let accepted: DetachedAcceptance = build_acceptance(
            &secret(7),
            root.event_id.into(),
            invite.event_id,
            invitee_id,
        );
        let acceptance = build_org_acceptance(&secret(1), &at, &accepted, T0).expect("builds");
        let removal = build_org_removal(&secret(1), &at, invitee_id, T0).expect("builds");

        for (event, name) in [
            (&invite, "OrgInvite"),
            (&acceptance, "OrgAcceptance"),
            (&removal, "OrgRemoval"),
        ] {
            assert_eq!(
                violation(&[&root, event]),
                Violation {
                    seq: 1,
                    reason: Reason::PayloadNotAllowed {
                        kind: LedgerKind::Person,
                        payload: name,
                    },
                },
                "{name} must not fold onto a person ledger"
            );
        }
    }

    #[test]
    fn an_attestation_duplicating_an_unrevoked_subject_is_rejected() {
        let root = inception();
        let first =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(9), T0).unwrap();
        let second =
            build_trust_attestation(&secret(1), &after(&root, &first, 2), subject(9), T0).unwrap();
        assert_eq!(
            violation(&[&root, &first, &second]),
            Violation {
                seq: 2,
                reason: Reason::DuplicateAttestation {
                    subject: subject(9),
                    attestation: first.event_id,
                },
            }
        );
    }

    #[test]
    fn revoking_an_unknown_attestation_is_rejected() {
        let root = inception();
        let unknown = EventId::from_bytes([0xcd; ID_BYTES]);
        let revoke =
            build_trust_revocation(&secret(1), &after(&root, &root, 1), unknown, T0).unwrap();
        assert_eq!(
            violation(&[&root, &revoke]),
            Violation {
                seq: 1,
                reason: Reason::UnknownRevocationTarget(unknown),
            }
        );
    }

    #[test]
    fn revoking_an_already_revoked_attestation_is_rejected() {
        let root = inception();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(9), T0).unwrap();
        let revoke =
            build_trust_revocation(&secret(1), &after(&root, &attest, 2), attest.event_id, T0)
                .unwrap();
        let again =
            build_trust_revocation(&secret(1), &after(&root, &revoke, 3), attest.event_id, T0)
                .unwrap();
        assert_eq!(
            violation(&[&root, &attest, &revoke, &again]),
            Violation {
                seq: 3,
                reason: Reason::AlreadyRevoked {
                    target: attest.event_id,
                    revoked_by: revoke.event_id,
                },
            }
        );
    }

    #[test]
    fn a_ledger_valid_to_n_folds_to_n_and_reports_the_failure_at_m() {
        let root = inception();
        let witnesses = build_witness_config(
            &secret(1),
            &after(&root, &root, 1),
            &[secret(4).public()],
            T0 + STEP,
        )
        .unwrap();
        let attest = build_trust_attestation(
            &secret(1),
            &after(&root, &witnesses, 2),
            subject(9),
            T0 + 2 * STEP,
        )
        .unwrap();
        // Seq 3 is signed by a key this ledger never authorized.
        let bad = build_trust_attestation(
            &secret(6),
            &after(&root, &attest, 3),
            subject(10),
            T0 + 3 * STEP,
        )
        .unwrap();
        // Seq 4 would be valid on its own; the fold never reaches it.
        let after_bad = build_trust_attestation(
            &secret(1),
            &after(&root, &bad, 4),
            subject(11),
            T0 + 4 * STEP,
        )
        .unwrap();

        let (state, violation) = fold(bytes(&[&root, &witnesses, &attest, &bad, &after_bad]));
        let violation = violation.expect("the fold reports a violation");
        assert_eq!(violation.seq, 3);
        assert_eq!(violation.code(), "unauthorized_signer");

        let head = state.head().expect("the valid prefix has a head");
        assert_eq!(head.seq, 2);
        assert_eq!(head.event_id, attest.event_id);
        assert_eq!(state.witnesses(), [secret(4).public()]);
        assert_eq!(state.trust().len(), 1);
        assert!(state.trusts(subject(9)));
        assert!(!state.trusts(subject(10)));
    }

    #[test]
    fn a_rejected_event_leaves_the_state_untouched() {
        let root = inception();
        let attest =
            build_trust_attestation(&secret(1), &after(&root, &root, 1), subject(9), T0).unwrap();
        let duplicate =
            build_trust_attestation(&secret(1), &after(&root, &attest, 2), subject(9), T0).unwrap();

        let (mut state, violation) = fold(bytes(&[&root, &attest]));
        assert_eq!(violation, None);
        let before = state.clone();
        assert!(state.apply(&duplicate.signed_event).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn an_org_inception_seeds_the_founder_as_a_controller() {
        let founder = inception();
        let org = build_org_inception(
            &secret(1),
            founder.event_id.into(),
            &founder.signed_event,
            [5u8; NONCE_BYTES],
            T0,
        )
        .expect("builds");

        let (state, violation) = fold(bytes(&[&org]));
        assert_eq!(violation, None);
        assert_eq!(state.kind(), Some(LedgerKind::Org));
        assert_eq!(state.ledger(), Some(org.event_id.into()));
        assert!(state.person().is_none());
        let org_state = state.org().expect("an org ledger has org state");
        assert_eq!(org_state.founder, founder.event_id.into());
        assert!(org_state.invites.is_empty());
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
    fn a_controller_may_attest_on_an_org_ledger() {
        let founder = inception();
        let org = build_org_inception(
            &secret(1),
            founder.event_id.into(),
            &founder.signed_event,
            [5u8; NONCE_BYTES],
            T0,
        )
        .expect("builds");
        let attest =
            build_trust_attestation(&secret(1), &after(&org, &org, 1), subject(9), T0).unwrap();

        let (state, violation) = fold(bytes(&[&org, &attest]));
        assert_eq!(violation, None);
        assert!(state.trusts(subject(9)));
    }

    #[test]
    fn org_membership_payloads_wait_for_ticket_005() {
        let founder = inception();
        let org = build_org_inception(
            &secret(1),
            founder.event_id.into(),
            &founder.signed_event,
            [5u8; NONCE_BYTES],
            T0,
        )
        .expect("builds");
        let removal = build_org_removal(
            &secret(1),
            &after(&org, &org, 1),
            founder.event_id.into(),
            T0,
        )
        .expect("builds");

        assert_eq!(
            violation(&[&org, &removal]),
            Violation {
                seq: 1,
                reason: Reason::OrgSemanticsPending {
                    payload: "OrgRemoval",
                },
            }
        );
    }

    #[test]
    fn malformed_bytes_are_reported_as_a_wire_violation() {
        let root = inception();
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
                    expected: EventId::from_bytes([1; ID_BYTES]),
                    found: EventId::from_bytes([2; ID_BYTES]),
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
                    kind: LedgerKind::Person,
                    payload: "OrgInvite",
                },
                "payload_not_allowed",
            ),
            (
                Reason::InvalidPublicKey {
                    field: "EventBody.author_key",
                },
                "invalid_public_key",
            ),
            (
                Reason::UnauthorizedSigner {
                    key: secret(1).public(),
                },
                "unauthorized_signer",
            ),
            (Reason::BadSignature, "bad_signature"),
            (
                Reason::DuplicateAttestation {
                    subject: subject(1),
                    attestation: EventId::from_bytes([2; ID_BYTES]),
                },
                "duplicate_attestation",
            ),
            (Reason::SelfAttestation(subject(1)), "self_attestation"),
            (
                Reason::UnknownRevocationTarget(EventId::from_bytes([1; ID_BYTES])),
                "unknown_revocation_target",
            ),
            (
                Reason::AlreadyRevoked {
                    target: EventId::from_bytes([1; ID_BYTES]),
                    revoked_by: EventId::from_bytes([2; ID_BYTES]),
                },
                "already_revoked",
            ),
            (
                Reason::OrgSemanticsPending {
                    payload: "OrgInvite",
                },
                "org_semantics_pending",
            ),
        ];
        for (reason, code) in cases {
            assert_eq!(reason.code(), code);
            let violation = Violation { seq: 3, reason };
            assert_eq!(violation.code(), code);
            assert!(violation.to_string().starts_with("seq 3: "));
        }
    }
}

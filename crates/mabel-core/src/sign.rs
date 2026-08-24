//! The signing path: the only code that produces event bytes.
//!
//! Each `build_*` function encodes an `EventBody` once, hashes and signs
//! those exact bytes, and returns them inside the encoded `SignedEvent`.
//! Callers store and ship what they get back; re-encoding a decoded event
//! invalidates its signature and changes its id (proposal 001 section 3.1,
//! pitfall 1).
//!
//! These functions check only what the byte layout forces: the size caps, the
//! timestamp bounds and the list bounds of the witness set and the endpoint
//! advertisement. Full field validation and the semantic rules belong to the
//! wire-format validator and the fold.
//!
//! `build_witness_config` is the one exception to "every payload has a builder
//! here": tag 11 is retired for writing, so it compiles only under the
//! `legacy-witness-config` feature and its only caller is the vector tests
//! (proposal 006 section 1).

use iroh_base::{EndpointId, PublicKey, SecretKey};
use mabel_proto::v0::{
    Acceptance, DeclaredKind, EndpointAdvertisement, EventBody, IdentityRoot, Inception,
    MembershipAcceptance, MembershipInvitation, MembershipRemoval, ProfileUpdate, RawRoot, Role,
    SignedEvent, TrustAttestation, TrustRevocation, WitnessSet, event_body::Payload, inception,
};

use crate::digest::{accept_input, event_id, reserve_commit, sign_input};
use crate::encoding::encode;
use crate::id::{EventId, IdentityId, LedgerId};
use crate::{
    MAX_ACCEPTANCE_BYTES, MAX_EMBEDDED_INCEPTION_BYTES, MAX_ENDPOINTS, MAX_EVENT_BYTES,
    MAX_TIMESTAMP_MS, MAX_WITNESSES, NONCE_BYTES,
};

/// A signed event and the bytes it is made of.
///
/// `body` is the exact byte string that was hashed and signed, and
/// `signed_event` embeds it verbatim. Store and transmit these bytes; do not
/// rebuild them from a decoded message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltEvent {
    /// `BLAKE3(EVENT_ID_DOMAIN || body)`.
    pub event_id: EventId,
    /// The encoded `EventBody`.
    pub body: Vec<u8>,
    /// The encoded `SignedEvent` carrying `body` and its signature.
    pub signed_event: Vec<u8>,
}

/// An invitee's detached acceptance, the blob a `MembershipAcceptance` embeds
/// verbatim (proposal 001 section 3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedAcceptance {
    /// The encoded `Acceptance`.
    pub acceptance: Vec<u8>,
    /// The invitee's signature over `accept_input(acceptance)`.
    pub signature: [u8; 64],
}

/// The one cryptographic root a ledger's inception carries (proposal 002
/// section 2).
///
/// The root is the only difference between what proposal 001 called a person
/// ledger and an organization ledger, and it is a fact about keys rather than
/// a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root<'a> {
    /// A self-keyed ledger. The signing key becomes a permanent `CONTROLLER`
    /// principal whose identity id is the ledger's own, and the event commits
    /// to `reserve_key` without recording it.
    Raw {
        /// The key committed to at inception and unused in this POC.
        reserve_key: &'a PublicKey,
    },
    /// A ledger whose first `CONTROLLER` is another identity. The signing key
    /// is that identity's active key.
    Identity {
        /// The founding identity's id.
        founder: IdentityId,
        /// The founder's seq-0 `SignedEvent` bytes, which this ledger embeds
        /// so membership needs no cross-ledger lookup.
        founder_inception: &'a [u8],
    },
}

/// Where a new event lands in an existing ledger.
///
/// `prev_timestamp_ms` is the previous event's `timestamp_ms`, which the new
/// event's timestamp may not fall below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The ledger's id, the `event_id` of its seq-0 event.
    pub ledger: LedgerId,
    /// This event's position, one past the previous event's.
    pub seq: u64,
    /// The previous event's id.
    pub prev: EventId,
    /// The previous event's `timestamp_ms`.
    pub prev_timestamp_ms: u64,
}

/// Why a caller's inputs cannot become an event.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    /// The timestamp fell outside `1..=4102444800000`.
    #[error("timestamp_ms {0} is outside 1..=4102444800000")]
    Timestamp(u64),
    /// A `Position` named seq 0, which only an inception occupies.
    #[error("seq 0 holds the inception; an append starts at seq 1")]
    AppendAtSeqZero,
    /// The encoded `SignedEvent` exceeded the 4096-byte cap.
    #[error("encoded SignedEvent is {0} bytes, over the 4096-byte cap")]
    EventTooLarge(usize),
    /// An embedded inception exceeded the 1024-byte cap.
    #[error("embedded inception is {0} bytes, over the 1024-byte cap")]
    InceptionTooLarge(usize),
    /// An acceptance blob exceeded the 1024-byte cap.
    #[error("acceptance is {0} bytes, over the 1024-byte cap")]
    AcceptanceTooLarge(usize),
    /// The witness set held more than 16 entries. The retired tag-11 list also
    /// reports this when it is empty, where it requires at least one.
    #[error("witness set holds {0} entries, over the 16-entry cap")]
    WitnessCount(usize),
    /// The witness set repeated an entry.
    #[error("witness set repeats an entry")]
    WitnessDuplicate,
    /// The advertisement held more than 8 endpoints.
    #[error("endpoint advertisement holds {0} endpoints, over the 8-endpoint cap")]
    EndpointCount(usize),
    /// The advertisement repeated an endpoint.
    #[error("endpoint advertisement repeats an endpoint")]
    EndpointDuplicate,
}

/// The timestamp an appender writes: `max(now_ms, prev.timestamp_ms)`.
///
/// Timestamps express ledger order, not wall time (proposal 001 section 3.2).
/// A caller whose clock lags the previous event reuses that event's timestamp
/// rather than producing an event no verifier would accept.
pub fn ledger_timestamp_ms(now_ms: u64, prev_timestamp_ms: u64) -> u64 {
    now_ms.max(prev_timestamp_ms)
}

/// Builds a ledger's seq-0 event around one root (proposal 002 section 2).
///
/// `signer` holds the root key: the ledger's own active key for
/// [`Root::Raw`], the founder's active key for [`Root::Identity`]. Either way
/// the seq-0 event is signed by the key its root records, which is what
/// self-authorizes it.
///
/// `kind` is advisory and must not be `KIND_UNSPECIFIED`; it gates nothing.
pub fn build_inception(
    signer: &SecretKey,
    kind: DeclaredKind,
    root: Root<'_>,
    nonce: [u8; NONCE_BYTES],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    let author_key = signer.public();
    let root = match root {
        Root::Raw { reserve_key } => inception::Root::RawRoot(RawRoot {
            active_key: author_key.as_bytes().to_vec(),
            reserve_commit: reserve_commit(reserve_key).to_vec(),
        }),
        Root::Identity {
            founder,
            founder_inception,
        } => {
            check_embedded_inception(founder_inception)?;
            inception::Root::IdentityRoot(IdentityRoot {
                founder: founder.to_vec(),
                founder_key: author_key.as_bytes().to_vec(),
                founder_inception: founder_inception.to_vec(),
            })
        }
    };
    let payload = Payload::Inception(Inception {
        kind: kind as i32,
        nonce: nonce.to_vec(),
        root: Some(root),
    });
    seal(signer, inception_body(&author_key, now_ms, payload)?)
}

/// Builds an event replacing the ledger's whole tag-11 endpoint list.
///
/// Retired for writing (proposal 006 section 1): no route, command or UI action
/// reaches this, and it is compiled only under the `legacy-witness-config`
/// feature, which the vector tests turn on. The golden and rejection vectors
/// for tag 11 are generated from it and must keep their exact bytes, which is
/// the whole reason it survives.
#[cfg(any(test, feature = "legacy-witness-config"))]
pub fn build_witness_config(
    signer: &SecretKey,
    at: &Position,
    witnesses: &[EndpointId],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    if witnesses.is_empty() || witnesses.len() > MAX_WITNESSES {
        return Err(BuildError::WitnessCount(witnesses.len()));
    }
    distinct(witnesses, BuildError::WitnessDuplicate)?;
    let payload = Payload::WitnessConfig(mabel_proto::v0::WitnessConfig {
        witnesses: witnesses.iter().map(|w| w.as_bytes().to_vec()).collect(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds an event replacing the whole set of identities that may keep this
/// ledger (proposal 006 section 1).
///
/// The set may be empty, which says nobody keeps this chain, and it may name
/// this ledger's own identity, which is how a self-hosted identity says it keeps
/// its own chain. Whether an id names a reachable identity is not a question
/// this crate can ask: it holds no network and no store.
pub fn build_witness_set(
    signer: &SecretKey,
    at: &Position,
    witnesses: &[IdentityId],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    if witnesses.len() > MAX_WITNESSES {
        return Err(BuildError::WitnessCount(witnesses.len()));
    }
    distinct(witnesses, BuildError::WitnessDuplicate)?;
    let payload = Payload::WitnessSet(WitnessSet {
        witnesses: witnesses.iter().map(IdentityId::to_vec).collect(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds an event replacing the whole list of endpoints that answer for this
/// identity (proposal 006 section 2).
///
/// Whole replacement, not append: one event says "these and only these", so a
/// rotation repeats the endpoint it keeps. An empty list is legal and says
/// nothing answers for this identity right now.
pub fn build_endpoint_advertisement(
    signer: &SecretKey,
    at: &Position,
    endpoints: &[EndpointId],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    if endpoints.len() > MAX_ENDPOINTS {
        return Err(BuildError::EndpointCount(endpoints.len()));
    }
    distinct(endpoints, BuildError::EndpointDuplicate)?;
    let payload = Payload::EndpointAdvertisement(EndpointAdvertisement {
        endpoints: endpoints.iter().map(|e| e.as_bytes().to_vec()).collect(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Refuses a repeated entry, which every list payload forbids.
fn distinct<T: PartialEq>(entries: &[T], error: BuildError) -> Result<(), BuildError> {
    for (index, entry) in entries.iter().enumerate() {
        if entries[index + 1..].contains(entry) {
            return Err(error);
        }
    }
    Ok(())
}

/// Builds an attestation that this ledger's identity trusts `subject`.
pub fn build_trust_attestation(
    signer: &SecretKey,
    at: &Position,
    subject: IdentityId,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    let payload = Payload::TrustAttestation(TrustAttestation {
        subject: subject.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds a revocation of an earlier attestation in this ledger, named by its
/// `event_id`.
pub fn build_trust_revocation(
    signer: &SecretKey,
    at: &Position,
    target: EventId,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    let payload = Payload::TrustRevocation(TrustRevocation {
        target: target.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds a membership invitation, embedding the invitee's seq-0
/// `SignedEvent` bytes.
///
/// Legal on every ledger: a raw-rooted ledger uses this to delegate signing
/// to a second controller (proposal 002 section 4).
pub fn build_membership_invitation(
    signer: &SecretKey,
    at: &Position,
    invitee: IdentityId,
    invitee_key: &PublicKey,
    role: Role,
    invitee_inception: &[u8],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    check_embedded_inception(invitee_inception)?;
    let payload = Payload::MembershipInvitation(MembershipInvitation {
        invitee: invitee.to_vec(),
        invitee_key: invitee_key.as_bytes().to_vec(),
        role: role as i32,
        invitee_inception: invitee_inception.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds the event that admits an invitee, embedding their detached
/// acceptance verbatim.
pub fn build_membership_acceptance(
    signer: &SecretKey,
    at: &Position,
    accepted: &DetachedAcceptance,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    if accepted.acceptance.len() > MAX_ACCEPTANCE_BYTES {
        return Err(BuildError::AcceptanceTooLarge(accepted.acceptance.len()));
    }
    let payload = Payload::MembershipAcceptance(MembershipAcceptance {
        acceptance: accepted.acceptance.clone(),
        signature: accepted.signature.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds an event removing an identity's membership and cancelling its open
/// invitation, whichever exist.
pub fn build_membership_removal(
    signer: &SecretKey,
    at: &Position,
    target: IdentityId,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    let payload = Payload::MembershipRemoval(MembershipRemoval {
        target: target.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds an event replacing the ledger's whole profile (proposal 003
/// section 1).
///
/// The operation is replacement, not patch: a `None` field clears that field,
/// and all three `None` encodes a zero-length payload that clears all three.
/// Any current `CONTROLLER` may append one.
///
/// The codepoint policy, the hostname syntax, the email rule and the byte caps
/// belong to the field table, which the validator runs over the encoded bytes;
/// refusing an update whose effect equals the folded profile is a node-side
/// guard (`no_op_profile_update`), never a rule of this crate.
pub fn build_profile_update(
    signer: &SecretKey,
    at: &Position,
    display_name: Option<&str>,
    hostname: Option<&str>,
    email: Option<&str>,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    build_append(
        signer,
        at,
        now_ms,
        Payload::ProfileUpdate(profile_update(display_name, hostname, email)),
    )
}

/// Runs the field table over the profile a `ProfileUpdate` would carry,
/// without building an event.
///
/// The scanner is the authority on what a published field may hold, and it
/// reads encoded bytes, so this encodes the payload alone and hands it the same
/// descriptor. A caller that mints a ledger and then appends the profile needs
/// the refusal before the mint: `mabel identity create --email <not an email>`
/// must leave no ledger and no taken alias behind.
///
/// # Errors
///
/// Returns the [`crate::validate::WireError`] the scanner produces for the
/// offending field, which is the reason every other surface reports.
pub fn check_profile(
    display_name: Option<&str>,
    hostname: Option<&str>,
    email: Option<&str>,
) -> Result<(), crate::validate::WireError> {
    let bytes = encode(&profile_update(display_name, hostname, email));
    crate::validate::message(&crate::validate::PROFILE_UPDATE, &bytes)
}

fn profile_update(
    display_name: Option<&str>,
    hostname: Option<&str>,
    email: Option<&str>,
) -> ProfileUpdate {
    ProfileUpdate {
        display_name: display_name.unwrap_or_default().to_owned(),
        hostname: hostname.unwrap_or_default().to_owned(),
        email: email.unwrap_or_default().to_owned(),
    }
}

/// Builds and signs an invitee's detached acceptance of an invitation.
///
/// The invitee cannot append to the inviting ledger, so this blob and its
/// signature travel back to a controller, who embeds them in a
/// `MembershipAcceptance` (proposal 001 section 3.5).
pub fn build_acceptance(
    invitee_active: &SecretKey,
    ledger: LedgerId,
    invitation_event: EventId,
    invitee: IdentityId,
) -> DetachedAcceptance {
    let acceptance = encode(&Acceptance {
        version: 0,
        ledger: ledger.to_vec(),
        invitation_event: invitation_event.to_vec(),
        invitee: invitee.to_vec(),
        invitee_key: invitee_active.public().as_bytes().to_vec(),
    });
    let signature = invitee_active.sign(&accept_input(&acceptance)).to_bytes();
    DetachedAcceptance {
        acceptance,
        signature,
    }
}

fn inception_body(
    author_key: &PublicKey,
    now_ms: u64,
    payload: Payload,
) -> Result<EventBody, BuildError> {
    check_timestamp(now_ms)?;
    Ok(EventBody {
        version: 0,
        ledger: Vec::new(),
        seq: 0,
        prev: Vec::new(),
        timestamp_ms: now_ms,
        author_key: author_key.as_bytes().to_vec(),
        payload: Some(payload),
    })
}

fn build_append(
    signer: &SecretKey,
    at: &Position,
    now_ms: u64,
    payload: Payload,
) -> Result<BuiltEvent, BuildError> {
    if at.seq == 0 {
        return Err(BuildError::AppendAtSeqZero);
    }
    let timestamp_ms = ledger_timestamp_ms(now_ms, at.prev_timestamp_ms);
    check_timestamp(timestamp_ms)?;
    seal(
        signer,
        EventBody {
            version: 0,
            ledger: at.ledger.to_vec(),
            seq: at.seq,
            prev: at.prev.to_vec(),
            timestamp_ms,
            author_key: signer.public().as_bytes().to_vec(),
            payload: Some(payload),
        },
    )
}

fn seal(signer: &SecretKey, body: EventBody) -> Result<BuiltEvent, BuildError> {
    let body = encode(&body);
    let signature = signer.sign(&sign_input(&body));
    let signed_event = encode(&SignedEvent {
        body: body.clone(),
        signature: signature.to_bytes().to_vec(),
    });
    if signed_event.len() > MAX_EVENT_BYTES {
        return Err(BuildError::EventTooLarge(signed_event.len()));
    }
    Ok(BuiltEvent {
        event_id: event_id(&body),
        body,
        signed_event,
    })
}

fn check_timestamp(timestamp_ms: u64) -> Result<(), BuildError> {
    if timestamp_ms == 0 || timestamp_ms > MAX_TIMESTAMP_MS {
        return Err(BuildError::Timestamp(timestamp_ms));
    }
    Ok(())
}

fn check_embedded_inception(bytes: &[u8]) -> Result<(), BuildError> {
    if bytes.len() > MAX_EMBEDDED_INCEPTION_BYTES {
        return Err(BuildError::InceptionTooLarge(bytes.len()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mabel_proto::prost::Message;

    fn secret(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    /// A raw-rooted ledger, the shape proposal 001 called a person.
    fn raw_rooted(now_ms: u64) -> BuiltEvent {
        build_inception(
            &secret(1),
            DeclaredKind::Person,
            Root::Raw {
                reserve_key: &secret(2).public(),
            },
            [3u8; 16],
            now_ms,
        )
        .expect("builds an inception")
    }

    fn position(after: &BuiltEvent, seq: u64, prev_timestamp_ms: u64) -> Position {
        Position {
            ledger: after.event_id.into(),
            seq,
            prev: after.event_id,
            prev_timestamp_ms,
        }
    }

    #[test]
    fn signed_event_carries_the_signed_bytes_and_verifies() {
        let built = raw_rooted(1_700_000_000_000);
        let decoded = SignedEvent::decode(&built.signed_event[..]).expect("decodes");
        assert_eq!(decoded.body, built.body);
        assert_eq!(built.event_id, event_id(&built.body));

        let signature: [u8; 64] = decoded.signature.try_into().expect("64-byte signature");
        let signature = iroh_base::Signature::from_bytes(&signature);
        secret(1)
            .public()
            .verify(&sign_input(&built.body), &signature)
            .expect("signature verifies over the body bytes");
    }

    #[test]
    fn inception_omits_ledger_and_prev() {
        let built = raw_rooted(1_700_000_000_000);
        let body = EventBody::decode(&built.body[..]).expect("decodes");
        assert!(body.ledger.is_empty());
        assert!(body.prev.is_empty());
        assert_eq!(body.seq, 0);
        assert_eq!(body.version, 0);
        // proto3 defaults are absent, so the encoded body starts at the
        // timestamp field (tag 5, wire type 0).
        assert_eq!(built.body[0], 0x28);
    }

    #[test]
    fn a_raw_root_commits_to_the_reserve_key_without_recording_it() {
        let reserve = secret(2).public();
        let built = raw_rooted(1_700_000_000_000);
        let body = EventBody::decode(&built.body[..]).expect("decodes");
        let Some(Payload::Inception(inception)) = body.payload else {
            panic!("expected an Inception payload");
        };
        assert_eq!(inception.kind, DeclaredKind::Person as i32);
        let Some(inception::Root::RawRoot(root)) = inception.root else {
            panic!("expected a raw root");
        };
        assert_eq!(root.active_key, secret(1).public().as_bytes().to_vec());
        assert_eq!(root.reserve_commit, reserve_commit(&reserve).to_vec());
        assert_ne!(root.reserve_commit, reserve.as_bytes().to_vec());
    }

    #[test]
    fn an_identity_root_records_the_founder_and_embeds_their_inception() {
        let founder = raw_rooted(1_700_000_000_000);
        let built = build_inception(
            &secret(1),
            DeclaredKind::Organization,
            Root::Identity {
                founder: founder.event_id.into(),
                founder_inception: &founder.signed_event,
            },
            [5u8; 16],
            1_700_000_000_000,
        )
        .expect("builds");
        let body = EventBody::decode(&built.body[..]).expect("decodes");
        let Some(Payload::Inception(inception)) = body.payload else {
            panic!("expected an Inception payload");
        };
        assert_eq!(inception.kind, DeclaredKind::Organization as i32);
        let Some(inception::Root::IdentityRoot(root)) = inception.root else {
            panic!("expected an identity root");
        };
        assert_eq!(root.founder, founder.event_id.to_vec());
        assert_eq!(root.founder_key, secret(1).public().as_bytes().to_vec());
        assert_eq!(root.founder_inception, founder.signed_event);
    }

    /// Two ledgers with the same declared kind and the same founder differ by
    /// nonce alone, which is what pitfall 6 asks of the id derivation.
    #[test]
    fn declared_kind_is_the_only_free_label_and_changes_the_id() {
        let a = build_inception(
            &secret(1),
            DeclaredKind::Agent,
            Root::Raw {
                reserve_key: &secret(2).public(),
            },
            [3u8; 16],
            1_700_000_000_000,
        )
        .expect("builds");
        let b = build_inception(
            &secret(1),
            DeclaredKind::Service,
            Root::Raw {
                reserve_key: &secret(2).public(),
            },
            [3u8; 16],
            1_700_000_000_000,
        )
        .expect("builds");
        assert_ne!(a.event_id, b.event_id);
    }

    #[test]
    fn a_lagging_clock_reuses_the_previous_timestamp() {
        assert_eq!(ledger_timestamp_ms(5, 9), 9);
        assert_eq!(ledger_timestamp_ms(9, 5), 9);
        assert_eq!(ledger_timestamp_ms(7, 7), 7);

        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 1, 1_700_000_000_000);
        let lagging = build_witness_config(&secret(1), &at, &[secret(4).public()], 1_000)
            .expect("builds despite the lagging clock");
        let body = EventBody::decode(&lagging.body[..]).expect("decodes");
        assert_eq!(body.timestamp_ms, 1_700_000_000_000);

        let ahead = build_witness_config(&secret(1), &at, &[secret(4).public()], 1_700_000_001_000)
            .expect("builds");
        let body = EventBody::decode(&ahead.body[..]).expect("decodes");
        assert_eq!(body.timestamp_ms, 1_700_000_001_000);
    }

    #[test]
    fn timestamps_outside_the_bounds_are_refused() {
        let reserve_key = secret(2).public();
        let raw = || Root::Raw {
            reserve_key: &reserve_key,
        };
        assert_eq!(
            build_inception(&secret(1), DeclaredKind::Person, raw(), [3u8; 16], 0),
            Err(BuildError::Timestamp(0))
        );
        assert_eq!(
            build_inception(
                &secret(1),
                DeclaredKind::Person,
                raw(),
                [3u8; 16],
                MAX_TIMESTAMP_MS + 1
            ),
            Err(BuildError::Timestamp(MAX_TIMESTAMP_MS + 1))
        );

        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 1, MAX_TIMESTAMP_MS + 1);
        assert_eq!(
            build_membership_removal(&secret(1), &at, IdentityId::from_bytes([8u8; 32]), 1_000),
            Err(BuildError::Timestamp(MAX_TIMESTAMP_MS + 1))
        );
    }

    #[test]
    fn an_append_cannot_take_seq_zero() {
        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 0, 1_700_000_000_000);
        assert_eq!(
            build_trust_attestation(&secret(1), &at, IdentityId::from_bytes([8u8; 32]), 1),
            Err(BuildError::AppendAtSeqZero)
        );
    }

    #[test]
    fn witness_sets_are_bounded_and_distinct() {
        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 1, 1_700_000_000_000);
        assert_eq!(
            build_witness_config(&secret(1), &at, &[], 1_700_000_000_000),
            Err(BuildError::WitnessCount(0))
        );

        let many: Vec<EndpointId> = (0..17u8).map(|i| secret(100 + i).public()).collect();
        assert_eq!(
            build_witness_config(&secret(1), &at, &many, 1_700_000_000_000),
            Err(BuildError::WitnessCount(17))
        );

        let repeated = [secret(4).public(), secret(4).public()];
        assert_eq!(
            build_witness_config(&secret(1), &at, &repeated, 1_700_000_000_000),
            Err(BuildError::WitnessDuplicate)
        );
    }

    /// Tag 19 takes an empty set where tag 11 required one entry, and refuses a
    /// seventeenth entry and a repeat (proposal 006 sections 1 and 3).
    #[test]
    fn a_witness_set_takes_none_and_stops_at_sixteen() {
        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 1, 1_700_000_000_000);
        build_witness_set(&secret(1), &at, &[], 1_700_000_000_000)
            .expect("an empty witness set builds");

        let many: Vec<IdentityId> = (0..17u8)
            .map(|seed| IdentityId::from_bytes([seed; 32]))
            .collect();
        assert_eq!(
            build_witness_set(&secret(1), &at, &many, 1_700_000_000_000),
            Err(BuildError::WitnessCount(17))
        );

        let witness = IdentityId::from_bytes([7u8; 32]);
        assert_eq!(
            build_witness_set(&secret(1), &at, &[witness, witness], 1_700_000_000_000),
            Err(BuildError::WitnessDuplicate)
        );
    }

    /// Tag 18 takes an empty list and stops at eight (proposal 006 section 2).
    #[test]
    fn an_advertisement_takes_none_and_stops_at_eight() {
        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 1, 1_700_000_000_000);
        build_endpoint_advertisement(&secret(1), &at, &[], 1_700_000_000_000)
            .expect("an empty advertisement builds");

        let many: Vec<EndpointId> = (0..9u8).map(|i| secret(100 + i).public()).collect();
        assert_eq!(
            build_endpoint_advertisement(&secret(1), &at, &many, 1_700_000_000_000),
            Err(BuildError::EndpointCount(9))
        );

        let repeated = [secret(4).public(), secret(4).public()];
        assert_eq!(
            build_endpoint_advertisement(&secret(1), &at, &repeated, 1_700_000_000_000),
            Err(BuildError::EndpointDuplicate)
        );
    }

    #[test]
    fn oversize_embedded_bytes_are_refused() {
        let head = raw_rooted(1_700_000_000_000);
        let at = position(&head, 1, 1_700_000_000_000);
        let big = vec![0u8; MAX_EMBEDDED_INCEPTION_BYTES + 1];
        assert_eq!(
            build_inception(
                &secret(1),
                DeclaredKind::Organization,
                Root::Identity {
                    founder: IdentityId::from_bytes([8u8; 32]),
                    founder_inception: &big,
                },
                [3u8; 16],
                1_700_000_000_000
            ),
            Err(BuildError::InceptionTooLarge(big.len()))
        );
        assert_eq!(
            build_membership_invitation(
                &secret(1),
                &at,
                IdentityId::from_bytes([8u8; 32]),
                &secret(2).public(),
                Role::Member,
                &big,
                1_700_000_000_000
            ),
            Err(BuildError::InceptionTooLarge(big.len()))
        );

        let accepted = DetachedAcceptance {
            acceptance: vec![0u8; MAX_ACCEPTANCE_BYTES + 1],
            signature: [0u8; 64],
        };
        assert_eq!(
            build_membership_acceptance(&secret(1), &at, &accepted, 1_700_000_000_000),
            Err(BuildError::AcceptanceTooLarge(MAX_ACCEPTANCE_BYTES + 1))
        );
    }

    #[test]
    fn an_acceptance_is_signed_over_its_own_bytes() {
        let invitee = secret(5);
        let accepted = build_acceptance(
            &invitee,
            LedgerId::from_bytes([1u8; 32]),
            EventId::from_bytes([2u8; 32]),
            IdentityId::from_bytes([3u8; 32]),
        );
        let signature = iroh_base::Signature::from_bytes(&accepted.signature);
        invitee
            .public()
            .verify(&accept_input(&accepted.acceptance), &signature)
            .expect("acceptance signature verifies");

        let decoded = Acceptance::decode(&accepted.acceptance[..]).expect("decodes");
        assert_eq!(decoded.ledger, vec![1u8; 32]);
        assert_eq!(decoded.invitation_event, vec![2u8; 32]);
        assert_eq!(decoded.invitee_key, invitee.public().as_bytes().to_vec());
        assert_eq!(decoded.version, 0);
    }
}

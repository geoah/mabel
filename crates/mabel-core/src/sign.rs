//! The signing path: the only code that produces event bytes.
//!
//! Each `build_*` function encodes an `EventBody` once, hashes and signs
//! those exact bytes, and returns them inside the encoded `SignedEvent`.
//! Callers store and ship what they get back; re-encoding a decoded event
//! invalidates its signature and changes its id (proposal 001 section 3.1,
//! pitfall 1).
//!
//! These functions check only what the byte layout forces: the size caps, the
//! timestamp bounds and the witness-set bounds. Full field validation and the
//! semantic rules belong to the wire-format validator and the fold.

use iroh_base::{EndpointId, PublicKey, SecretKey};
use mabel_proto::v0::{
    Acceptance, EventBody, IdentityKind, OrgAcceptance, OrgInception, OrgInvite, OrgRemoval,
    PersonInception, Role, SignedEvent, TrustAttestation, TrustRevocation, WitnessConfig,
    event_body::Payload,
};

use crate::digest::{accept_input, event_id, reserve_commit, sign_input};
use crate::encoding::encode;
use crate::id::{EventId, IdentityId, LedgerId};
use crate::{
    MAX_ACCEPTANCE_BYTES, MAX_EMBEDDED_INCEPTION_BYTES, MAX_EVENT_BYTES, MAX_TIMESTAMP_MS,
    MAX_WITNESSES, NONCE_BYTES,
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

/// An invitee's detached acceptance, the blob an `OrgAcceptance` embeds
/// verbatim (proposal 001 section 3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedAcceptance {
    /// The encoded `Acceptance`.
    pub acceptance: Vec<u8>,
    /// The invitee's signature over `accept_input(acceptance)`.
    pub sig: [u8; 64],
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
    /// The witness set was empty or held more than 16 endpoints.
    #[error("witness set holds {0} endpoints, outside 1..=16")]
    WitnessCount(usize),
    /// The witness set repeated an endpoint.
    #[error("witness set repeats an endpoint")]
    WitnessDuplicate,
}

/// The timestamp an appender writes: `max(now_ms, prev.timestamp_ms)`.
///
/// Timestamps express ledger order, not wall time (proposal 001 section 3.2).
/// A caller whose clock lags the previous event reuses that event's timestamp
/// rather than producing an event no verifier would accept.
pub fn ledger_timestamp_ms(now_ms: u64, prev_timestamp_ms: u64) -> u64 {
    now_ms.max(prev_timestamp_ms)
}

/// Builds a person's seq-0 event, self-signed by its active key.
///
/// The reserve key is committed to, never recorded: the event carries
/// `reserve_commit(reserve_key)`.
pub fn build_person_inception(
    active: &SecretKey,
    reserve_key: &PublicKey,
    nonce: [u8; NONCE_BYTES],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    let author_key = active.public();
    let payload = Payload::PersonInception(PersonInception {
        kind: IdentityKind::Person as i32,
        active_key: author_key.as_bytes().to_vec(),
        reserve_commit: reserve_commit(reserve_key).to_vec(),
        nonce: nonce.to_vec(),
    });
    seal(active, inception_body(&author_key, now_ms, payload)?)
}

/// Builds an org's seq-0 event, signed by the founder's personal active key.
///
/// `founder_inception` is the founder's own seq-0 `SignedEvent` bytes, which
/// the org ledger embeds so membership needs no cross-ledger lookup
/// (proposal 001 section 3.4).
pub fn build_org_inception(
    founder_active: &SecretKey,
    founder: IdentityId,
    founder_inception: &[u8],
    nonce: [u8; NONCE_BYTES],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    check_embedded_inception(founder_inception)?;
    let author_key = founder_active.public();
    let payload = Payload::OrgInception(OrgInception {
        kind: IdentityKind::Org as i32,
        founder: founder.to_vec(),
        founder_key: author_key.as_bytes().to_vec(),
        founder_inception: founder_inception.to_vec(),
        nonce: nonce.to_vec(),
    });
    seal(
        founder_active,
        inception_body(&author_key, now_ms, payload)?,
    )
}

/// Builds an event replacing the ledger's whole witness set.
pub fn build_witness_config(
    signer: &SecretKey,
    at: &Position,
    witnesses: &[EndpointId],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    if witnesses.is_empty() || witnesses.len() > MAX_WITNESSES {
        return Err(BuildError::WitnessCount(witnesses.len()));
    }
    let mut seen: Vec<&EndpointId> = Vec::with_capacity(witnesses.len());
    for witness in witnesses {
        if seen.contains(&witness) {
            return Err(BuildError::WitnessDuplicate);
        }
        seen.push(witness);
    }
    let payload = Payload::WitnessConfig(WitnessConfig {
        witnesses: witnesses.iter().map(|w| w.as_bytes().to_vec()).collect(),
    });
    build_append(signer, at, now_ms, payload)
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

/// Builds an org invitation, embedding the invitee's seq-0 `SignedEvent`
/// bytes.
pub fn build_org_invite(
    signer: &SecretKey,
    at: &Position,
    invitee: IdentityId,
    invitee_key: &PublicKey,
    role: Role,
    invitee_inception: &[u8],
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    check_embedded_inception(invitee_inception)?;
    let payload = Payload::OrgInvite(OrgInvite {
        invitee: invitee.to_vec(),
        invitee_key: invitee_key.as_bytes().to_vec(),
        role: role as i32,
        invitee_inception: invitee_inception.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds the org event that admits an invitee, embedding their detached
/// acceptance verbatim.
pub fn build_org_acceptance(
    signer: &SecretKey,
    at: &Position,
    accepted: &DetachedAcceptance,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    if accepted.acceptance.len() > MAX_ACCEPTANCE_BYTES {
        return Err(BuildError::AcceptanceTooLarge(accepted.acceptance.len()));
    }
    let payload = Payload::OrgAcceptance(OrgAcceptance {
        acceptance: accepted.acceptance.clone(),
        sig: accepted.sig.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds an event removing an identity's membership and cancelling its open
/// invite, whichever exist.
pub fn build_org_removal(
    signer: &SecretKey,
    at: &Position,
    target: IdentityId,
    now_ms: u64,
) -> Result<BuiltEvent, BuildError> {
    let payload = Payload::OrgRemoval(OrgRemoval {
        target: target.to_vec(),
    });
    build_append(signer, at, now_ms, payload)
}

/// Builds and signs an invitee's detached acceptance of an org invitation.
///
/// The invitee holds no org ledger and cannot append to it, so this blob and
/// its signature travel back to a controller, who embeds them in an
/// `OrgAcceptance` (proposal 001 section 3.5).
pub fn build_acceptance(
    invitee_active: &SecretKey,
    org: LedgerId,
    invite_event: EventId,
    invitee: IdentityId,
) -> DetachedAcceptance {
    let acceptance = encode(&Acceptance {
        version: 0,
        org: org.to_vec(),
        invite_event: invite_event.to_vec(),
        invitee: invitee.to_vec(),
        invitee_key: invitee_active.public().as_bytes().to_vec(),
    });
    let sig = invitee_active.sign(&accept_input(&acceptance)).to_bytes();
    DetachedAcceptance { acceptance, sig }
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
    let sig = signer.sign(&sign_input(&body));
    let signed_event = encode(&SignedEvent {
        body: body.clone(),
        sig: sig.to_bytes().to_vec(),
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

    fn person(now_ms: u64) -> BuiltEvent {
        build_person_inception(&secret(1), &secret(2).public(), [3u8; 16], now_ms)
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
        let built = person(1_700_000_000_000);
        let decoded = SignedEvent::decode(&built.signed_event[..]).expect("decodes");
        assert_eq!(decoded.body, built.body);
        assert_eq!(built.event_id, event_id(&built.body));

        let sig_bytes: [u8; 64] = decoded.sig.try_into().expect("64-byte signature");
        let sig = iroh_base::Signature::from_bytes(&sig_bytes);
        secret(1)
            .public()
            .verify(&sign_input(&built.body), &sig)
            .expect("signature verifies over the body bytes");
    }

    #[test]
    fn inception_omits_ledger_and_prev() {
        let built = person(1_700_000_000_000);
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
    fn inception_commits_to_the_reserve_key_without_recording_it() {
        let reserve = secret(2).public();
        let built = person(1_700_000_000_000);
        let body = EventBody::decode(&built.body[..]).expect("decodes");
        let Some(Payload::PersonInception(inception)) = body.payload else {
            panic!("expected a PersonInception payload");
        };
        assert_eq!(inception.reserve_commit, reserve_commit(&reserve).to_vec());
        assert_ne!(inception.reserve_commit, reserve.as_bytes().to_vec());
        assert_eq!(inception.kind, IdentityKind::Person as i32);
    }

    #[test]
    fn a_lagging_clock_reuses_the_previous_timestamp() {
        assert_eq!(ledger_timestamp_ms(5, 9), 9);
        assert_eq!(ledger_timestamp_ms(9, 5), 9);
        assert_eq!(ledger_timestamp_ms(7, 7), 7);

        let head = person(1_700_000_000_000);
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
        assert_eq!(
            build_person_inception(&secret(1), &secret(2).public(), [3u8; 16], 0),
            Err(BuildError::Timestamp(0))
        );
        assert_eq!(
            build_person_inception(
                &secret(1),
                &secret(2).public(),
                [3u8; 16],
                MAX_TIMESTAMP_MS + 1
            ),
            Err(BuildError::Timestamp(MAX_TIMESTAMP_MS + 1))
        );

        let head = person(1_700_000_000_000);
        let at = position(&head, 1, MAX_TIMESTAMP_MS + 1);
        assert_eq!(
            build_org_removal(&secret(1), &at, IdentityId::from_bytes([8u8; 32]), 1_000),
            Err(BuildError::Timestamp(MAX_TIMESTAMP_MS + 1))
        );
    }

    #[test]
    fn an_append_cannot_take_seq_zero() {
        let head = person(1_700_000_000_000);
        let at = position(&head, 0, 1_700_000_000_000);
        assert_eq!(
            build_trust_attestation(&secret(1), &at, IdentityId::from_bytes([8u8; 32]), 1),
            Err(BuildError::AppendAtSeqZero)
        );
    }

    #[test]
    fn witness_sets_are_bounded_and_distinct() {
        let head = person(1_700_000_000_000);
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

    #[test]
    fn oversize_embedded_bytes_are_refused() {
        let head = person(1_700_000_000_000);
        let at = position(&head, 1, 1_700_000_000_000);
        let big = vec![0u8; MAX_EMBEDDED_INCEPTION_BYTES + 1];
        assert_eq!(
            build_org_inception(
                &secret(1),
                IdentityId::from_bytes([8u8; 32]),
                &big,
                [3u8; 16],
                1_700_000_000_000
            ),
            Err(BuildError::InceptionTooLarge(big.len()))
        );
        assert_eq!(
            build_org_invite(
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
            sig: [0u8; 64],
        };
        assert_eq!(
            build_org_acceptance(&secret(1), &at, &accepted, 1_700_000_000_000),
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
        let sig = iroh_base::Signature::from_bytes(&accepted.sig);
        invitee
            .public()
            .verify(&accept_input(&accepted.acceptance), &sig)
            .expect("acceptance signature verifies");

        let decoded = Acceptance::decode(&accepted.acceptance[..]).expect("decodes");
        assert_eq!(decoded.invitee_key, invitee.public().as_bytes().to_vec());
        assert_eq!(decoded.version, 0);
    }
}

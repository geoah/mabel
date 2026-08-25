//! Turning the failures a wallet meets into the one error envelope.
//!
//! [`ServiceError`] is the envelope both surfaces render: the HTTP API returns
//! it directly and the CLI copies its code, message and details (proposal 001
//! section 9, `contracts/README.md`). Everything a wallet can fail at is
//! mapped here, so no caller invents a second spelling.

use axum::http::StatusCode;
use iroh_base::EndpointId;
use mabel_core::LedgerId;
use mabel_core::artifacts::ArtifactError;
use mabel_core::fold::Reason;
use mabel_core::sign::BuildError;
use mabel_net::Error as NetError;
use mabel_net::store::Head;

use crate::api::documents::Id;
use crate::api::error::ServiceError;
use crate::error::StorageError;
use crate::wallet::ids;

/// A storage failure, carrying the code `mabel-node` assigned it.
#[must_use]
pub fn storage_error(error: StorageError) -> ServiceError {
    let message = error.to_string();
    match &error {
        StorageError::InsecurePermissions { path, mode } => ServiceError::permissions(
            "insecure_key_permissions",
            format!(
                "key file has insecure permissions: {} is mode {mode:04o}, \
                 pass --allow-insecure-permissions to continue",
                path.display()
            ),
        )
        .with_detail("path", path.display().to_string())
        .with_detail("mode", format!("{mode:04o}"))
        .with_detail("expected_mode", "0600"),
        StorageError::HomeUnknown | StorageError::NotAHome { .. } => {
            ServiceError::usage("no_node_home", message)
        }
        StorageError::Json { .. }
        | StorageError::MalformedKey { .. }
        | StorageError::MalformedEvent { .. } => ServiceError::schema("malformed_file", message),
        StorageError::UnknownIdentity { identity } => {
            ServiceError::usage("unknown_identity", message)
                .with_detail("identity", identity.to_string())
                .with_status(StatusCode::NOT_FOUND)
        }
        StorageError::MissingEvent { .. } | StorageError::EventIdMismatch { .. } => {
            ServiceError::ledger("missing_event", message)
        }
        StorageError::OutOfOrderAppend { .. } => {
            ServiceError::state("out_of_order_append", message)
        }
        _ => ServiceError::state("storage_unavailable", message)
            .with_status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// The sentence a person reads for a rejection, with every identity id it
/// names carrying the `mabel://` prefix (decision 019).
///
/// [`Reason`]'s own `Display` spells its ids bare and has to keep doing so:
/// `test-vectors/rejections/` pins those strings as wire evidence another
/// implementation compares its rejections against, and a display prefix has no
/// place in a conformance vector. The prefix is added here instead, at the one
/// point both the HTTP envelope and the CLI envelope pass through, so neither
/// surface spells a rejection differently from the other.
///
/// Every variant is listed, including the ones that name no identity. [`Reason`]
/// is `#[non_exhaustive]`, so a wildcard is still required and the compiler
/// cannot force the next person to choose; `every_identity_bearing_reason_is_
/// prefixed` below is what catches a new id-bearing variant falling through it.
#[must_use]
pub fn fold_message(reason: &Reason) -> String {
    let prefix = mabel_core::LINK_PREFIX;
    match reason {
        Reason::DuplicateAttestation {
            subject,
            attestation,
        } => format!("{prefix}{subject} already has an unrevoked attestation, {attestation}"),
        Reason::DuplicateInvitation {
            invitee,
            invitation,
        } => format!("{prefix}{invitee} already has an open invitation, {invitation}"),
        Reason::PrincipalKeyMismatch {
            identity,
            expected,
            found,
        } => format!("{prefix}{identity} is a principal with key {expected}, not {found}"),
        Reason::DuplicatePrincipalKey { key, held_by } => {
            format!("key {key} is already held by principal {prefix}{held_by}")
        }
        Reason::RootNotRemovable(identity) => {
            format!("{prefix}{identity} is this ledger's raw root and is not removable")
        }
        Reason::LastController(identity) => {
            format!("removing {prefix}{identity} would leave this ledger with no controller")
        }
        Reason::RootNotDemotable(identity) => {
            format!("{prefix}{identity} is this ledger's raw root and is not demotable")
        }
        Reason::DemotesLastController(identity) => {
            format!("demoting {prefix}{identity} would leave this ledger with no controller")
        }
        // These name a protobuf field and then the identity it holds. The field
        // name is machine vocabulary and stays as it is; the id beside it is
        // still an identity being put in front of a reader, so it carries the
        // prefix like every other.
        Reason::WrongLedger { expected, found } => {
            format!("EventBody.ledger names {prefix}{found}, not this ledger {prefix}{expected}")
        }
        Reason::SelfAttestation(identity) => {
            format!("TrustAttestation.subject is this ledger's own id {prefix}{identity}")
        }
        Reason::AcceptanceForAnotherLedger { named, expected } => {
            format!("Acceptance.ledger names {prefix}{named}, not this ledger {prefix}{expected}")
        }
        Reason::AcceptanceInviteeMismatch { named, invited } => format!(
            "Acceptance.invitee names {prefix}{named}, but the invitation invited {prefix}{invited}"
        ),
        Reason::UnknownRemovalTarget(identity) => format!(
            "MembershipRemoval.target {prefix}{identity} is neither a principal nor an open invitee"
        ),
        // Everything left names no identity: sequence numbers, timestamps, event
        // ids, public keys and field names. The fold's own wording is already
        // right for a reader, so it is what both surfaces show.
        Reason::Wire(_)
        | Reason::WrongSeq { .. }
        | Reason::BrokenPrevLink { .. }
        | Reason::BackwardsTimestamp { .. }
        | Reason::PayloadNotAllowed { .. }
        | Reason::InvalidPublicKey { .. }
        | Reason::UnauthorizedSigner { .. }
        | Reason::BadSignature
        | Reason::UnknownRevocationTarget(_)
        | Reason::AlreadyRevoked { .. }
        | Reason::UnknownInvitation(_)
        | Reason::InvitationNotOpen { .. }
        | Reason::AcceptanceInviteeKeyMismatch { .. } => reason.to_string(),
        // Forced by `#[non_exhaustive]`. A variant added upstream lands here
        // and reads as the fold spells it, which is right for one naming no
        // identity and wrong for one that does; the test below is the guard.
        _ => reason.to_string(),
    }
}

/// A rejection from the fold, which is the authority on why an event is not
/// allowed.
///
/// See [`fold_error_at`] for the one reason both surfaces respell.
#[must_use]
pub fn fold_error(reason: &Reason) -> ServiceError {
    fold_error_at(reason, None)
}

/// [`fold_error`] for a caller that folded the ledger and knows where the
/// attestation a duplicate collides with sits.
///
/// The fold names the standing attestation by event id and not by position, so
/// a duplicate is respelled here: reason `duplicate_unrevoked_attestation` and
/// the message `contracts/cli/errors.json` and
/// `contracts/http/wallet-post-trust.json` both pin. `standing_seq` is `None`
/// for a caller holding no fold of the ledger, which reads as seq 0, the same
/// fallback `mabel-cli` uses.
#[must_use]
pub fn fold_error_at(reason: &Reason, standing_seq: Option<u64>) -> ServiceError {
    if let Reason::DuplicateAttestation {
        subject,
        attestation,
    } = reason
    {
        let at = standing_seq.unwrap_or_default();
        return ServiceError::policy(
            "duplicate_unrevoked_attestation",
            format!(
                "an unrevoked attestation for {}{subject} already exists at seq {at}",
                mabel_core::LINK_PREFIX
            ),
        )
        .with_detail("subject", subject.to_string())
        .with_detail("attestation_event", attestation.to_string())
        .with_detail("at_seq", at);
    }
    let message = fold_message(reason);
    match reason {
        Reason::Wire(_) => ServiceError::schema(reason.code(), message),
        Reason::WrongSeq { .. }
        | Reason::WrongLedger { .. }
        | Reason::BrokenPrevLink { .. }
        | Reason::BackwardsTimestamp { .. }
        | Reason::PayloadNotAllowed { .. }
        | Reason::InvalidPublicKey { .. }
        | Reason::UnauthorizedSigner { .. }
        | Reason::BadSignature => ServiceError::ledger(reason.code(), message),
        _ => ServiceError::policy(reason.code(), message),
    }
}

/// A file artifact a caller handed over that is not what it claims to be.
///
/// A prefix or an inception that does not fold carries the fold's own reason
/// and code 20, because the bytes are an artifact and the ledger inside it is
/// the thing that is wrong; everything else is a malformed artifact and code
/// 10. This is the mapping `mabel-cli` uses for the same files, so one file
/// fails the same way on both surfaces.
#[must_use]
pub fn artifact_error(name: &'static str, error: &ArtifactError) -> ServiceError {
    let failure = match error {
        ArtifactError::Prefix(violation) | ArtifactError::Inception(violation) => {
            fold_error(&violation.reason).with_detail("failed_at_seq", violation.seq)
        }
        _ => ServiceError::schema(error.code(), error.to_string()),
    };
    failure.with_detail("artifact", name)
}

/// A refusal from the signing path, which checks the byte-layout caps.
#[must_use]
pub fn build_error(error: &BuildError) -> ServiceError {
    ServiceError::schema("event_not_buildable", error.to_string())
}

/// No source answered for a ledger: code 30, the wording
/// `contracts/cli/errors.json` pins.
#[must_use]
pub fn no_source_available(ledger: LedgerId, queried: &[EndpointId]) -> ServiceError {
    ServiceError::network(
        "no_source_available",
        format!("no source answered for {}{ledger}", mabel_core::LINK_PREFIX),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("sources_queried", rendered(queried))
}

/// A peer that could not be dialled or did not answer: code 30.
#[must_use]
pub fn unreachable(endpoint: EndpointId, error: &NetError) -> ServiceError {
    ServiceError::network("peer_unreachable", peer_message(endpoint, error))
        .with_detail("endpoint", ids::key(&endpoint).as_str())
        .with_detail("error", error.to_string())
}

/// The one line a person reads about a peer that did not answer.
#[must_use]
pub fn peer_message(endpoint: EndpointId, error: &NetError) -> String {
    let endpoint = ids::key(&endpoint);
    match error {
        NetError::Connect { .. } => format!("no route to {endpoint}: {error}"),
        other => format!("{endpoint} did not answer: {other}"),
    }
}

/// The remote holds a head this node's copy does not extend: code 50, the
/// wording of `contracts/http/wallet-post-sync-push.json`.
#[must_use]
pub fn stale_head(
    ledger: LedgerId,
    local: u64,
    observed: &Head,
    source: EndpointId,
) -> ServiceError {
    ServiceError::state(
        "stale_head",
        format!(
            "witness {} reports head seq {}, this node holds seq {local}",
            ids::key(&source),
            observed.head_seq
        ),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("local_head_seq", local)
    .with_detail("observed_head_seq", observed.head_seq)
    .with_detail("source", ids::key(&source).as_str())
}

/// One side of an equivocation: a source and the event it holds at the
/// sequence where the two disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergent {
    /// The endpoint that served this event.
    pub source: EndpointId,
    /// The event it holds there.
    pub event: Id,
}

/// Two sources hold different valid events at one sequence: code 20
/// (proposal 001 section 3.7).
#[must_use]
pub fn equivocation(
    ledger: LedgerId,
    at_seq: u64,
    first: &Divergent,
    second: &Divergent,
) -> ServiceError {
    let candidate = |side: &Divergent| {
        serde_json::json!({
            "source": ids::key(&side.source).as_str(),
            "event_id": side.event.as_str(),
        })
    };
    ServiceError::ledger(
        "equivocation",
        format!(
            "two sources hold divergent events at seq {at_seq} of {}{ledger}",
            mabel_core::LINK_PREFIX
        ),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("at_seq", at_seq)
    .with_detail("candidates", vec![candidate(first), candidate(second)])
}

/// Every endpoint id as a document spells it.
#[must_use]
pub fn rendered(endpoints: &[EndpointId]) -> Vec<String> {
    endpoints
        .iter()
        .map(|endpoint| ids::key(endpoint).as_str().to_owned())
        .collect()
}

#[cfg(test)]
mod fold_message_tests {
    use super::fold_message;
    use mabel_core::fold::Reason;
    use mabel_core::{EventId, IdentityId, LINK_PREFIX};

    fn identity(seed: u8) -> IdentityId {
        IdentityId::from_bytes([seed; 32])
    }

    fn event(seed: u8) -> EventId {
        EventId::from_bytes([seed; 32])
    }

    fn key(seed: u8) -> iroh_base::PublicKey {
        iroh_base::SecretKey::from_bytes(&[seed; 32]).public()
    }

    /// Every rejection that names an identity names it the way a person reads
    /// one, and names it that way everywhere it appears in the sentence
    /// (decision 019).
    ///
    /// [`Reason`] is `#[non_exhaustive]`, so `fold_message` must keep a wildcard
    /// arm and the compiler cannot object when a new id-bearing variant falls
    /// through it. This is the thing that objects: add a variant that carries an
    /// identity, forget its arm, and the id shows up bare here.
    #[test]
    fn every_identity_bearing_reason_is_prefixed() {
        let one = identity(0x11);
        let two = identity(0x22);
        let cases: Vec<(Reason, Vec<IdentityId>)> = vec![
            (
                Reason::DuplicateAttestation {
                    subject: one,
                    attestation: event(0x33),
                },
                vec![one],
            ),
            (Reason::SelfAttestation(one), vec![one]),
            (
                Reason::DuplicateInvitation {
                    invitee: one,
                    invitation: event(0x33),
                },
                vec![one],
            ),
            (
                Reason::PrincipalKeyMismatch {
                    identity: one,
                    expected: key(0x44),
                    found: key(0x55),
                },
                vec![one],
            ),
            (
                Reason::AcceptanceForAnotherLedger {
                    named: one,
                    expected: two,
                },
                vec![one, two],
            ),
            (
                Reason::AcceptanceInviteeMismatch {
                    named: one,
                    invited: two,
                },
                vec![one, two],
            ),
            (
                Reason::DuplicatePrincipalKey {
                    key: key(0x44),
                    held_by: one,
                },
                vec![one],
            ),
            (Reason::RootNotRemovable(one), vec![one]),
            (Reason::UnknownRemovalTarget(one), vec![one]),
            (Reason::LastController(one), vec![one]),
            (Reason::RootNotDemotable(one), vec![one]),
            (Reason::DemotesLastController(one), vec![one]),
            (
                Reason::WrongLedger {
                    expected: two,
                    found: one,
                },
                vec![one, two],
            ),
        ];

        for (reason, identities) in cases {
            let shown = fold_message(&reason);
            for id in identities {
                let prefixed = format!("{LINK_PREFIX}{id}");
                assert!(
                    shown.contains(&prefixed),
                    "{reason:?} shows {id} without its prefix: {shown}"
                );
                // Not merely present somewhere: no occurrence of this id in the
                // sentence may stand without the prefix in front of it.
                assert!(
                    !shown.replace(&prefixed, "").contains(&id.to_string()),
                    "{reason:?} names {id} bare somewhere in: {shown}"
                );
            }
            // The wire evidence another implementation compares against is
            // untouched by how this one draws an id.
            assert!(
                !reason.to_string().contains(LINK_PREFIX),
                "the conformance vector for {reason:?} must stay bare"
            );
        }
    }
}

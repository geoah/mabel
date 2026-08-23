//! Fork-record validation (proposal 001 section 5, fork records).
//!
//! A fork record claims that two distinct valid events exist at one sequence
//! of one ledger. [`validate_fork_record`] is the single implementation of
//! that claim: the witness runs it before storing a record, and a reader runs
//! it on a record it was served, because a record carries both full
//! `SignedEvent`s so nobody has to ask a second time.
//!
//! A conflicting event is only evidence of equivocation if it would have been
//! accepted at that position. Both events are therefore applied to the
//! caller's shared-prefix state, which runs the whole of proposal 001
//! section 3.6 on each: canonical form, field table, sequence, ledger id,
//! `prev` link, the authorized signer at that position, the signature and the
//! payload's semantic rules. Anything that fails is invalid and is not stored.
//!
//! The prefix is the caller's: this crate does no IO and reads no store. A
//! witness passes the state it folded up to the shared head, and a reader
//! passes the state it folded from the events it holds.

use crate::fold::{LedgerState, Violation};
use crate::id::{EventId, LedgerId};

/// Two distinct valid events at one position of one ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fork {
    /// The ledger both events belong to.
    pub ledger: LedgerId,
    /// The position both events occupy, which is one past the shared prefix.
    pub seq: u64,
    /// The event the holder saw first and keeps.
    pub kept: EventId,
    /// The event that conflicts with it.
    pub conflicting: EventId,
}

/// Why a claimed fork is not one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ForkError {
    /// The shared prefix folded to nothing, so there is no position to fork
    /// at. A fork at seq 0 cannot exist: the ledger id is the id of the seq-0
    /// event, so two distinct seq-0 events are two ledgers.
    #[error("a fork record needs a shared prefix, and this one is empty")]
    EmptyPrefix,
    /// The kept event does not verify against the shared prefix.
    #[error("the kept event does not verify against the shared prefix: {0}")]
    Kept(Violation),
    /// The conflicting event does not verify against the shared prefix, so it
    /// is not evidence of anything and must not be stored.
    #[error("the conflicting event does not verify against the shared prefix: {0}")]
    Conflicting(Violation),
    /// Both events are the same event, which is a duplicate rather than a
    /// fork.
    #[error("both events are {0}, so there is no fork")]
    SameEvent(EventId),
}

impl ForkError {
    /// A stable snake-case name for this rejection class.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyPrefix => "empty_prefix",
            Self::Kept(_) => "kept_invalid",
            Self::Conflicting(_) => "conflicting_invalid",
            Self::SameEvent(_) => "same_event",
        }
    }
}

/// Checks that `kept` and `conflicting` are two distinct events that both
/// verify at the position after `prefix`.
///
/// `prefix` is the state folded from the events both branches agree on, so the
/// position under test is [`LedgerState::next_seq`]. The state is not
/// mutated: each event is applied to a copy.
pub fn validate_fork_record(
    prefix: &LedgerState,
    kept: &[u8],
    conflicting: &[u8],
) -> Result<Fork, ForkError> {
    let ledger = prefix.ledger().ok_or(ForkError::EmptyPrefix)?;
    let seq = prefix.next_seq();
    let kept = verify_at(prefix, seq, kept).map_err(ForkError::Kept)?;
    let conflicting = verify_at(prefix, seq, conflicting).map_err(ForkError::Conflicting)?;
    if kept == conflicting {
        return Err(ForkError::SameEvent(kept));
    }
    Ok(Fork {
        ledger,
        seq,
        kept,
        conflicting,
    })
}

/// Applies one event to a copy of the prefix and returns its `event_id`.
fn verify_at(prefix: &LedgerState, seq: u64, event: &[u8]) -> Result<EventId, Violation> {
    let mut state = prefix.clone();
    state
        .apply(event)
        .map_err(|reason| Violation { seq, reason })?;
    Ok(state.head().expect("an applied event is the head").event_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::{Reason, fold};
    use crate::id::IdentityId;
    use crate::sign::{
        BuiltEvent, Position, Root, build_inception, build_trust_attestation, build_witness_config,
    };
    use crate::{ID_BYTES, NONCE_BYTES};
    use iroh_base::SecretKey;
    use mabel_proto::v0::DeclaredKind;

    const T0: u64 = 1_700_000_000_000;

    fn secret(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    fn subject(seed: u8) -> IdentityId {
        IdentityId::from_bytes([seed; ID_BYTES])
    }

    /// A raw-rooted ledger keyed by `secret(1)`.
    fn alice() -> BuiltEvent {
        build_inception(
            &secret(1),
            DeclaredKind::Person,
            Root::Raw {
                reserve_key: &secret(2).public(),
            },
            [3u8; NONCE_BYTES],
            T0,
        )
        .expect("builds")
    }

    /// The state folded from the inception alone, and the position after it.
    fn prefix(inception: &BuiltEvent) -> (LedgerState, Position) {
        let (state, violation) = fold([&inception.signed_event]);
        assert_eq!(violation, None);
        let at = Position {
            ledger: inception.event_id.into(),
            seq: 1,
            prev: inception.event_id,
            prev_timestamp_ms: T0,
        };
        (state, at)
    }

    #[test]
    fn two_valid_events_at_one_sequence_are_a_fork() {
        let root = alice();
        let (state, at) = prefix(&root);
        let kept = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        let conflicting =
            build_witness_config(&secret(1), &at, &[secret(4).public()], T0).expect("builds");

        let fork = validate_fork_record(&state, &kept.signed_event, &conflicting.signed_event)
            .expect("both events verify at seq 1");
        assert_eq!(
            fork,
            Fork {
                ledger: root.event_id.into(),
                seq: 1,
                kept: kept.event_id,
                conflicting: conflicting.event_id,
            }
        );
        // The prefix is untouched, so a caller may test another candidate.
        assert_eq!(state.next_seq(), 1);
    }

    #[test]
    fn the_same_event_twice_is_not_a_fork() {
        let root = alice();
        let (state, at) = prefix(&root);
        let event = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        assert_eq!(
            validate_fork_record(&state, &event.signed_event, &event.signed_event),
            Err(ForkError::SameEvent(event.event_id)),
        );
    }

    #[test]
    fn a_malformed_conflicting_event_is_rejected() {
        let root = alice();
        let (state, at) = prefix(&root);
        let kept = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        // A truncated event never reaches the fold's chain rules.
        let mut malformed = kept.signed_event.clone();
        malformed.truncate(malformed.len() - 1);

        let error = validate_fork_record(&state, &kept.signed_event, &malformed)
            .expect_err("a truncated event is not evidence of a fork");
        assert_eq!(error.code(), "conflicting_invalid");
        let ForkError::Conflicting(violation) = error else {
            panic!("the conflicting event is the one that failed");
        };
        assert_eq!(violation.seq, 1);
        assert_eq!(violation.code(), "truncated");
    }

    #[test]
    fn an_event_signed_by_an_unauthorized_key_is_rejected() {
        let root = alice();
        let (state, at) = prefix(&root);
        let kept = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        // secret(6) holds no principal on this ledger, so its event is not a
        // fork: it is one valid event and one forgery.
        let forged = build_trust_attestation(&secret(6), &at, subject(9), T0).expect("builds");

        assert_eq!(
            validate_fork_record(&state, &kept.signed_event, &forged.signed_event),
            Err(ForkError::Conflicting(Violation {
                seq: 1,
                reason: Reason::UnauthorizedSigner {
                    key: secret(6).public(),
                },
            })),
        );
    }

    #[test]
    fn an_invalid_kept_event_is_named_as_the_kept_one() {
        let root = alice();
        let (state, at) = prefix(&root);
        let conflicting = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        let forged = build_trust_attestation(&secret(6), &at, subject(9), T0).expect("builds");

        let error = validate_fork_record(&state, &forged.signed_event, &conflicting.signed_event)
            .expect_err("the kept event does not verify");
        assert_eq!(error.code(), "kept_invalid");
    }

    #[test]
    fn an_event_at_another_position_is_rejected() {
        let root = alice();
        let (state, at) = prefix(&root);
        let kept = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        let mut later = at;
        later.seq = 2;
        let elsewhere =
            build_trust_attestation(&secret(1), &later, subject(9), T0).expect("builds");

        assert_eq!(
            validate_fork_record(&state, &kept.signed_event, &elsewhere.signed_event),
            Err(ForkError::Conflicting(Violation {
                seq: 1,
                reason: Reason::WrongSeq {
                    expected: 1,
                    found: 2,
                },
            })),
        );
    }

    #[test]
    fn an_event_from_another_ledger_is_rejected() {
        let root = alice();
        let (state, at) = prefix(&root);
        let kept = build_trust_attestation(&secret(1), &at, subject(9), T0).expect("builds");
        let mut elsewhere = at;
        elsewhere.ledger = subject(0xbb);
        let other =
            build_trust_attestation(&secret(1), &elsewhere, subject(9), T0).expect("builds");

        let error = validate_fork_record(&state, &kept.signed_event, &other.signed_event)
            .expect_err("an event of another ledger is not a fork of this one");
        assert_eq!(error.code(), "conflicting_invalid");
    }

    #[test]
    fn an_empty_prefix_has_no_position_to_fork_at() {
        let root = alice();
        let error = validate_fork_record(
            &LedgerState::default(),
            &root.signed_event,
            &root.signed_event,
        )
        .expect_err("seq 0 cannot fork");
        assert_eq!(error, ForkError::EmptyPrefix);
        assert_eq!(error.code(), "empty_prefix");
    }
}

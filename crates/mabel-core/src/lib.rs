//! Ledger semantics for mabel (proposal 001 section 3).
//!
//! This crate is IO-free and async-free: it exposes pure functions over
//! bytes so cold verification is a real code path (hearsay pitfall 5).

pub mod digest;
pub mod encoding;
pub mod fold;
pub mod id;
pub mod sign;
pub mod validate;

pub use digest::{accept_input, event_id, reserve_commit, sign_input};
pub use fold::{
    Attestation, Head, Invite, InviteStatus, LedgerKind, LedgerState, OrgState, PersonState,
    Principal, Reason, Violation, fold,
};
pub use id::{EventId, IdentityId, LedgerId, ParseIdError};
pub use mabel_proto::v0 as proto;
pub use sign::{
    BuildError, BuiltEvent, DetachedAcceptance, Position, build_acceptance, build_org_acceptance,
    build_org_inception, build_org_invite, build_org_removal, build_person_inception,
    build_trust_attestation, build_trust_revocation, build_witness_config, ledger_timestamp_ms,
};
pub use validate::{
    MessageDescriptor, StandaloneInception, WireError, verify_inception_standalone,
};

pub const EVENT_ID_DOMAIN: &[u8] = b"mabel/event/v0\n";
pub const SIGN_DOMAIN: &[u8] = b"mabel/sig/v0\n";
pub const ACCEPT_DOMAIN: &[u8] = b"mabel/accept/v0\n";
pub const RESERVE_DOMAIN: &[u8] = b"mabel/reserve/v0\n";

/// Upper bound for `timestamp_ms`: 2100-01-01T00:00:00Z.
pub const MAX_TIMESTAMP_MS: u64 = 4_102_444_800_000;

/// Maximum encoded size of a `SignedEvent` (proposal 001 section 5).
pub const MAX_EVENT_BYTES: usize = 4096;

/// Maximum encoded size of an embedded inception, `founder_inception` or
/// `invitee_inception` (proposal 001, clarifications).
pub const MAX_EMBEDDED_INCEPTION_BYTES: usize = 1024;

/// Maximum encoded size of an `Acceptance` blob (proposal 001 section 3.4).
pub const MAX_ACCEPTANCE_BYTES: usize = 1024;

/// Length of an identity id, ledger id, event id and public key.
pub const ID_BYTES: usize = 32;

/// Length of an ed25519 signature.
pub const SIG_BYTES: usize = 64;

/// Length of an inception `nonce` (proposal 001 section 3.3).
pub const NONCE_BYTES: usize = 16;

/// Maximum number of witnesses in one `WitnessConfig`.
pub const MAX_WITNESSES: usize = 16;

#[cfg(test)]
mod tests {
    /// Milestone-1 probe (proposal 001 section 4): iroh-base key types work
    /// under `default-features = false, features = ["key"]` with no runtime
    /// dependencies pulled into this crate.
    #[test]
    fn iroh_base_key_probe() {
        let sk = iroh_base::SecretKey::from_bytes(&[7u8; 32]);
        let pk = sk.public();
        let sig = sk.sign(b"probe");
        pk.verify(b"probe", &sig).expect("signature verifies");
    }
}

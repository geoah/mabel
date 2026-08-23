//! Ledger semantics for mabel (proposal 001 section 3).
//!
//! This crate is IO-free and async-free: it exposes pure functions over
//! bytes so cold verification is a real code path (hearsay pitfall 5).

pub mod artifacts;
pub mod digest;
pub mod encoding;
pub mod fold;
pub mod fork;
pub mod id;
pub mod sign;
pub mod validate;

pub use artifacts::{
    AcceptanceFile, ArtifactError, IdentityDescriptor, InvitationBundle, InvitationSummary,
};
pub use digest::{accept_input, event_id, reserve_commit, sign_input};
pub use fold::{
    Attestation, Head, Invitation, InvitationStatus, LedgerRoot, LedgerState, Principal, Profile,
    Reason, SigningPrincipal, Violation, declared_kind_name, fold,
};
pub use fork::{Fork, ForkError, validate_fork_record};
pub use id::{EventId, IdentityId, LedgerId, ParseIdError};
pub use mabel_proto::v0 as proto;
pub use sign::{
    BuildError, BuiltEvent, DetachedAcceptance, Position, Root, build_acceptance, build_inception,
    build_membership_acceptance, build_membership_invitation, build_membership_removal,
    build_profile_update, build_trust_attestation, build_trust_revocation, build_witness_config,
    ledger_timestamp_ms,
};
pub use validate::{
    MessageDescriptor, StandaloneInception, StringRule, WireError, verify_inception_standalone,
};

pub const EVENT_ID_DOMAIN: &[u8] = b"mabel/event/v0\n";
pub const SIGN_DOMAIN: &[u8] = b"mabel/sig/v0\n";
pub const ACCEPT_DOMAIN: &[u8] = b"mabel/accept/v0\n";
pub const RESERVE_DOMAIN: &[u8] = b"mabel/reserve/v0\n";

/// Upper bound for `timestamp_ms`: 2100-01-01T00:00:00Z.
pub const MAX_TIMESTAMP_MS: u64 = 4_102_444_800_000;

/// Maximum encoded size of a `SignedEvent` (proposal 001 section 5).
pub const MAX_EVENT_BYTES: usize = 4096;

/// Maximum encoded size of an embedded inception, `IdentityRoot.founder_inception`
/// or `MembershipInvitation.invitee_inception` (proposal 001, clarifications).
pub const MAX_EMBEDDED_INCEPTION_BYTES: usize = 1024;

/// Maximum encoded size of an `Acceptance` blob (proposal 001 section 3.4).
pub const MAX_ACCEPTANCE_BYTES: usize = 1024;

/// Maximum encoded size of an `InvitationBundle` file (proposal 001
/// section 3.8).
pub const MAX_INVITATION_BUNDLE_BYTES: usize = 1024 * 1024;

/// Maximum encoded size of an `AcceptanceFile` (proposal 001 section 3.8).
pub const MAX_ACCEPTANCE_FILE_BYTES: usize = 4096;

/// Maximum encoded size of an `IdentityDescriptor` file (proposal 001
/// section 3.8).
pub const MAX_IDENTITY_DESCRIPTOR_BYTES: usize = 64 * 1024;

/// Most events an `InvitationBundle` may carry, which is the per-ledger event
/// cap of proposal 001 section 5. The 1 MiB file cap binds first for any
/// realistic prefix.
pub const MAX_BUNDLE_EVENTS: usize = 4096;

/// Maximum encoded length of `ProfileUpdate.display_name` (proposal 003
/// section 1).
pub const MAX_DISPLAY_NAME_BYTES: usize = 64;

/// Maximum encoded length of `ProfileUpdate.hostname`: 253 bytes minus the
/// `_mabel.` prefix the DNS check prepends (proposal 003 section 2).
pub const MAX_HOSTNAME_BYTES: usize = 246;

/// Longest label a hostname may carry (proposal 003 section 2).
pub const MAX_HOSTNAME_LABEL_BYTES: usize = 63;

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
        let signature = sk.sign(b"probe");
        pk.verify(b"probe", &signature).expect("signature verifies");
    }
}

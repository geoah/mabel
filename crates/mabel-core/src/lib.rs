//! Ledger semantics for mabel (proposal 001 section 3).
//!
//! This crate is IO-free and async-free: it exposes pure functions over
//! bytes so cold verification is a real code path (hearsay pitfall 5).

pub const EVENT_ID_DOMAIN: &[u8] = b"mabel/event/v0\n";
pub const SIGN_DOMAIN: &[u8] = b"mabel/sig/v0\n";
pub const ACCEPT_DOMAIN: &[u8] = b"mabel/accept/v0\n";
pub const RESERVE_DOMAIN: &[u8] = b"mabel/reserve/v0\n";

/// Upper bound for `timestamp_ms`: 2100-01-01T00:00:00Z.
pub const MAX_TIMESTAMP_MS: u64 = 4_102_444_800_000;

/// Maximum encoded size of a `SignedEvent` (proposal 001 section 5).
pub const MAX_EVENT_BYTES: usize = 4096;

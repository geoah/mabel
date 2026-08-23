//! Sync protocol over Iroh (proposal 001 section 5).

/// ALPN for the mabel ledger sync protocol.
pub const ALPN: &[u8] = b"mabel/ledger/0";

/// Hard cap for a protocol frame in either direction.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

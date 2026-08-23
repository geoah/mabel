//! Sync protocol over Iroh (proposal 001 section 5).
//!
//! One ALPN, [`ALPN`], one request per bidirectional stream. The client
//! writes an encoded `Request`, calls `finish()` on the send stream and reads
//! the encoded `Response` to EOF under a hard byte cap; the server mirrors
//! that. There is no length prefix: QUIC stream termination frames both
//! directions.
//!
//! Every received frame passes the [`mabel_core::validate`] scanner before
//! anything decodes it, so a caps violation or a malformed record is answered
//! before a peer-sized allocation happens (proposal 001 section 5, pitfall 7).
//!
//! This crate owns four reject codes: `MALFORMED` for a validator failure,
//! `TOO_LARGE` for a cap, `UNSUPPORTED` for a `Request` variant this version
//! does not know, and `BUSY` when the verification semaphore is saturated.
//! `INVALID`, `FORK` and `NOT_ADMITTED` come from the [`Store`] and pass
//! through unchanged.
//!
//! Transport identity is provenance, never authorization (proposal 001
//! section 4): the peer's `EndpointId` reaches the store as [`Provenance`] and
//! nothing in this crate reads it to decide anything.

pub mod client;
pub mod descriptors;
pub mod endpoint;
pub mod error;
pub mod server;
pub mod store;
pub mod testing;
pub mod wire;

pub use client::Client;
pub use endpoint::{BoundEndpoint, EndpointConfig, RelayChoice, bind_endpoint, parse_peer_ticket};
pub use error::{Error, Rejection};
pub use server::{LedgerProtocol, ServerConfig};
pub use store::{
    EventPage, ForkRecord, Head, LedgerSummary, Page, Provenance, PushOutcome, Store, StoreError,
    StoreFuture,
};

/// ALPN for the mabel ledger sync protocol.
pub const ALPN: &[u8] = b"mabel/ledger/0";

/// Hard cap for a protocol frame in either direction.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// Maximum encoded size of one `SignedEvent` inside a frame.
pub const MAX_EVENT_BYTES: usize = mabel_core::MAX_EVENT_BYTES;

/// Most events one `Push` may carry.
pub const MAX_PUSH_EVENTS: usize = 512;

/// Maximum encoded size of one `PushReq`.
pub const MAX_PUSH_BYTES: usize = 2 * 1024 * 1024;

/// `Get.limit` is clamped to this.
pub const MAX_GET_LIMIT: u32 = 512;

/// `List.limit` is clamped to this.
pub const MAX_LIST_LIMIT: u32 = 256;

/// `Forks.limit` is clamped to this.
pub const MAX_FORKS_LIMIT: u32 = 64;

/// Maximum encoded size of `RejectedResp.msg`.
pub const MAX_REJECT_MSG_BYTES: usize = 256;

/// Connections one server serves at once; further connections are closed.
pub const MAX_CONNECTIONS: usize = 32;

/// Requests one connection may make before the server closes it.
pub const MAX_REQUESTS_PER_CONNECTION: u32 = 64;

/// Requests one server validates at once; further requests answer `BUSY`.
pub const MAX_CONCURRENT_VERIFICATIONS: usize = 8;

/// Bytes a response body fills before it stops and sets `more`.
///
/// The remainder of [`MAX_FRAME_BYTES`] covers the framing around the entries.
pub const RESPONSE_BUDGET_BYTES: usize = MAX_FRAME_BYTES - 64 * 1024;

/// The QUIC close code the server uses when it is already serving
/// [`MAX_CONNECTIONS`].
pub const CLOSE_CONNECTION_LIMIT: u32 = 1;

/// The QUIC close code the server uses after [`MAX_REQUESTS_PER_CONNECTION`].
pub const CLOSE_REQUEST_LIMIT: u32 = 2;

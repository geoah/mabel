//! What a client call can fail with, and the rejection a server can answer.

use std::fmt;

use mabel_core::proto::RejectCode;
use mabel_core::validate::WireError;

use crate::{MAX_FRAME_BYTES, MAX_PUSH_BYTES, MAX_PUSH_EVENTS};

/// A `RejectedResp`: why the peer refused the request.
///
/// `MALFORMED`, `TOO_LARGE`, `UNSUPPORTED` and `BUSY` come from the transport
/// layer of this crate; `INVALID`, `FORK` and `NOT_ADMITTED` come from the
/// store (proposal 001 section 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// The code the peer sent.
    pub code: RejectCode,
    /// The sequence the rejection is about, 0 when none applies.
    pub at_seq: u64,
    /// A human-readable reason, never authoritative.
    pub msg: String,
}

impl Rejection {
    /// A rejection with no sequence attached.
    pub fn new(code: RejectCode, msg: impl Into<String>) -> Self {
        Self {
            code,
            at_seq: 0,
            msg: msg.into(),
        }
    }

    /// A rejection about one sequence.
    pub fn at(code: RejectCode, at_seq: u64, msg: impl Into<String>) -> Self {
        Self {
            code,
            at_seq,
            msg: msg.into(),
        }
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code.as_str_name())?;
        if self.at_seq != 0 {
            write!(f, " at seq {}", self.at_seq)?;
        }
        if !self.msg.is_empty() {
            write!(f, ": {}", self.msg)?;
        }
        Ok(())
    }
}

/// Why a client call did not produce an answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The endpoint could not reach the peer.
    #[error("connecting to the peer failed")]
    Connect {
        /// The iroh error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The connection or one of its streams failed mid-request.
    #[error("the connection failed")]
    Connection {
        /// The iroh error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The response frame is over [`MAX_FRAME_BYTES`].
    #[error("the response frame is over the {MAX_FRAME_BYTES}-byte cap")]
    ResponseTooLarge,
    /// The response frame did not pass the field table.
    #[error("the response is not a valid Response: {0}")]
    Malformed(#[from] WireError),
    /// The peer answered `RejectedResp`.
    #[error("the peer rejected the request: {0}")]
    Rejected(Rejection),
    /// The peer answered a variant this request never gets.
    #[error("the peer answered {got} to a {sent} request")]
    UnexpectedResponse {
        /// The request that was sent.
        sent: &'static str,
        /// The response variant that came back.
        got: &'static str,
    },
    /// The peer answered a well-formed frame that contradicts the protocol,
    /// for example a `Get` page that does not advance.
    #[error("the peer broke the protocol: {0}")]
    Protocol(String),
    /// The caller asked to push more than one `Push` may carry.
    #[error(
        "the push carries {events} events and {bytes} bytes, over the \
         {MAX_PUSH_EVENTS}-event and {MAX_PUSH_BYTES}-byte caps"
    )]
    PushTooLarge {
        /// How many events the caller offered.
        events: usize,
        /// How many bytes those events encode to.
        bytes: usize,
    },
}

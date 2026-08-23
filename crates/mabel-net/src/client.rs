//! The client side of `mabel/ledger/0`.
//!
//! One [`Client`] owns one connection and reuses it: each request opens a
//! bidirectional stream, writes the encoded `Request`, finishes the send side
//! and reads the encoded `Response` to EOF under [`crate::MAX_FRAME_BYTES`].
//!
//! Nothing a peer answers is trusted here beyond the field table. A response
//! is validated before it is read, and the events it carries are handed back
//! as the byte strings that arrived, for the caller to verify (proposal 001
//! section 3.7).

use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, EndpointId};
use mabel_core::LedgerId;

use crate::error::{Error, Rejection};
use crate::store::{EventPage, ForkRecord, Head, LedgerSummary, Page, PushOutcome};
use crate::wire::{self, Response};
use crate::{ALPN, MAX_FRAME_BYTES, MAX_PUSH_BYTES, MAX_PUSH_EVENTS};

/// A connected sync client.
#[derive(Debug, Clone)]
pub struct Client {
    connection: Connection,
    max_frame_bytes: usize,
}

impl Client {
    /// Dials `addr` over [`ALPN`].
    ///
    /// `addr` may be an [`EndpointId`], which relies on the endpoint's
    /// address lookup, or a full [`EndpointAddr`] from a ticket or from
    /// `endpoint.addr()`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Connect`] if the peer cannot be reached.
    pub async fn connect(
        endpoint: &Endpoint,
        addr: impl Into<EndpointAddr>,
    ) -> Result<Self, Error> {
        let connection = endpoint
            .connect(addr, ALPN)
            .await
            .map_err(|error| Error::Connect {
                source: Box::new(error),
            })?;
        Ok(Self::from_connection(connection))
    }

    /// Wraps a connection that already speaks [`ALPN`].
    pub fn from_connection(connection: Connection) -> Self {
        Self {
            connection,
            max_frame_bytes: MAX_FRAME_BYTES,
        }
    }

    /// The peer's endpoint id, authenticated by the QUIC handshake.
    pub fn remote_id(&self) -> EndpointId {
        self.connection.remote_id()
    }

    /// Closes the connection.
    pub fn close(&self) {
        self.connection.close(0u32.into(), b"done");
    }

    /// Sends one encoded `Request` frame and returns the encoded `Response`
    /// frame, validating neither.
    ///
    /// This is the escape hatch the typed helpers are built on. Tests use it
    /// to send frames the encoders cannot produce.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Connection`] if the stream fails and
    /// [`Error::ResponseTooLarge`] if the answer passes the frame cap.
    pub async fn send_frame(&self, frame: &[u8]) -> Result<Vec<u8>, Error> {
        let (mut send, mut recv) =
            self.connection
                .open_bi()
                .await
                .map_err(|error| Error::Connection {
                    source: Box::new(error),
                })?;

        // EOF frames the request; there is no length prefix.
        let written = match send.write_all(frame).await {
            // `finish` only fails on an already closed stream, which the read
            // below reports better than this error does.
            Ok(()) => send.finish().map_err(|error| Error::Connection {
                source: Box::new(error),
            }),
            Err(error) => Err(Error::Connection {
                source: Box::new(error),
            }),
        };

        // A server that refuses the frame stops reading and answers straight
        // away, which fails the write above. The answer still arrives, so the
        // read decides: only if it fails too does the write error surface.
        match recv.read_to_end(self.max_frame_bytes).await {
            Ok(answer) => Ok(answer),
            Err(error) => {
                written?;
                Err(match error {
                    iroh::endpoint::ReadToEndError::TooLong => Error::ResponseTooLarge,
                    other => Error::Connection {
                        source: Box::new(other),
                    },
                })
            }
        }
    }

    /// Sends a request and reads the answer, turning `RejectedResp` into an
    /// error.
    async fn request(&self, frame: &[u8]) -> Result<Response, Error> {
        let answer = self.send_frame(frame).await?;
        match wire::parse_response(&answer)? {
            Response::Rejected(rejection) => Err(Error::Rejected(rejection)),
            other => Ok(other),
        }
    }

    /// Where a ledger ends, or `None` if the peer does not hold it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Rejected`] if the peer refused the request, and the
    /// transport errors of [`Client::send_frame`].
    pub async fn head(&self, ledger: LedgerId) -> Result<Option<Head>, Error> {
        match self.request(&wire::head_request(ledger)).await? {
            Response::Head(head) => Ok(Some(head)),
            Response::NotFound => Ok(None),
            other => Err(unexpected("Head", &other)),
        }
    }

    /// One page of a ledger's events from `since` inclusive.
    ///
    /// `limit` 0 asks for as many as the peer's cap allows.
    ///
    /// # Errors
    ///
    /// As [`Client::head`].
    pub async fn get(
        &self,
        ledger: LedgerId,
        since: u64,
        limit: u32,
    ) -> Result<Option<EventPage>, Error> {
        match self
            .request(&wire::get_request(ledger, since, limit))
            .await?
        {
            Response::Events(page) => Ok(Some(page)),
            Response::NotFound => Ok(None),
            other => Err(unexpected("Get", &other)),
        }
    }

    /// Every event from `since` inclusive, paging until `more` is false.
    ///
    /// # Errors
    ///
    /// As [`Client::head`], plus [`Error::Protocol`] if a page claims more
    /// events but carries none, which would page forever.
    pub async fn get_all(
        &self,
        ledger: LedgerId,
        since: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, Error> {
        let mut all: Vec<Vec<u8>> = Vec::new();
        let mut next = since;
        loop {
            let Some(page) = self.get(ledger, next, 0).await? else {
                return Ok(None);
            };
            let count = page.events.len() as u64;
            all.extend(page.events);
            if !page.more {
                return Ok(Some(all));
            }
            if count == 0 {
                return Err(Error::Protocol(
                    "a Get page claims more events but carries none".to_string(),
                ));
            }
            // The page is contiguous from `next`, so the next page starts one
            // past the last event it carried.
            next += count;
        }
    }

    /// Offers events for a ledger.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PushTooLarge`] before sending anything if the events
    /// exceed [`MAX_PUSH_EVENTS`] or [`MAX_PUSH_BYTES`], and otherwise as
    /// [`Client::head`].
    pub async fn push(&self, ledger: LedgerId, events: &[Vec<u8>]) -> Result<PushOutcome, Error> {
        let bytes: usize = events.iter().map(Vec::len).sum();
        if events.len() > MAX_PUSH_EVENTS || bytes > MAX_PUSH_BYTES {
            return Err(Error::PushTooLarge {
                events: events.len(),
                bytes,
            });
        }
        match self.request(&wire::push_request(ledger, events)).await? {
            Response::Accepted(outcome) => Ok(outcome),
            other => Err(unexpected("Push", &other)),
        }
    }

    /// One page of the peer's ledgers, by ascending ledger id.
    ///
    /// # Errors
    ///
    /// As [`Client::head`].
    pub async fn list(&self, offset: u32, limit: u32) -> Result<Page<LedgerSummary>, Error> {
        match self.request(&wire::list_request(offset, limit)).await? {
            Response::Ledgers(page) => Ok(page),
            other => Err(unexpected("List", &other)),
        }
    }

    /// One page of fork records, for one ledger or for every ledger.
    ///
    /// # Errors
    ///
    /// As [`Client::head`].
    pub async fn forks(
        &self,
        ledger: Option<LedgerId>,
        offset: u32,
        limit: u32,
    ) -> Result<Page<ForkRecord>, Error> {
        match self
            .request(&wire::forks_request(ledger, offset, limit))
            .await?
        {
            Response::Forks(page) => Ok(page),
            other => Err(unexpected("Forks", &other)),
        }
    }
}

fn unexpected(sent: &'static str, got: &Response) -> Error {
    Error::UnexpectedResponse {
        sent,
        got: got.name(),
    }
}

/// The rejection inside an error, if it is one.
///
/// Callers that branch on a reject code, such as a wallet handling
/// `NOT_ADMITTED`, use this instead of matching the whole error.
pub fn rejection_of(error: &Error) -> Option<&Rejection> {
    match error {
        Error::Rejected(rejection) => Some(rejection),
        _ => None,
    }
}

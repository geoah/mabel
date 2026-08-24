//! The `ProtocolHandler` that answers sync requests from a [`Store`].
//!
//! One connection, many requests: the handler loops on `accept_bi` so a
//! wallet can push several ledgers without reconnecting. Each stream carries
//! one request, read to EOF under the frame cap, and one response, written
//! and finished.
//!
//! Three limits bound the work a peer can ask for (proposal 001 section 5):
//! [`ServerConfig::max_connections`] connections, then further connections
//! are closed; [`ServerConfig::max_requests_per_connection`] requests, then
//! the connection is closed; and
//! [`ServerConfig::max_concurrent_verifications`] frames being validated at
//! once, past which a request is answered `BUSY` rather than queued.

use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, VarInt};
use iroh::protocol::{AcceptError, ProtocolHandler};
use mabel_core::proto::RejectCode;
use tokio::sync::Semaphore;
use tracing::{debug, error, warn};

use crate::error::Rejection;
use crate::store::{Provenance, Store, StoreError};
use crate::wire::{self, Request};
use crate::{
    CLOSE_CONNECTION_LIMIT, CLOSE_REQUEST_LIMIT, MAX_CONCURRENT_VERIFICATIONS, MAX_CONNECTIONS,
    MAX_FORKS_LIMIT, MAX_FRAME_BYTES, MAX_GET_LIMIT, MAX_LIST_LIMIT, MAX_REQUESTS_PER_CONNECTION,
    RESPONSE_BUDGET_BYTES,
};

/// How long the server waits for the peer to acknowledge the last response
/// on a connection it is about to close.
const LAST_RESPONSE_GRACE: Duration = Duration::from_secs(3);

/// The caps one server enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    /// Connections served at once. Further connections are closed with
    /// [`crate::CLOSE_CONNECTION_LIMIT`].
    pub max_connections: usize,
    /// Requests one connection may make before it is closed with
    /// [`crate::CLOSE_REQUEST_LIMIT`].
    pub max_requests_per_connection: u32,
    /// Frames validated at once. Further requests answer `BUSY`.
    pub max_concurrent_verifications: usize,
    /// The hard cap on a received frame, enforced before allocation.
    pub max_frame_bytes: usize,
    /// Bytes a response body fills before it stops and sets `more`.
    pub response_budget_bytes: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_connections: MAX_CONNECTIONS,
            max_requests_per_connection: MAX_REQUESTS_PER_CONNECTION,
            max_concurrent_verifications: MAX_CONCURRENT_VERIFICATIONS,
            max_frame_bytes: MAX_FRAME_BYTES,
            response_budget_bytes: RESPONSE_BUDGET_BYTES,
        }
    }
}

/// Serves `mabel/ledger/0` from a [`Store`].
#[derive(Debug, Clone)]
pub struct LedgerProtocol {
    store: Arc<dyn Store>,
    config: ServerConfig,
    connections: Arc<Semaphore>,
    verifications: Arc<Semaphore>,
}

impl LedgerProtocol {
    /// A server with the caps of proposal 001 section 5.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self::with_config(store, ServerConfig::default())
    }

    /// A server with caps a test can shrink.
    pub fn with_config(store: Arc<dyn Store>, config: ServerConfig) -> Self {
        Self {
            store,
            connections: Arc::new(Semaphore::new(config.max_connections)),
            verifications: Arc::new(Semaphore::new(config.max_concurrent_verifications)),
            config,
        }
    }

    /// The caps this server enforces.
    pub fn config(&self) -> ServerConfig {
        self.config
    }

    /// Answers one encoded `Request` frame with one encoded `Response` frame.
    ///
    /// This is the whole protocol: the accept loop only adds framing. A
    /// caller that already has the bytes, such as a test or a local debug
    /// surface, can use it directly.
    pub async fn handle_frame(&self, frame: &[u8], provenance: Provenance) -> Vec<u8> {
        let Ok(_permit) = self.verifications.clone().try_acquire_owned() else {
            debug!("answering BUSY: the verification semaphore is saturated");
            return wire::rejected_response(&Rejection::new(
                RejectCode::Busy,
                "the verification queue is full, retry",
            ));
        };
        match wire::parse_request(frame) {
            Ok(request) => self.answer(request, provenance).await,
            Err(rejection) => {
                debug!(code = rejection.code.as_str_name(), "refusing a frame");
                wire::rejected_response(&rejection)
            }
        }
    }

    async fn answer(&self, request: Request<'_>, provenance: Provenance) -> Vec<u8> {
        let name = request.name();
        match self.dispatch(request, provenance).await {
            Ok(frame) => frame,
            Err(StoreError::Rejected(rejection)) => wire::rejected_response(&rejection),
            Err(StoreError::Unavailable(reason)) => {
                error!(request = name, %reason, "the store failed");
                wire::rejected_response(&Rejection::new(
                    RejectCode::Busy,
                    "the store cannot answer right now",
                ))
            }
        }
    }

    async fn dispatch(
        &self,
        request: Request<'_>,
        provenance: Provenance,
    ) -> Result<Vec<u8>, StoreError> {
        let budget = self.config.response_budget_bytes;
        match request {
            Request::Head { ledger } => Ok(match self.store.head(ledger).await? {
                Some(head) => wire::head_response(&head),
                None => wire::not_found_response(),
            }),
            Request::Get {
                ledger,
                since,
                limit,
            } => {
                let limit = clamp(limit, MAX_GET_LIMIT);
                let Some(page) = self.store.read_from(ledger, since, limit).await? else {
                    return Ok(wire::not_found_response());
                };
                let (events, truncated) =
                    wire::fill_budget(&page.events, 1, budget, |event| event.clone());
                let borrowed: Vec<&[u8]> = events.iter().map(Vec::as_slice).collect();
                Ok(wire::events_response(
                    &borrowed,
                    page.head_seq,
                    page.more || truncated,
                ))
            }
            Request::Push { ledger, events } => {
                let events: Vec<Vec<u8>> = events.into_iter().map(<[u8]>::to_vec).collect();
                let outcome = self.store.push(ledger, events, provenance).await?;
                Ok(wire::accepted_response(&outcome))
            }
            // The store decides what a `List` names: the enumerable set, not the
            // stored set (proposal 006 section 8). Nothing here filters, and
            // nothing here is authorization: a caller that can name a ledger id
            // still reads that ledger through `Get`.
            Request::List { offset, limit } => {
                let limit = clamp(limit, MAX_LIST_LIMIT);
                let page = self.store.list(offset as usize, limit).await?;
                let (entries, truncated) =
                    wire::fill_budget(&page.items, 1, budget, wire::summary_entry);
                Ok(wire::ledgers_response(&entries, page.more || truncated))
            }
            Request::Forks {
                ledger,
                offset,
                limit,
            } => {
                let limit = clamp(limit, MAX_FORKS_LIMIT);
                let page = self.store.forks(ledger, offset as usize, limit).await?;
                let (entries, truncated) =
                    wire::fill_budget(&page.items, 1, budget, wire::fork_entry);
                Ok(wire::forks_response(&entries, page.more || truncated))
            }
        }
    }

    /// Reads one request frame and answers it, or answers the frame cap.
    async fn serve_stream(&self, recv: &mut RecvStream, provenance: Provenance) -> Option<Vec<u8>> {
        match recv.read_to_end(self.config.max_frame_bytes).await {
            Ok(frame) => Some(self.handle_frame(&frame, provenance).await),
            Err(error) if is_too_long(&error) => {
                debug!("answering TOO_LARGE: a request frame passed the frame cap");
                Some(wire::rejected_response(&Rejection::new(
                    RejectCode::TooLarge,
                    "the request frame is over the 4 MiB cap",
                )))
            }
            Err(error) => {
                debug!(%error, "a request stream failed before EOF");
                None
            }
        }
    }
}

fn clamp(limit: u32, cap: u32) -> usize {
    // A limit of 0 is absent on the wire and means "as many as the cap".
    if limit == 0 {
        cap as usize
    } else {
        limit.min(cap) as usize
    }
}

fn is_too_long(error: &iroh::endpoint::ReadToEndError) -> bool {
    matches!(error, iroh::endpoint::ReadToEndError::TooLong)
}

impl ProtocolHandler for LedgerProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let Ok(_slot) = self.connections.clone().try_acquire_owned() else {
            warn!(
                limit = self.config.max_connections,
                "closing a connection: the connection limit is reached"
            );
            connection.close(
                VarInt::from_u32(CLOSE_CONNECTION_LIMIT),
                b"too many connections",
            );
            return Ok(());
        };

        // The peer's endpoint id is authenticated by the QUIC handshake and
        // travels no further than the store's provenance argument (proposal
        // 001 section 4).
        let provenance = Provenance::from_endpoint(connection.remote_id());
        let mut served = 0u32;

        while served < self.config.max_requests_per_connection {
            let Ok((mut send, mut recv)) = connection.accept_bi().await else {
                // The peer closed, or the connection failed; either way there
                // is nothing left to answer.
                return Ok(());
            };
            served += 1;
            let Some(response) = self.serve_stream(&mut recv, provenance).await else {
                continue;
            };
            if let Err(error) = send.write_all(&response).await {
                debug!(%error, "a response could not be written");
                return Ok(());
            }
            if let Err(error) = send.finish() {
                debug!(%error, "a response stream was already closed");
                return Ok(());
            }
            if served >= self.config.max_requests_per_connection {
                // Closing the connection can discard stream data the peer has
                // not acknowledged, so wait for the last response to land.
                let _ = tokio::time::timeout(LAST_RESPONSE_GRACE, send.stopped()).await;
            }
        }

        debug!(
            served,
            "closing a connection: the per-connection request limit is reached"
        );
        connection.close(
            VarInt::from_u32(CLOSE_REQUEST_LIMIT),
            b"request limit reached",
        );
        Ok(())
    }
}

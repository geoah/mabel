//! Pushing ledgers to witnesses, fetching them back, and the append
//! discipline for a ledger this wallet does not solely control.
//!
//! Dialling names an [`EndpointId`] and nothing else; a `--peer` ticket is
//! seeded into the endpoint's address lookup beforehand and is an address
//! hint, never authorization (proposal 001 sections 3.7 and 4). Every event a
//! peer serves is verified from nothing before it is stored: no source is
//! trusted, including one this wallet pushed to a minute ago.

use std::time::Duration;

use iroh::{Endpoint, EndpointId};
use mabel_core::LedgerId;
use mabel_core::fold::fold;
use mabel_net::client::rejection_of;
use mabel_net::store::Head;
use mabel_net::{Client, Error as NetError};

use crate::api::documents::{PushResult, PushStatus, Pushed};
use crate::api::error::ServiceError;
use crate::wallet::core::WalletCore;
use crate::wallet::error::{peer_message, stale_head, unreachable};
use crate::wallet::ids;
use crate::wallet::ledger::LoadedLedger;

/// How long a dial and one request may take before the peer counts as
/// unreachable.
///
/// The wording of `contracts/cli/sync-push.json` names this number, so the
/// message is built from it rather than from a literal.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What one ledger fetch produced.
#[derive(Debug, Clone)]
pub struct Fetched {
    /// The ledger that was fetched.
    pub ledger: LedgerId,
    /// The endpoint that served it.
    pub source: EndpointId,
    /// Events the source served.
    pub event_count: u64,
    /// Events this fetch newly stored.
    pub stored: u64,
    /// The head after storing.
    pub head_seq: u64,
    /// The head event after storing.
    pub head_event: mabel_core::EventId,
    /// When the source answered.
    pub fetched_at_ms: u64,
}

/// What checking a ledger against its witnesses found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// No witness holds anything this node does not.
    UpToDate,
    /// A witness was ahead and this node fast-forwarded to its head.
    FastForwarded {
        /// The head this node now holds.
        head_seq: u64,
    },
}

/// The network side of a wallet: one Iroh endpoint, dialled per request.
#[derive(Debug, Clone)]
pub struct WalletSync {
    endpoint: Endpoint,
    timeout: Duration,
}

impl WalletSync {
    /// A sync client over `endpoint`.
    #[must_use]
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            timeout: REQUEST_TIMEOUT,
        }
    }

    /// The same client with a shorter deadline, which is what a test wants.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The endpoint this wallet dials from.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Connects to one peer under the request deadline.
    ///
    /// # Errors
    ///
    /// Returns the connect error, or a timeout spelled as one.
    pub async fn connect(&self, peer: EndpointId) -> Result<Client, NetError> {
        match tokio::time::timeout(self.timeout, Client::connect(&self.endpoint, peer)).await {
            Ok(result) => result,
            Err(_) => Err(NetError::Protocol(format!(
                "no route to {} after {}s",
                ids::key(&peer),
                self.timeout.as_secs()
            ))),
        }
    }

    /// Pushes every event of `ledger` to each of `witnesses`, in order.
    ///
    /// One entry per endpoint comes back whatever happened, so a caller can
    /// report a partial failure rather than losing the successes
    /// (`contracts/http/wallet-post-sync-push.json`).
    ///
    /// # Errors
    ///
    /// Returns code 2 when `witnesses` is empty, and the errors of reading the
    /// ledger.
    pub async fn push(
        &self,
        core: &WalletCore,
        ledger: LedgerId,
        witnesses: &[EndpointId],
    ) -> Result<Pushed, ServiceError> {
        if witnesses.is_empty() {
            return Err(ServiceError::usage(
                "no_witness_configured",
                format!("ledger {ledger} names no witness to push to"),
            )
            .with_detail("ledger_id", ledger.to_string()));
        }
        let loaded = core.load(ledger)?;
        core.require_valid(&loaded)?;

        let mut results = Vec::with_capacity(witnesses.len());
        for witness in witnesses {
            results.push(self.push_one(*witness, ledger, &loaded.events).await);
        }
        Ok(Pushed {
            ledger_id: ids::identity(ledger),
            head_seq: loaded.head_seq,
            head_event: ids::event(loaded.head_event),
            results,
        })
    }

    /// One endpoint's outcome, never an error: an unreachable witness is a
    /// row in the report.
    async fn push_one(
        &self,
        witness: EndpointId,
        ledger: LedgerId,
        events: &[Vec<u8>],
    ) -> PushResult {
        let endpoint = ids::key(&witness);
        let client = match self.connect(witness).await {
            Ok(client) => client,
            Err(error) => {
                return PushResult {
                    endpoint,
                    status: PushStatus::Unreachable,
                    head_seq: None,
                    stored: 0,
                    reject_code: None,
                    at_seq: None,
                    message: Some(format!("Network error: {}", peer_message(witness, &error))),
                };
            }
        };
        let outcome = client.push(ledger, events).await;
        client.close();
        match outcome {
            Ok(outcome) => PushResult {
                endpoint,
                status: PushStatus::Accepted,
                head_seq: Some(outcome.head_seq),
                stored: u64::from(outcome.stored),
                reject_code: None,
                at_seq: None,
                message: None,
            },
            Err(error) => match rejection_of(&error) {
                Some(rejection) => PushResult {
                    endpoint,
                    status: PushStatus::Rejected,
                    head_seq: None,
                    stored: 0,
                    reject_code: Some(rejection.code.as_str_name().to_owned()),
                    at_seq: Some(rejection.at_seq),
                    message: Some(rejection.msg.clone()),
                },
                None => PushResult {
                    endpoint,
                    status: PushStatus::Unreachable,
                    head_seq: None,
                    stored: 0,
                    reject_code: None,
                    at_seq: None,
                    message: Some(format!("Network error: {}", peer_message(witness, &error))),
                },
            },
        }
    }

    /// Where one peer says a ledger ends, or `None` if it does not hold it.
    ///
    /// # Errors
    ///
    /// Returns code 30 when the peer cannot be reached or refuses the request.
    pub async fn head(
        &self,
        peer: EndpointId,
        ledger: LedgerId,
    ) -> Result<Option<Head>, ServiceError> {
        let client = self
            .connect(peer)
            .await
            .map_err(|error| unreachable(peer, &error))?;
        let head = client.head(ledger).await;
        client.close();
        head.map_err(|error| unreachable(peer, &error))
    }

    /// Every event one peer holds for `ledger`, verified from nothing.
    ///
    /// The run must fold with no violation and its ledger id must equal the
    /// one that was asked for, which a tampered inception cannot fake
    /// (proposal 001 section 3.7).
    ///
    /// # Errors
    ///
    /// Returns code 30 when the peer cannot be reached, and code 20 when what
    /// it served does not verify.
    pub async fn candidate(
        &self,
        peer: EndpointId,
        ledger: LedgerId,
    ) -> Result<Option<LoadedLedger>, ServiceError> {
        let client = self
            .connect(peer)
            .await
            .map_err(|error| unreachable(peer, &error))?;
        let served = client.get_all(ledger, 0).await;
        client.close();
        let Some(events) = served.map_err(|error| unreachable(peer, &error))? else {
            return Ok(None);
        };
        if events.is_empty() {
            return Ok(None);
        }
        Ok(Some(verified(peer, ledger, events)?))
    }

    /// Fetches a ledger from one source, verifies it from nothing and stores
    /// it under `ledgers/`.
    ///
    /// # Errors
    ///
    /// Returns code 30 when the source cannot be reached or does not hold the
    /// ledger, code 20 when what it served does not verify, and code 50 when
    /// this node's copy diverges from it.
    pub async fn fetch(
        &self,
        core: &WalletCore,
        ledger: LedgerId,
        source: EndpointId,
    ) -> Result<Fetched, ServiceError> {
        let Some(candidate) = self.candidate(source, ledger).await? else {
            return Err(ServiceError::network(
                "ledger_not_held",
                format!("{} does not hold {ledger}", ids::key(&source)),
            )
            .with_detail("ledger_id", ledger.to_string())
            .with_detail("source", ids::key(&source).as_str()));
        };
        let fetched_at_ms = crate::now_ms();
        let stored = core.store_events(ledger, &candidate.events, Some(source))?;
        Ok(Fetched {
            ledger,
            source,
            event_count: candidate.event_count(),
            stored,
            head_seq: candidate.head_seq,
            head_event: candidate.head_event,
            fetched_at_ms,
        })
    }

    /// The append discipline of proposal 001 section 5.
    ///
    /// Before appending to a ledger this wallet does not solely control, ask
    /// every configured witness where the ledger ends. A witness that is ahead
    /// on a chain extending this node's copy is fast-forwarded from. A local
    /// event that conflicts with an observed head is discarded, the observed
    /// chain is taken instead, and the caller is told to re-sign its intent on
    /// the new head: exit code 50, `stale_head`.
    ///
    /// # Errors
    ///
    /// Returns code 50 when a local event lost a race, code 20 when a witness
    /// serves a chain that does not verify, and code 30 when a witness cannot
    /// be reached.
    pub async fn ensure_fresh(
        &self,
        core: &WalletCore,
        ledger: LedgerId,
        witnesses: &[EndpointId],
    ) -> Result<Freshness, ServiceError> {
        let mut freshness = Freshness::UpToDate;
        for witness in witnesses {
            let local = core.load(ledger)?;
            let Some(head) = self.head(*witness, ledger).await? else {
                continue;
            };
            if head.head_seq <= local.head_seq
                && local
                    .event_ids
                    .get(head.head_seq as usize)
                    .copied()
                    .flatten()
                    == Some(head.head_event)
            {
                // The witness holds a prefix of what this node holds, so the
                // unpushed suffix is this node's to push.
                continue;
            }
            let Some(candidate) = self.candidate(*witness, ledger).await? else {
                continue;
            };
            let shared = shared_prefix(&local.events, &candidate.events);
            if shared == local.events.len() {
                // The local copy is a prefix of the witness's: fast-forward.
                let stored = core.store_events(ledger, &candidate.events, Some(*witness))?;
                if stored > 0 {
                    freshness = Freshness::FastForwarded {
                        head_seq: candidate.head_seq,
                    };
                }
                continue;
            }
            // A local event at `shared` is not the event the witness holds
            // there. The witness's copy is what other parties see, so the
            // local one is discarded and the intent is re-signed on the new
            // head.
            core.truncate(ledger, shared.saturating_sub(1) as u64)?;
            core.store_events(ledger, &candidate.events, Some(*witness))?;
            return Err(stale_head(ledger, local.head_seq, &head, *witness));
        }
        Ok(freshness)
    }
}

/// Folds a run a peer served and refuses anything that does not verify.
fn verified(
    peer: EndpointId,
    ledger: LedgerId,
    events: Vec<Vec<u8>>,
) -> Result<LoadedLedger, ServiceError> {
    let (state, violation) = fold(&events);
    if let Some(violation) = violation {
        return Err(ServiceError::ledger(
            violation.code(),
            format!(
                "{} served a chain for {ledger} that fails at seq {}: {}",
                ids::key(&peer),
                violation.seq,
                violation.reason
            ),
        )
        .with_detail("ledger_id", ledger.to_string())
        .with_detail("source", ids::key(&peer).as_str())
        .with_detail("failed_at_seq", violation.seq));
    }
    if state.ledger() != Some(ledger) {
        return Err(ServiceError::ledger(
            "wrong_ledger",
            format!(
                "{} served a chain whose ledger id is not {ledger}",
                ids::key(&peer)
            ),
        )
        .with_detail("ledger_id", ledger.to_string())
        .with_detail("source", ids::key(&peer).as_str()));
    }
    Ok(LoadedLedger::fold(ledger, events))
}

/// How many leading events two runs share byte for byte.
fn shared_prefix(left: &[Vec<u8>], right: &[Vec<u8>]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(one, other)| one == other)
        .count()
}

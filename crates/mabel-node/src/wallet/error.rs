//! Turning the failures a wallet meets into the one error envelope.
//!
//! [`ServiceError`] is the envelope both surfaces render: the HTTP API returns
//! it directly and the CLI copies its code, message and details (proposal 001
//! section 9, `contracts/README.md`). Everything a wallet can fail at is
//! mapped here, so no caller invents a second spelling.

use axum::http::StatusCode;
use iroh_base::EndpointId;
use mabel_core::LedgerId;
use mabel_core::fold::Reason;
use mabel_core::sign::BuildError;
use mabel_net::Error as NetError;
use mabel_net::store::Head;

use crate::api::documents::Id;
use crate::api::error::ServiceError;
use crate::error::StorageError;
use crate::wallet::ids;

/// A storage failure, carrying the code `mabel-node` assigned it.
#[must_use]
pub fn storage_error(error: StorageError) -> ServiceError {
    let message = error.to_string();
    match &error {
        StorageError::InsecurePermissions { path, mode } => ServiceError::permissions(
            "insecure_key_permissions",
            format!(
                "key file has insecure permissions: {} is mode {mode:04o}, \
                 pass --allow-insecure-permissions to continue",
                path.display()
            ),
        )
        .with_detail("path", path.display().to_string())
        .with_detail("mode", format!("{mode:04o}"))
        .with_detail("expected_mode", "0600"),
        StorageError::HomeUnknown | StorageError::NotAHome { .. } => {
            ServiceError::usage("no_node_home", message)
        }
        StorageError::Json { .. }
        | StorageError::MalformedKey { .. }
        | StorageError::MalformedEvent { .. } => ServiceError::schema("malformed_file", message),
        StorageError::UnknownIdentity { identity } => {
            ServiceError::usage("unknown_identity", message)
                .with_detail("identity", identity.to_string())
                .with_status(StatusCode::NOT_FOUND)
        }
        StorageError::MissingEvent { .. } | StorageError::EventIdMismatch { .. } => {
            ServiceError::ledger("missing_event", message)
        }
        StorageError::OutOfOrderAppend { .. } => {
            ServiceError::state("out_of_order_append", message)
        }
        _ => ServiceError::state("storage_unavailable", message)
            .with_status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// A rejection from the fold, which is the authority on why an event is not
/// allowed.
#[must_use]
pub fn fold_error(reason: &Reason) -> ServiceError {
    let message = reason.to_string();
    match reason {
        Reason::Wire(_) => ServiceError::schema(reason.code(), message),
        Reason::WrongSeq { .. }
        | Reason::WrongLedger { .. }
        | Reason::BrokenPrevLink { .. }
        | Reason::BackwardsTimestamp { .. }
        | Reason::PayloadNotAllowed { .. }
        | Reason::InvalidPublicKey { .. }
        | Reason::UnauthorizedSigner { .. }
        | Reason::BadSignature => ServiceError::ledger(reason.code(), message),
        _ => ServiceError::policy(reason.code(), message),
    }
}

/// A refusal from the signing path, which checks the byte-layout caps.
#[must_use]
pub fn build_error(error: &BuildError) -> ServiceError {
    ServiceError::schema("event_not_buildable", error.to_string())
}

/// No source answered for a ledger: code 30, the fixture's wording
/// (`contracts/http/wallet-post-verify.json`).
#[must_use]
pub fn no_source_available(ledger: LedgerId, queried: &[EndpointId]) -> ServiceError {
    ServiceError::network(
        "no_source_available",
        format!("no source answered for {ledger}"),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("sources_queried", rendered(queried))
}

/// A peer that could not be dialled or did not answer: code 30.
#[must_use]
pub fn unreachable(endpoint: EndpointId, error: &NetError) -> ServiceError {
    ServiceError::network("peer_unreachable", peer_message(endpoint, error))
        .with_detail("endpoint", ids::key(&endpoint).as_str())
        .with_detail("error", error.to_string())
}

/// The one line a person reads about a peer that did not answer.
#[must_use]
pub fn peer_message(endpoint: EndpointId, error: &NetError) -> String {
    let endpoint = ids::key(&endpoint);
    match error {
        NetError::Connect { .. } => format!("no route to {endpoint}: {error}"),
        other => format!("{endpoint} did not answer: {other}"),
    }
}

/// The remote holds a head this node's copy does not extend: code 50, the
/// wording of `contracts/http/wallet-post-sync-push.json`.
#[must_use]
pub fn stale_head(
    ledger: LedgerId,
    local: u64,
    observed: &Head,
    source: EndpointId,
) -> ServiceError {
    ServiceError::state(
        "stale_head",
        format!(
            "witness {} reports head seq {}, this node holds seq {local}",
            ids::key(&source),
            observed.head_seq
        ),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("local_head_seq", local)
    .with_detail("observed_head_seq", observed.head_seq)
    .with_detail("source", ids::key(&source).as_str())
}

/// One side of an equivocation: a source and the event it holds at the
/// sequence where the two disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergent {
    /// The endpoint that served this event.
    pub source: EndpointId,
    /// The event it holds there.
    pub event: Id,
}

/// Two sources hold different valid events at one sequence: code 20
/// (proposal 001 section 3.7).
#[must_use]
pub fn equivocation(
    ledger: LedgerId,
    at_seq: u64,
    first: &Divergent,
    second: &Divergent,
) -> ServiceError {
    let candidate = |side: &Divergent| {
        serde_json::json!({
            "source": ids::key(&side.source).as_str(),
            "event_id": side.event.as_str(),
        })
    };
    ServiceError::ledger(
        "equivocation",
        format!("two sources hold divergent events at seq {at_seq} of {ledger}"),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("at_seq", at_seq)
    .with_detail("candidates", vec![candidate(first), candidate(second)])
}

/// Every endpoint id as a document spells it.
#[must_use]
pub fn rendered(endpoints: &[EndpointId]) -> Vec<String> {
    endpoints
        .iter()
        .map(|endpoint| ids::key(endpoint).as_str().to_owned())
        .collect()
}

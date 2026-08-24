//! The node routes, under `/api` (proposal 001 section 10, proposal 006
//! section 8).
//!
//! One router, served by every node. Nothing here is gated on what the home
//! holds: a home with no identities answers `{"ok": true, "identities": []}`,
//! which is emptiness and not a refusal, and a mutating route naming a ledger
//! this home holds but cannot append to answers `no_local_signer`.
//!
//! Every handler validates, calls one [`NodeService`] method and renders.
//! Nothing else: the node decides, and the UI holds no keys and does no
//! crypto, so no route serves raw event bytes.
//!
//! A static segment matches before `{identity_id}`, and an identity id is 52
//! base32 characters, so no id collides with `known`, `holdings`, `endpoints`,
//! `witnesses`, `fetch`, `ledger` or `keys`.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query as AxumQuery, State};
use axum::http::Uri;
use axum::response::Response;
use axum::routing::{get, post};

use super::documents::{IdentityList, IdentityView};
use super::error::ServiceError;
use super::parse::{self, IdKind};
use super::service::NodeService;
use super::{Query, query, success};

type Service = Arc<dyn NodeService>;

/// Every node route, to be nested under `/api`.
pub(super) fn router(service: Service) -> Router {
    Router::new()
        .route("/node", get(node))
        .route("/identities", get(identities).post(create_identity))
        .route("/identities/known", get(known_identities))
        .route("/identities/{identity_id}", get(identity))
        .route("/identities/{identity_id}/ledger", get(identity_ledger))
        .route("/identities/{identity_id}/keys", get(identity_keys))
        .route("/identities/{identity_id}/profile", post(replace_profile))
        .route(
            "/identities/{identity_id}/verification",
            post(check_verification),
        )
        .route(
            "/identities/{identity_id}/contact",
            get(contact).put(set_contact),
        )
        .route("/identities/{identity_id}/witnesses", post(set_witnesses))
        .route("/identities/{identity_id}/endpoints", post(set_endpoints))
        .route("/identities/{identity_id}/fetch", post(fetch_identity))
        .route("/lookup/{identity_id}", get(lookup))
        // A query parameter, not a path segment: a link carries `://` and `?`
        // (proposal 006 section 7).
        .route("/resolve", get(resolve))
        .route("/witnesses", get(witnesses))
        // The last segment changed with the key: an endpoint id and an identity
        // id are both 52 base32 characters, so a client still sending an
        // endpoint id gets a 404 rather than a dial that finds nothing
        // (proposal 006 section 8).
        .route("/witnesses/{identity_id}/holdings", get(witness_holdings))
        .route("/forks", get(forks))
        .route("/graph", get(graph))
        .route("/graph/sync", post(sync_graph))
        .route("/identities/{identity_id}/memberships", get(memberships))
        .route(
            "/identities/{identity_id}/memberships/invitations",
            post(invite),
        )
        .route(
            "/identities/{identity_id}/memberships/acceptances",
            post(accept_invitation),
        )
        .route(
            "/identities/{identity_id}/memberships/admissions",
            post(admit_acceptance),
        )
        .route(
            "/identities/{identity_id}/memberships/removals",
            post(remove_membership),
        )
        .route("/trust", post(add_trust))
        .route("/trust/{event_id}/revoke", post(revoke_trust))
        .route("/sync/push", post(push))
        .with_state(service)
}

async fn node(State(service): State<Service>) -> Result<Response, ServiceError> {
    Ok(success(service.node().await?))
}

async fn identities(State(service): State<Service>) -> Result<Response, ServiceError> {
    let identities = service.identities().await?;
    Ok(success(IdentityList { identities }))
}

async fn create_identity(
    State(service): State<Service>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let request = parse::create_identity(&body)?;
    Ok(success(service.create_identity(request).await?))
}

async fn known_identities(
    State(service): State<Service>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let page = parse::known_page(&query(parameters)?)?;
    Ok(success(service.known_identities(page).await?))
}

async fn identity(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let identity = service.identity(identity_id).await?;
    Ok(success(IdentityView { identity }))
}

async fn identity_ledger(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let page = parse::event_page(&query(parameters)?)?;
    Ok(success(service.identity_ledger(identity_id, page).await?))
}

async fn identity_keys(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    Ok(success(service.identity_keys(identity_id).await?))
}

async fn replace_profile(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::replace_profile(identity_id, &body)?;
    Ok(success(service.replace_profile(request).await?))
}

async fn check_verification(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    Ok(success(service.check_verification(identity_id).await?))
}

async fn contact(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    Ok(success(service.contact(identity_id).await?))
}

async fn set_contact(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::set_contact(identity_id, &body)?;
    Ok(success(service.set_contact(request).await?))
}

async fn fetch_identity(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::fetch_identity(identity_id, &body)?;
    Ok(success(service.fetch_identity(request).await?))
}

async fn lookup(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::lookup(identity_id, &query(parameters)?)?;
    Ok(success(service.lookup(request).await?))
}

/// `GET /api/resolve?input=`, taking an identity id, a hostname or a link.
///
/// The raw URI, not the `Query` extractor: the parameter is decoded exactly
/// once by the parser below, and a repeated `input` has to be visible to be
/// refused (proposal 006 section 7). The route writes nothing and touches no
/// verification cache: navigation is not verification.
async fn resolve(State(service): State<Service>, uri: Uri) -> Result<Response, ServiceError> {
    let input = parse::resolve(uri.query())?;
    Ok(success(service.resolve(input).await?))
}

async fn witnesses(State(service): State<Service>) -> Result<Response, ServiceError> {
    Ok(success(service.witnesses().await?))
}

/// `GET /api/witnesses/{identity_id}/holdings`, the ledgers that witness
/// identity keeps (proposal 006 section 8).
async fn witness_holdings(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let page = parse::ledger_page(&query(parameters)?)?;
    Ok(success(service.witness_holdings(identity_id, page).await?))
}

/// `GET /api/forks`, on every node: a fork is a fact about a stored ledger and
/// no other route reports it (proposal 006 section 8).
async fn forks(
    State(service): State<Service>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let request = parse::fork_query(&query(parameters)?)?;
    Ok(success(service.forks(request).await?))
}

async fn graph(State(service): State<Service>) -> Result<Response, ServiceError> {
    Ok(success(service.graph().await?))
}

async fn sync_graph(State(service): State<Service>) -> Result<Response, ServiceError> {
    Ok(success(service.sync_graph().await?))
}

async fn set_witnesses(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let witnesses = parse::witnesses(&body)?;
    Ok(success(
        service.set_witnesses(identity_id, witnesses).await?,
    ))
}

/// The endpoints that answer for this identity, replaced whole (proposal 006
/// section 2).
async fn set_endpoints(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let endpoints = parse::endpoints(&body)?;
    Ok(success(
        service.set_endpoints(identity_id, endpoints).await?,
    ))
}

async fn memberships(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    Ok(success(service.memberships(identity_id).await?))
}

async fn invite(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let ledger_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::invite(ledger_id, &body)?;
    Ok(success(service.invite(request).await?))
}

async fn accept_invitation(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::accept_invitation(identity_id, &body)?;
    Ok(success(service.accept_invitation(request).await?))
}

async fn admit_acceptance(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let ledger_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::admit_acceptance(ledger_id, &body)?;
    Ok(success(service.admit_acceptance(request).await?))
}

async fn remove_membership(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let ledger_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::remove_membership(ledger_id, &body)?;
    Ok(success(service.remove_membership(request).await?))
}

async fn add_trust(State(service): State<Service>, body: Bytes) -> Result<Response, ServiceError> {
    let request = parse::add_trust(&body)?;
    Ok(success(service.add_trust(request).await?))
}

async fn revoke_trust(
    State(service): State<Service>,
    Path(event_id): Path<String>,
    body: Bytes,
) -> Result<Response, ServiceError> {
    let event_id = parse::id(IdKind::Event, &event_id)?;
    let issuer = parse::revoke(&body)?;
    Ok(success(service.revoke_trust(event_id, issuer).await?))
}

async fn push(State(service): State<Service>, body: Bytes) -> Result<Response, ServiceError> {
    let request = parse::push(&body)?;
    Ok(success(service.push(request).await?))
}

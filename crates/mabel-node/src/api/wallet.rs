//! The wallet routes, under `/api` (proposal 001 section 10).
//!
//! Every handler here validates, calls one [`WalletService`] method and
//! renders. Nothing else: the node decides, and the UI holds no keys and does
//! no crypto, so no route serves raw event bytes.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query as AxumQuery, State};
use axum::response::Response;
use axum::routing::{get, post};

use super::documents::{IdentityList, IdentityView};
use super::error::ServiceError;
use super::parse::{self, IdKind};
use super::service::WalletService;
use super::{Query, query, success};

type Service = Arc<dyn WalletService>;

/// Every wallet route, to be nested under `/api`.
pub(super) fn router(service: Service) -> Router {
    Router::new()
        .route("/node", get(node))
        .route("/identities", get(identities).post(create_identity))
        .route("/identities/{identity_id}", get(identity))
        .route("/identities/{identity_id}/ledger", get(identity_ledger))
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
        .route("/lookup/{identity_id}", get(lookup))
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
        .route("/verify", post(verify))
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

async fn lookup(
    State(service): State<Service>,
    Path(identity_id): Path<String>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let identity_id = parse::id(IdKind::Identity, &identity_id)?;
    let request = parse::lookup(identity_id, &query(parameters)?)?;
    Ok(success(service.lookup(request).await?))
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

async fn verify(State(service): State<Service>, body: Bytes) -> Result<Response, ServiceError> {
    let request = parse::verify(&body)?;
    Ok(success(service.verify(request).await?))
}

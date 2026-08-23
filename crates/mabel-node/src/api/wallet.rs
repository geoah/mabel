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
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
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
        .route("/identities/{identity_id}/witnesses", post(set_witnesses))
        .route(
            "/identities/{identity_id}/memberships/invitations",
            post(membership),
        )
        .route(
            "/identities/{identity_id}/memberships/acceptances",
            post(membership),
        )
        .route(
            "/identities/{identity_id}/memberships/removals",
            post(membership),
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

/// The three membership routes, until proposal 002 freezes them.
async fn membership(uri: Uri) -> Response {
    parse::pending_membership(uri.path()).into_response()
}

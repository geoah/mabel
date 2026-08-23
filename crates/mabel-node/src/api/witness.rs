//! The witness routes, under `/api` (proposal 001 section 10).
//!
//! Read-only: a witness signs nothing and holds no identity keys, so there is
//! no route that appends. Events arrive over Iroh, never over HTTP.

use std::sync::Arc;

use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query as AxumQuery, State};
use axum::response::Response;
use axum::routing::get;

use super::error::ServiceError;
use super::parse::{self, IdKind};
use super::service::WitnessService;
use super::{Query, query, success};

type Service = Arc<dyn WitnessService>;

/// Every witness route, to be nested under `/api`.
pub(super) fn router(service: Service) -> Router {
    Router::new()
        .route("/node", get(node))
        .route("/ledgers", get(ledgers))
        .route("/ledgers/{ledger_id}", get(ledger))
        .route("/ledgers/{ledger_id}/events", get(ledger_events))
        .route("/forks", get(forks))
        .with_state(service)
}

async fn node(State(service): State<Service>) -> Result<Response, ServiceError> {
    Ok(success(service.node().await?))
}

async fn ledgers(
    State(service): State<Service>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let page = parse::ledger_page(&query(parameters)?)?;
    Ok(success(service.ledgers(page).await?))
}

async fn ledger(
    State(service): State<Service>,
    Path(ledger_id): Path<String>,
) -> Result<Response, ServiceError> {
    let ledger_id = parse::id(IdKind::Ledger, &ledger_id)?;
    Ok(success(service.ledger(ledger_id).await?))
}

async fn ledger_events(
    State(service): State<Service>,
    Path(ledger_id): Path<String>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let ledger_id = parse::id(IdKind::Ledger, &ledger_id)?;
    let page = parse::event_page(&query(parameters)?)?;
    Ok(success(service.ledger_events(ledger_id, page).await?))
}

async fn forks(
    State(service): State<Service>,
    parameters: Result<AxumQuery<Query>, QueryRejection>,
) -> Result<Response, ServiceError> {
    let request = parse::fork_query(&query(parameters)?)?;
    Ok(success(service.forks(request).await?))
}

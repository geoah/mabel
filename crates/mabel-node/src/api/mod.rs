//! The loopback HTTP API of both node roles (proposal 001 section 10, ticket
//! 012).
//!
//! [`wallet_router`] and [`witness_router`] serve every route indexed by
//! `contracts/README.md`, in the shapes the fixtures under `contracts/http/`
//! freeze. Handlers validate the request, call one method of
//! [`WalletService`] or [`WitnessService`], and render the answer; they hold
//! no node state and reach no storage, so the runtimes of tickets 010 and 011
//! decide everything and this layer decides nothing (proposal 001 section 10).
//! [`stub`] implements both traits from the fixtures, which is what the
//! contract tests and the UI work of ticket 013 run against.
//!
//! ```no_run
//! use std::sync::Arc;
//! use mabel_node::api::{ApiOptions, bind, stub::StubWalletService, wallet_router};
//!
//! # async fn serve() -> anyhow::Result<()> {
//! let options = ApiOptions::default();
//! let service = Arc::new(StubWalletService::new());
//! let router = wallet_router(service, &options);
//! let bound = bind::bind(options.http_bind).await?;
//! axum::serve(bound.listener, router).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Deviations from the fixtures
//!
//! - Admitting an acceptance is `POST
//!   /api/identities/{identity_id}/memberships/admissions`. Proposal 002
//!   section 6 names three membership routes and leaves the fourth verb
//!   unnamed; accepting an invitation you received and admitting someone
//!   else's acceptance run on different wallets and get different paths
//!   (`contracts/README.md`, "Membership").
//! - `POST /api/verify` with `kind: "ledger"` names its ledger in `ledger_id`.
//!   The fixture pins the `trust` body only, where the ledger is the `issuer`.
//! - An unknown query parameter is refused with code 2, matching the
//!   "unknown route or parameter" row of the table in `contracts/README.md`.
//! - `limit` above a route's maximum is clamped, not refused; the response
//!   echoes the effective limit. Only an unparseable or zero `limit` is an
//!   error, which is what the fixture pins.

pub mod bind;
pub mod documents;
pub mod error;
pub mod loopback;
mod parse;
pub mod service;
pub mod stub;
mod ui;
mod wallet;
mod witness;

use std::sync::Arc;

use axum::extract::Query as AxumQuery;
use axum::extract::rejection::QueryRejection;
use axum::http::{Method, Uri};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::Serialize;

pub use bind::{DEFAULT_HTTP_BIND, DEFAULT_HTTP_PORT, HttpBind, non_loopback_warning};
pub use documents::Id;
pub use error::{ErrorLayer, ServiceError};
pub use loopback::LoopbackRules;
pub use service::{WalletService, WitnessService};
pub use ui::UiSource;

use error::ServiceError as Error;
use parse::Query;

/// What the routers need besides a service: where the API is bound, which
/// fixes the `Host` and `Origin` the loopback rules demand, and where the UI
/// bundle comes from.
#[derive(Debug, Clone)]
pub struct ApiOptions {
    /// The address the HTTP API listens on.
    pub http_bind: std::net::SocketAddr,
    /// The UI bundle, embedded unless `--ui-dir` overrides it.
    pub ui: UiSource,
}

impl Default for ApiOptions {
    fn default() -> Self {
        Self {
            http_bind: DEFAULT_HTTP_BIND,
            ui: UiSource::default(),
        }
    }
}

impl ApiOptions {
    /// Options for an API bound to `http_bind`, serving the embedded UI.
    #[must_use]
    pub fn new(http_bind: std::net::SocketAddr) -> Self {
        Self {
            http_bind,
            ui: UiSource::default(),
        }
    }

    /// Serves the UI from this source instead of the embedded bundle.
    #[must_use]
    pub fn with_ui(mut self, ui: UiSource) -> Self {
        self.ui = ui;
        self
    }

    /// The rules a request must satisfy to reach a handler.
    #[must_use]
    pub fn loopback_rules(&self) -> LoopbackRules {
        LoopbackRules::new(self.http_bind.port())
    }
}

/// The wallet API, the UI and the loopback rules, as one router.
pub fn wallet_router(service: Arc<dyn WalletService>, options: &ApiOptions) -> Router {
    assemble(Router::new().nest("/api", wallet::router(service)), options)
}

/// The witness API, the UI and the loopback rules, as one router.
///
/// The witness API is read-only: every route is a `GET`, and a mutating
/// request to one of them answers 405 with the error envelope.
pub fn witness_router(service: Arc<dyn WitnessService>, options: &ApiOptions) -> Router {
    assemble(
        Router::new().nest("/api", witness::router(service)),
        options,
    )
}

fn assemble(router: Router, options: &ApiOptions) -> Router {
    let ui = options.ui.clone();
    router
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(move |method: Method, uri: Uri| {
            let ui = ui.clone();
            async move { fallback(ui, &method, &uri).await }
        })
        .layer(middleware::from_fn_with_state(
            options.loopback_rules(),
            loopback::enforce,
        ))
}

async fn method_not_allowed(method: Method, uri: Uri) -> Response {
    parse::method_not_allowed(method.as_str(), uri.path()).into_response()
}

/// Anything no route claims: a 404 envelope under `/api`, the UI bundle
/// everywhere else.
async fn fallback(ui: UiSource, method: &Method, uri: &Uri) -> Response {
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        return parse::unknown_route(method.as_str(), path).into_response();
    }
    if !matches!(*method, Method::GET | Method::HEAD) {
        return parse::method_not_allowed(method.as_str(), path).into_response();
    }
    match ui::serve(&ui, path).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

/// A success document: `ok: true` and the payload flat beside it.
#[derive(Debug, Serialize)]
struct Success<T> {
    ok: bool,
    #[serde(flatten)]
    payload: T,
}

/// Renders a success document.
fn success<T: Serialize>(payload: T) -> Response {
    Json(Success { ok: true, payload }).into_response()
}

/// The query string, or code 2 when it is not even parseable as one.
fn query(result: Result<AxumQuery<Query>, QueryRejection>) -> Result<Query, Error> {
    match result {
        Ok(AxumQuery(query)) => Ok(query),
        Err(rejection) => Err(Error::usage(
            "malformed_query_parameter",
            "the query string could not be read",
        )
        .with_detail("error", rejection.body_text())),
    }
}

#[cfg(test)]
mod tests;

//! The loopback HTTP API, one router served by every node (proposal 001
//! section 10, proposal 006 section 8, ticket 012).
//!
//! [`node_router`] serves every route indexed by `contracts/README.md`, in the
//! shapes the fixtures under `contracts/http/` freeze. Handlers validate the
//! request, call one method of [`NodeService`], and render the answer; they
//! hold no node state and reach no storage, so the runtime decides everything
//! and this layer decides nothing. [`stub`] implements the trait from the
//! fixtures, which is what the contract tests and the UI work of ticket 013
//! run against.
//!
//! ```no_run
//! use std::sync::Arc;
//! use mabel_node::api::{ApiOptions, bind, node_router, stub::StubNodeService};
//!
//! # async fn serve() -> anyhow::Result<()> {
//! let options = ApiOptions::default();
//! let service = Arc::new(StubNodeService::new());
//! let router = node_router(service, &options);
//! let bound = bind::bind(options.http_bind).await?;
//! axum::serve(bound.listener, router).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Where the routes depart from the proposals
//!
//! The fixtures are normative and the handlers match them. Each ruling below
//! is also recorded in `contracts/README.md`, "Decisions taken here".
//!
//! - Admitting an acceptance is `POST
//!   /api/identities/{identity_id}/memberships/admissions`. Proposal 002
//!   section 6 names three membership routes and leaves the fourth verb
//!   unnamed; accepting an invitation you received and admitting someone
//!   else's acceptance run on different wallets and get different paths
//!   (`contracts/README.md`, "Membership").
//! - There is no `POST /api/verify`. Proposal 004 removed it with the verify
//!   tab, and `mabel verify trust|ledger` runs over the wallet core with no
//!   HTTP in the path.
//! - `GET /api/witnesses/{identity_id}/holdings` names its array `ledgers` and
//!   carries neither `first_seen_ms`, `forks_truncated` nor `source_endpoint`:
//!   those come from the answering node's own `meta.json` rather than from the
//!   `List` answer.
//! - `GET /api/resolve?input=` never reads or writes the verification cache.
//!   Its four statuses are navigation, not the five-status verdict of proposal
//!   003 section 2, and it takes an identity id, a hostname or a `mabel://`
//!   link (proposal 006 section 7).
//! - An unknown query parameter is refused with code 2, matching the
//!   "unknown route or parameter" row of the table in `contracts/README.md`.
//! - `limit` above a route's maximum is clamped, not refused; the response
//!   echoes the effective limit. Only an unparseable or zero `limit` is an
//!   error, which is what the fixture pins.
//! - The contact routes are `GET` and `PUT
//!   /api/identities/{identity_id}/contact`, the fixture names proposal 003
//!   section 5 lists. `PUT` is the only non-`POST` mutating verb here; the
//!   loopback rules treat it exactly as they treat `POST`.
//! - `ResolvedIdentity` spells its verdict `verification_status` and carries
//!   the status string alone. Proposal 003 section 4 writes the key as
//!   `verification`, which would put six timestamps in every path hop.
//! - The identity document's `verification` carries `unreachable`, the failed
//!   re-check proposal 003 section 2 requires the document to report beside a
//!   decisive result. Section 5 does not list the key.
//! - `no_op_profile_update` is code 20 with the `Policy error:` prefix at 409:
//!   a semantic rule the node enforces before signing, which is the row code
//!   20 already names.
//! - `GET /api/lookup/{identity_id}` defaults `from` to the lowest local
//!   identity id. Proposal 003 section 3 defaults it to the identity selected
//!   in the wallet, which is a browser fact the node does not hold.

pub mod bind;
pub mod documents;
pub mod error;
pub mod loopback;
mod parse;
mod routes;
pub mod service;
pub mod stub;
mod ui;

use std::sync::Arc;

use axum::extract::Query as AxumQuery;
use axum::extract::Request;
use axum::extract::rejection::QueryRejection;
use axum::http::{HeaderMap, HeaderValue, Method, Uri, header};
use axum::middleware;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::compression::CompressionLayer;

pub use bind::{DEFAULT_HTTP_BIND, DEFAULT_HTTP_PORT, HttpBind, non_loopback_warning};
pub use documents::Id;
pub use error::{ErrorLayer, ServiceError};
pub use loopback::LoopbackRules;
pub use service::NodeService;
pub use ui::UiSource;

use error::ServiceError as Error;
use parse::Query;

/// What the routers need besides a service: where the API is bound, which
/// fixes the `Host` and `Origin` the loopback rules demand, which further hosts
/// the operator allowed, and where the UI bundle comes from.
#[derive(Debug, Clone)]
pub struct ApiOptions {
    /// The address the HTTP API listens on.
    pub http_bind: std::net::SocketAddr,
    /// The UI bundle, embedded unless `--ui-dir` overrides it.
    pub ui: UiSource,
    /// `Host` values accepted beyond loopback (decision 018). Empty by
    /// default.
    pub allowed_hosts: Vec<String>,
}

impl Default for ApiOptions {
    fn default() -> Self {
        Self {
            http_bind: DEFAULT_HTTP_BIND,
            ui: UiSource::default(),
            allowed_hosts: Vec::new(),
        }
    }
}

impl ApiOptions {
    /// Options for an API bound to `http_bind`, serving the embedded UI to
    /// loopback alone.
    #[must_use]
    pub fn new(http_bind: std::net::SocketAddr) -> Self {
        Self {
            http_bind,
            ui: UiSource::default(),
            allowed_hosts: Vec::new(),
        }
    }

    /// Serves the UI from this source instead of the embedded bundle.
    #[must_use]
    pub fn with_ui(mut self, ui: UiSource) -> Self {
        self.ui = ui;
        self
    }

    /// Adds `Host` values the API accepts beyond loopback.
    ///
    /// Adds rather than replaces: `node.json`'s `allowed_hosts` and the
    /// `--allow-host` flags both call this, and a one-off flag must not drop
    /// the set the file records (decision 018).
    #[must_use]
    pub fn with_allowed_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_hosts.extend(hosts.into_iter().map(Into::into));
        self
    }

    /// The rules a request must satisfy to reach a handler.
    #[must_use]
    pub fn loopback_rules(&self) -> LoopbackRules {
        LoopbackRules::new(self.http_bind.port()).with_allowed_hosts(&self.allowed_hosts)
    }
}

/// The node API, the UI and the loopback rules, as one router.
///
/// Every node serves this, whether it signs, witnesses or both: no route is
/// gated on what the home holds (proposal 006 section 8).
pub fn node_router(service: Arc<dyn NodeService>, options: &ApiOptions) -> Router {
    // The compression layer is scoped to these routes and no others. Over the
    // UI it would re-encode a file that shipped no precompressed sibling and
    // leave `api::ui`'s validator, which is the hash of the bytes that module
    // chose, on bytes the layer produced instead (issue 043).
    let api = routes::router(service).layer(CompressionLayer::new());
    assemble(Router::new().nest("/api", api), options)
}

fn assemble(router: Router, options: &ApiOptions) -> Router {
    let ui = options.ui.clone();
    router
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(
            move |method: Method, uri: Uri, request_headers: HeaderMap| {
                let ui = ui.clone();
                async move { fallback(ui, &method, &uri, &request_headers).await }
            },
        )
        // Outside the compression layer, so it sees the finished response, and
        // inside the loopback layer, so a rejection envelope is byte for byte
        // what it was before either of these existed.
        .layer(middleware::from_fn(no_store_on_the_api))
        .layer(middleware::from_fn_with_state(
            options.loopback_rules(),
            loopback::enforce,
        ))
}

/// `Cache-Control: no-store` on every `/api` answer.
///
/// A wallet document is one person's address book read over a connection an
/// operator may have put a reverse proxy in front of (decision 018). Without a
/// caching rule an intermediary may serve one from its heuristics, which shows
/// a reader an identity as it was rather than as it is. The UI bundle sets its
/// own rules and is left alone.
async fn no_store_on_the_api(request: Request, next: Next) -> Response {
    let is_api = {
        let path = request.uri().path();
        path == "/api" || path.starts_with("/api/")
    };
    let mut response = next.run(request).await;
    if is_api {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

async fn method_not_allowed(method: Method, uri: Uri) -> Response {
    parse::method_not_allowed(method.as_str(), uri.path()).into_response()
}

/// Anything no route claims: a 404 envelope under `/api`, the UI bundle
/// everywhere else.
async fn fallback(
    ui: UiSource,
    method: &Method,
    uri: &Uri,
    request_headers: &HeaderMap,
) -> Response {
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        return parse::unknown_route(method.as_str(), path).into_response();
    }
    if !matches!(*method, Method::GET | Method::HEAD) {
        return parse::method_not_allowed(method.as_str(), path).into_response();
    }
    match ui::serve(&ui, path, request_headers).await {
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

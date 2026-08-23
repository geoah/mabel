//! The three loopback rules, as one middleware layer (proposal 001 section
//! 10).
//!
//! There is no authentication. What protects a keyholding daemon on loopback
//! is that a page the user happens to have open cannot reach it: `Host` blocks
//! DNS rebinding, `Origin` blocks drive-by form posts, and the content type
//! blocks the form post that a `<form>` element can make without CORS.
//!
//! All three reject with code 2 and no layer prefix: 403 for a bad `Host` or
//! `Origin`, 415 for the content type (`contracts/README.md`).

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ServiceError;

/// The rules, parameterized by the port the API is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackRules {
    port: u16,
}

impl LoopbackRules {
    /// Rules for an API listening on `port`.
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    /// The port a request's `Host` and `Origin` must name.
    #[must_use]
    pub const fn port(self) -> u16 {
        self.port
    }

    /// Whether the method may change state, which is what the `Origin` and
    /// content-type rules apply to.
    #[must_use]
    pub fn is_mutating(method: &Method) -> bool {
        !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
    }

    fn hosts(self) -> [String; 2] {
        [
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ]
    }

    fn origins(self) -> [String; 2] {
        let [loopback, localhost] = self.hosts();
        [format!("http://{loopback}"), format!("http://{localhost}")]
    }

    fn accepts_host(self, host: &str) -> bool {
        let host = host.trim().to_ascii_lowercase();
        if self.hosts().contains(&host) {
            return true;
        }
        self.port == 80 && (host == "127.0.0.1" || host == "localhost")
    }

    /// Applies the three rules to one request.
    ///
    /// # Errors
    ///
    /// Returns the rejection to render: 403 for a `Host` that is not loopback
    /// or an `Origin` that does not match it, 415 for a mutating request whose
    /// content type is not `application/json`.
    pub fn check(self, method: &Method, headers: &HeaderMap) -> Result<(), ServiceError> {
        let [loopback, localhost] = self.hosts();
        let host_message =
            format!("request rejected: Host header must be {loopback} or {localhost}");
        match text(headers, &header::HOST) {
            None => {
                return Err(rejected(
                    "host_missing",
                    &host_message,
                    StatusCode::FORBIDDEN,
                ));
            }
            Some(host) if !self.accepts_host(&host) => {
                return Err(
                    rejected("host_not_loopback", &host_message, StatusCode::FORBIDDEN)
                        .with_detail("host", host),
                );
            }
            Some(_) => {}
        }

        if !Self::is_mutating(method) {
            return Ok(());
        }

        let [http_loopback, http_localhost] = self.origins();
        let origin_message = format!(
            "request rejected: Origin must be {http_loopback} or {http_localhost} on a mutating route"
        );
        match text(headers, &header::ORIGIN) {
            None => {
                return Err(rejected(
                    "origin_missing",
                    &origin_message,
                    StatusCode::FORBIDDEN,
                ));
            }
            Some(origin) if !self.origins().contains(&origin.to_ascii_lowercase()) => {
                return Err(
                    rejected("origin_mismatch", &origin_message, StatusCode::FORBIDDEN)
                        .with_detail("origin", origin),
                );
            }
            Some(_) => {}
        }

        let type_message =
            "request rejected: mutating routes require content-type: application/json";
        match text(headers, &header::CONTENT_TYPE) {
            None => Err(rejected(
                "content_type_missing",
                type_message,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            )),
            Some(content_type) if !is_json(&content_type) => Err(rejected(
                "content_type_not_json",
                type_message,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            )
            .with_detail("content_type", content_type)),
            Some(_) => Ok(()),
        }
    }
}

fn rejected(reason: &str, message: &str, status: StatusCode) -> ServiceError {
    ServiceError::usage(reason, message).with_status(status)
}

/// A header as text, treating bytes that are not valid UTF-8 as the lossy
/// rendering so the rejection can still name what arrived.
fn text(headers: &HeaderMap, name: &HeaderName) -> Option<String> {
    headers.get(name).map(|value| {
        value.to_str().map_or_else(
            |_| String::from_utf8_lossy(value.as_bytes()).into_owned(),
            ToOwned::to_owned,
        )
    })
}

fn is_json(content_type: &str) -> bool {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json"
}

/// The middleware `Router::layer` applies to every route, the fallback
/// included.
pub async fn enforce(State(rules): State<LoopbackRules>, request: Request, next: Next) -> Response {
    match rules.check(request.method(), request.headers()) {
        Ok(()) => next.run(request).await,
        Err(rejection) => rejection.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{LoopbackRules, StatusCode};
    use axum::http::{HeaderMap, HeaderValue, Method, header};

    const PORT: u16 = 9080;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(name.clone(), HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    fn rules() -> LoopbackRules {
        LoopbackRules::new(PORT)
    }

    #[test]
    fn a_get_from_either_loopback_spelling_is_accepted() {
        for host in ["127.0.0.1:9080", "localhost:9080", "LOCALHOST:9080"] {
            let headers = headers(&[(header::HOST, host)]);
            assert!(rules().check(&Method::GET, &headers).is_ok(), "{host}");
        }
    }

    #[test]
    fn a_host_that_is_not_loopback_is_rejected_with_403() {
        for host in ["evil.example", "localhost.example", "127.0.0.1:9999"] {
            let headers = headers(&[(header::HOST, host)]);
            let error = rules().check(&Method::GET, &headers).expect_err(host);
            assert_eq!(error.status(), StatusCode::FORBIDDEN, "{host}");
            assert_eq!(error.reason(), "host_not_loopback", "{host}");
            assert_eq!(error.code(), 2, "{host}");
        }
    }

    #[test]
    fn an_absent_host_is_rejected() {
        let error = rules()
            .check(&Method::GET, &HeaderMap::new())
            .expect_err("no host");
        assert_eq!(error.reason(), "host_missing");
    }

    #[test]
    fn a_mutating_request_needs_a_matching_origin() {
        let base = [(header::HOST, "127.0.0.1:9080")];
        let error = rules()
            .check(&Method::POST, &headers(&base))
            .expect_err("no origin");
        assert_eq!(error.reason(), "origin_missing");
        assert_eq!(error.status(), StatusCode::FORBIDDEN);

        let mismatched = headers(&[
            base[0].clone(),
            (header::ORIGIN, "https://attacker.example"),
        ]);
        let error = rules()
            .check(&Method::POST, &mismatched)
            .expect_err("wrong origin");
        assert_eq!(error.reason(), "origin_mismatch");

        let wrong_port = headers(&[base[0].clone(), (header::ORIGIN, "http://localhost:9999")]);
        assert_eq!(
            rules()
                .check(&Method::POST, &wrong_port)
                .expect_err("wrong port")
                .reason(),
            "origin_mismatch"
        );
    }

    #[test]
    fn a_mutating_request_needs_a_json_content_type() {
        let with = |content_type: &str| {
            headers(&[
                (header::HOST, "127.0.0.1:9080"),
                (header::ORIGIN, "http://127.0.0.1:9080"),
                (header::CONTENT_TYPE, content_type),
            ])
        };
        assert!(
            rules()
                .check(&Method::POST, &with("application/json"))
                .is_ok()
        );
        assert!(
            rules()
                .check(&Method::POST, &with("application/json; charset=utf-8"))
                .is_ok()
        );
        let error = rules()
            .check(&Method::POST, &with("application/x-www-form-urlencoded"))
            .expect_err("a form post");
        assert_eq!(error.reason(), "content_type_not_json");
        assert_eq!(error.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let error = rules()
            .check(
                &Method::POST,
                &headers(&[
                    (header::HOST, "127.0.0.1:9080"),
                    (header::ORIGIN, "http://127.0.0.1:9080"),
                ]),
            )
            .expect_err("no content type");
        assert_eq!(error.reason(), "content_type_missing");
    }

    #[test]
    fn only_reads_skip_the_origin_and_content_type_rules() {
        let headers = headers(&[(header::HOST, "127.0.0.1:9080")]);
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!LoopbackRules::is_mutating(&method), "{method}");
            assert!(rules().check(&method, &headers).is_ok(), "{method}");
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(LoopbackRules::is_mutating(&method), "{method}");
            assert!(rules().check(&method, &headers).is_err(), "{method}");
        }
    }

    #[test]
    fn the_expected_port_comes_from_the_bind() {
        let rules = LoopbackRules::new(4000);
        assert_eq!(rules.port(), 4000);
        let matching = headers(&[(header::HOST, "127.0.0.1:4000")]);
        assert!(rules.check(&Method::GET, &matching).is_ok());
        let other_port = headers(&[(header::HOST, "127.0.0.1:9080")]);
        let error = rules
            .check(&Method::GET, &other_port)
            .expect_err("wrong port");
        assert!(
            error.message().contains("127.0.0.1:4000"),
            "{}",
            error.message()
        );
    }
}

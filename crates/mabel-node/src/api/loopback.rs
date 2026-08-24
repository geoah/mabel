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
//!
//! [`LoopbackRules::with_allowed_hosts`] widens the `Host` and `Origin` sets to
//! names an operator asked for, which is how a node reached over a tailnet or a
//! reverse proxy answers at all (decision 018). The default is unchanged:
//! loopback only, and the operator owns the network boundary.

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ServiceError;

/// The rules, parameterized by the port the API is bound to and by the hosts
/// an operator allowed beyond loopback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackRules {
    port: u16,
    allowed_hosts: Vec<String>,
}

impl LoopbackRules {
    /// Rules for an API listening on `port`, accepting loopback alone.
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            allowed_hosts: Vec::new(),
        }
    }

    /// Also accepts these `Host` values, and the `http` and `https` origins
    /// that match them (decision 018).
    ///
    /// Each value is trimmed and lowercased, the normalization the `Host` rule
    /// already applies, and is then matched as a whole string: `wallet.example`
    /// accepts a request whose `Host` is `wallet.example` and refuses one whose
    /// `Host` is `wallet.example:8443`. An empty value, a repeat and a spelling
    /// this port already accepts are dropped.
    ///
    /// Calling this twice adds to the set rather than replacing it, which is
    /// what lets `node.json` and `--allow-host` both contribute.
    #[must_use]
    pub fn with_allowed_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for host in hosts {
            let host = normalized(host.as_ref());
            if host.is_empty() || self.hosts().contains(&host) {
                continue;
            }
            self.allowed_hosts.push(host);
        }
        self
    }

    /// The port a request's `Host` and `Origin` must name.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The hosts this API accepts beyond loopback, normalized.
    #[must_use]
    pub fn allowed_hosts(&self) -> &[String] {
        &self.allowed_hosts
    }

    /// Whether the method may change state, which is what the `Origin` and
    /// content-type rules apply to.
    #[must_use]
    pub fn is_mutating(method: &Method) -> bool {
        !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
    }

    /// Every accepted `Host`, the two loopback spellings first.
    fn hosts(&self) -> Vec<String> {
        let mut hosts = Vec::with_capacity(2 + self.allowed_hosts.len());
        hosts.push(format!("127.0.0.1:{}", self.port));
        hosts.push(format!("localhost:{}", self.port));
        hosts.extend(self.allowed_hosts.iter().cloned());
        hosts
    }

    /// Every accepted `Origin`. An allowed host contributes both schemes,
    /// because the reverse proxy that terminates TLS is what makes the name
    /// reachable in the first place.
    fn origins(&self) -> Vec<String> {
        let mut origins = Vec::with_capacity(2 + self.allowed_hosts.len() * 2);
        origins.push(format!("http://127.0.0.1:{}", self.port));
        origins.push(format!("http://localhost:{}", self.port));
        for host in &self.allowed_hosts {
            origins.push(format!("http://{host}"));
            origins.push(format!("https://{host}"));
        }
        origins
    }

    fn accepts_host(&self, host: &str) -> bool {
        let host = normalized(host);
        if self.hosts().contains(&host) {
            return true;
        }
        self.port == 80 && (host == "127.0.0.1" || host == "localhost")
    }

    /// Applies the three rules to one request.
    ///
    /// # Errors
    ///
    /// Returns the rejection to render: 403 for a `Host` the rules do not
    /// accept or an `Origin` that does not match one, 415 for a mutating
    /// request whose content type is not `application/json`.
    pub fn check(&self, method: &Method, headers: &HeaderMap) -> Result<(), ServiceError> {
        let hosts = self.hosts();
        let host_message = format!("request rejected: Host header must be {}", listed(&hosts));
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

        let origins = self.origins();
        let origin_message = format!(
            "request rejected: Origin must be {} on a mutating route",
            listed(&origins)
        );
        match text(headers, &header::ORIGIN) {
            None => {
                return Err(rejected(
                    "origin_missing",
                    &origin_message,
                    StatusCode::FORBIDDEN,
                ));
            }
            Some(origin) if !origins.contains(&origin.to_ascii_lowercase()) => {
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

/// A `Host` value as the rules compare it: trimmed and lowercased.
fn normalized(host: &str) -> String {
    host.trim().to_ascii_lowercase()
}

/// The accepted values as a sentence names them: `a`, `a or b`, `a, b or c`.
fn listed(values: &[String]) -> String {
    match values {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
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

    /// The default set is the two loopback spellings and nothing else, and the
    /// rejection names them in the words `contracts/http/wallet-get-node.json`
    /// freezes.
    #[test]
    fn allowing_no_host_leaves_the_default_rules_exactly_as_they_were() {
        let rules = rules();
        assert!(rules.allowed_hosts().is_empty());
        assert_eq!(
            rules,
            LoopbackRules::new(PORT).with_allowed_hosts::<_, &str>([])
        );

        let error = rules
            .check(&Method::GET, &headers(&[(header::HOST, "wallet.example")]))
            .expect_err("no host is allowed beyond loopback");
        assert_eq!(
            error.message(),
            "request rejected: Host header must be 127.0.0.1:9080 or localhost:9080"
        );

        let error = rules
            .check(
                &Method::POST,
                &headers(&[
                    (header::HOST, "127.0.0.1:9080"),
                    (header::ORIGIN, "https://127.0.0.1:9080"),
                    (header::CONTENT_TYPE, "application/json"),
                ]),
            )
            .expect_err("https is not a loopback origin");
        assert_eq!(
            error.message(),
            "request rejected: Origin must be http://127.0.0.1:9080 or \
             http://localhost:9080 on a mutating route"
        );
    }

    #[test]
    fn an_allowed_host_is_accepted_and_every_other_host_is_still_refused() {
        let rules = rules().with_allowed_hosts(["Wallet.Tailnet.Example ", "", "localhost:9080"]);
        assert_eq!(rules.allowed_hosts(), ["wallet.tailnet.example"]);

        for host in [
            "wallet.tailnet.example",
            "WALLET.TAILNET.EXAMPLE",
            "127.0.0.1:9080",
            "localhost:9080",
        ] {
            let headers = headers(&[(header::HOST, host)]);
            assert!(rules.check(&Method::GET, &headers).is_ok(), "{host}");
        }

        for host in [
            "wallet.tailnet.example:8443",
            "other.tailnet.example",
            "evil.example",
        ] {
            let headers = headers(&[(header::HOST, host)]);
            let error = rules.check(&Method::GET, &headers).expect_err(host);
            assert_eq!(error.reason(), "host_not_loopback", "{host}");
            assert_eq!(error.status(), StatusCode::FORBIDDEN, "{host}");
            assert!(
                error.message().contains("wallet.tailnet.example"),
                "the rejection still names what is accepted: {}",
                error.message()
            );
        }
    }

    #[test]
    fn an_allowed_host_contributes_its_http_and_https_origins() {
        let rules = rules().with_allowed_hosts(["wallet.tailnet.example"]);
        let with = |origin: &str| {
            headers(&[
                (header::HOST, "wallet.tailnet.example"),
                (header::ORIGIN, origin),
                (header::CONTENT_TYPE, "application/json"),
            ])
        };
        for origin in [
            "https://wallet.tailnet.example",
            "http://wallet.tailnet.example",
            "http://127.0.0.1:9080",
        ] {
            assert!(
                rules.check(&Method::POST, &with(origin)).is_ok(),
                "{origin}"
            );
        }
        let error = rules
            .check(&Method::POST, &with("https://other.tailnet.example"))
            .expect_err("another host's origin");
        assert_eq!(error.reason(), "origin_mismatch");
        assert!(
            error
                .message()
                .contains("https://wallet.tailnet.example on a mutating route"),
            "{}",
            error.message()
        );
    }

    /// Two allowed hosts read as a list, not as a repeated "or".
    #[test]
    fn the_rejection_lists_every_accepted_host() {
        let rules = rules().with_allowed_hosts(["one.example", "two.example"]);
        let error = rules
            .check(&Method::GET, &headers(&[(header::HOST, "three.example")]))
            .expect_err("a third host");
        assert_eq!(
            error.message(),
            "request rejected: Host header must be 127.0.0.1:9080, localhost:9080, \
             one.example or two.example"
        );
    }
}

//! The `mabel://` link: one shareable string for an identity and up to four
//! machines that answer for it (proposal 006 section 7).
//!
//! ```text
//! mabel://<identity-id>[?endpoints=<endpoint-id>[,<endpoint-id>]{0,3}]
//! ```
//!
//! The grammar has no flexibility. The parser reads bytes and never decodes
//! anything: percent-encoding is refused where a URL library would expand it,
//! so a caller that decoded `%252f` into `%2f` before calling gets a refusal
//! rather than a second decode into `/`. Every refusal is whole, carries the
//! reason `invalid_mabel_link` and names the string as it was given; nothing is
//! trimmed to fit, so a link with three good endpoints and one bad one is
//! refused, the same rule the DNS endpoints record follows.
//!
//! Parsing is case-insensitive throughout, which is the id codec's rule
//! extended to the scheme and the one query key so that an uppercased paste of
//! a link reads as the link it is. Rendering is always lowercase: two spellings
//! of one id is what the anti-spoofing rules of proposal 003 section 4 forbid.

use std::fmt;
use std::str::FromStr;

use data_encoding::BASE32_NOPAD;
use iroh_base::EndpointId;

use crate::ID_BYTES;
use crate::id::{ID_STR_LEN, IdentityId};

/// The scheme, with no `://`.
pub const LINK_SCHEME: &str = "mabel";

/// The scheme and its separator, the prefix every link carries.
pub const LINK_PREFIX: &str = "mabel://";

/// The one query key a link may carry.
pub const LINK_ENDPOINTS_KEY: &str = "endpoints";

/// Most endpoints one link may name.
///
/// The payload caps at [`crate::MAX_ENDPOINTS`]; the link caps lower on string
/// length alone, so four endpoints render as 282 characters and still fit a
/// chat message and a printed line (proposal 006 section 7).
pub const MAX_LINK_ENDPOINTS: usize = 4;

/// The one refusal spelling both the CLI and `GET /api/resolve?input=` answer
/// with, beside code 2 (proposal 006 section 7).
pub const INVALID_MABEL_LINK: &str = "invalid_mabel_link";

/// Why a string is not a `mabel://` link.
///
/// One reason, [`INVALID_MABEL_LINK`], and one clause naming the rule that was
/// broken. The clause completes the sentence "`<input>` is not a mabel link:
/// ...", so a caller renders the input it holds and never has to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct InvalidLink(&'static str);

impl InvalidLink {
    /// The clause naming the rule the string broke.
    #[must_use]
    pub const fn clause(self) -> &'static str {
        self.0
    }

    /// The stable reason both surfaces answer with.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        INVALID_MABEL_LINK
    }
}

const WHITESPACE: InvalidLink = InvalidLink("it holds whitespace");
const NOT_ASCII: InvalidLink = InvalidLink("it holds a character outside ASCII");
const PERCENT: InvalidLink = InvalidLink("it holds percent-encoding");
const FRAGMENT: InvalidLink = InvalidLink("it holds a fragment");
const SCHEME: InvalidLink = InvalidLink("it does not begin with mabel://");
const USERINFO: InvalidLink = InvalidLink("the authority holds userinfo");
const PORT: InvalidLink = InvalidLink("the authority holds a port");
const AUTHORITY: InvalidLink = InvalidLink("the authority is not one identity id");
const PATH: InvalidLink = InvalidLink("it holds a path segment");
const EMPTY_QUERY: InvalidLink = InvalidLink("the query is empty");
const QUERY_KEY: InvalidLink = InvalidLink("the query holds a key other than endpoints");
const REPEATED_KEY: InvalidLink = InvalidLink("the query holds endpoints more than once");
const NO_ENDPOINT: InvalidLink = InvalidLink("endpoints names nothing");
const EMPTY_ENDPOINT: InvalidLink = InvalidLink("endpoints holds an empty element");
const TOO_MANY_ENDPOINTS: InvalidLink =
    InvalidLink("endpoints names more than 4 endpoints, the link cap");
const DUPLICATE_ENDPOINT: InvalidLink = InvalidLink("endpoints names one endpoint twice");
const ENDPOINT_CODEC: InvalidLink =
    InvalidLink("an endpoint is not 52 base32 characters under the id codec");
const ENDPOINT_POINT: InvalidLink =
    InvalidLink("an endpoint does not decompress to an ed25519 point");

/// One identity and the machines a link says answer for it.
///
/// The endpoints are hints and authorize nothing: they say where to ask, and
/// what comes back is verified from nothing like any other fetch (proposal 001
/// section 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MabelLink {
    identity: IdentityId,
    endpoints: Vec<EndpointId>,
}

impl MabelLink {
    /// Builds a link from an identity and 0 to 4 distinct endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLink`] for more than four endpoints or a repeat, the
    /// same two rules the parser enforces.
    pub fn new(identity: IdentityId, endpoints: &[EndpointId]) -> Result<Self, InvalidLink> {
        if endpoints.len() > MAX_LINK_ENDPOINTS {
            return Err(TOO_MANY_ENDPOINTS);
        }
        for (index, endpoint) in endpoints.iter().enumerate() {
            if endpoints[..index].contains(endpoint) {
                return Err(DUPLICATE_ENDPOINT);
            }
        }
        Ok(Self {
            identity,
            endpoints: endpoints.to_vec(),
        })
    }

    /// Reads a link, refusing the whole string on any broken rule.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLink`] with the clause naming the rule.
    pub fn parse(input: &str) -> Result<Self, InvalidLink> {
        if input.chars().any(char::is_whitespace) {
            return Err(WHITESPACE);
        }
        if !input.is_ascii() {
            return Err(NOT_ASCII);
        }
        if input.contains('%') {
            return Err(PERCENT);
        }
        if input.contains('#') {
            return Err(FRAGMENT);
        }
        let rest = strip_prefix_ignore_case(input, LINK_PREFIX).ok_or(SCHEME)?;

        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(authority_end);
        if authority.contains('@') {
            return Err(USERINFO);
        }
        if authority.contains(':') {
            return Err(PORT);
        }
        let identity = IdentityId::from_str(authority).map_err(|_| AUTHORITY)?;

        // The path is empty or a single slash, and the query is what is left.
        let query = match tail.strip_prefix('/').unwrap_or(tail) {
            "" => None,
            with_query => match with_query.strip_prefix('?') {
                Some(query) => Some(query),
                None => return Err(PATH),
            },
        };

        let endpoints = match query {
            None => Vec::new(),
            Some("") => return Err(EMPTY_QUERY),
            Some(query) => {
                let mut endpoints = None;
                for pair in query.split('&') {
                    let (key, value) = pair.split_once('=').ok_or(QUERY_KEY)?;
                    if !key.eq_ignore_ascii_case(LINK_ENDPOINTS_KEY) {
                        return Err(QUERY_KEY);
                    }
                    if endpoints.is_some() {
                        return Err(REPEATED_KEY);
                    }
                    endpoints = Some(parse_endpoints(value)?);
                }
                endpoints.unwrap_or_default()
            }
        };
        Ok(Self {
            identity,
            endpoints,
        })
    }

    /// Whether a string is meant to be a link, so a caller that takes an
    /// alias, an id or a link knows which refusal to give.
    ///
    /// A string carrying `://`, or beginning `mabel:` in either case, is a link
    /// attempt and is refused as one rather than looked up as an alias. No
    /// identity id and no alias in this system spells either.
    #[must_use]
    pub fn looks_like_link(input: &str) -> bool {
        input.contains("://") || strip_prefix_ignore_case(input, "mabel:").is_some()
    }

    /// The identity the link names.
    #[must_use]
    pub const fn identity(&self) -> IdentityId {
        self.identity
    }

    /// The endpoints the link hints at, in the order it named them.
    #[must_use]
    pub fn endpoints(&self) -> &[EndpointId] {
        &self.endpoints
    }
}

impl FromStr for MabelLink {
    type Err = InvalidLink;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl fmt::Display for MabelLink {
    /// Renders lowercase, with no path and no query when the link names no
    /// endpoint.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{LINK_PREFIX}{}", self.identity)?;
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            if index == 0 {
                write!(f, "?{LINK_ENDPOINTS_KEY}=")?;
            } else {
                write!(f, ",")?;
            }
            write!(f, "{}", render_id(endpoint.as_bytes()))?;
        }
        Ok(())
    }
}

/// The 1 to 4 endpoint ids of the `endpoints` value.
fn parse_endpoints(value: &str) -> Result<Vec<EndpointId>, InvalidLink> {
    if value.is_empty() {
        return Err(NO_ENDPOINT);
    }
    let mut endpoints: Vec<EndpointId> = Vec::new();
    for element in value.split(',') {
        if element.is_empty() {
            return Err(EMPTY_ENDPOINT);
        }
        let endpoint = parse_endpoint(element)?;
        if endpoints.contains(&endpoint) {
            return Err(DUPLICATE_ENDPOINT);
        }
        if endpoints.len() == MAX_LINK_ENDPOINTS {
            return Err(TOO_MANY_ENDPOINTS);
        }
        endpoints.push(endpoint);
    }
    Ok(endpoints)
}

/// One endpoint id under the id codec, checked to be a curve point.
fn parse_endpoint(text: &str) -> Result<EndpointId, InvalidLink> {
    let bytes = decode_id(text).ok_or(ENDPOINT_CODEC)?;
    EndpointId::from_bytes(&bytes).map_err(|_| ENDPOINT_POINT)
}

/// The 32 bytes of a 52-character base32 id, either case.
fn decode_id(text: &str) -> Option<[u8; ID_BYTES]> {
    if text.len() != ID_STR_LEN {
        return None;
    }
    BASE32_NOPAD
        .decode(text.to_ascii_uppercase().as_bytes())
        .ok()?
        .try_into()
        .ok()
}

/// 32 bytes as the lowercase base32 every surface spells an id in.
///
/// `iroh_base` renders an endpoint id as hex, and a link never does.
#[must_use]
pub fn render_id(bytes: &[u8; ID_BYTES]) -> String {
    BASE32_NOPAD.encode(bytes).to_ascii_lowercase()
}

/// `input` without `prefix`, matched without regard to ASCII case.
fn strip_prefix_ignore_case<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, rest) = input.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::{INVALID_MABEL_LINK, LINK_PREFIX, MAX_LINK_ENDPOINTS, MabelLink, render_id};
    use crate::IdentityId;
    use iroh_base::{EndpointId, SecretKey};

    fn identity() -> IdentityId {
        IdentityId::from_bytes([0x11; 32])
    }

    fn endpoint(seed: u8) -> EndpointId {
        SecretKey::from_bytes(&[seed; 32]).public()
    }

    fn link(text: &str) -> MabelLink {
        MabelLink::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    fn refusal(text: &str) -> &'static str {
        MabelLink::parse(text).expect_err(text).clause()
    }

    #[test]
    fn a_bare_identity_link_parses_and_renders_back() {
        let text = format!("{LINK_PREFIX}{}", identity());
        let parsed = link(&text);
        assert_eq!(parsed.identity(), identity());
        assert!(parsed.endpoints().is_empty());
        assert_eq!(parsed.to_string(), text);
    }

    #[test]
    fn four_endpoints_parse_and_render_in_the_order_given() {
        let endpoints: Vec<EndpointId> = (1..=4).map(endpoint).collect();
        let rendered: Vec<String> = endpoints
            .iter()
            .map(|endpoint| render_id(endpoint.as_bytes()))
            .collect();
        let text = format!(
            "{LINK_PREFIX}{}?endpoints={}",
            identity(),
            rendered.join(",")
        );
        let parsed = link(&text);
        assert_eq!(parsed.endpoints(), endpoints.as_slice());
        assert_eq!(parsed.to_string(), text);
        assert_eq!(
            MabelLink::new(identity(), &endpoints)
                .expect("four distinct endpoints")
                .to_string(),
            text
        );
    }

    #[test]
    fn an_uppercase_link_parses_and_renders_lowercase() {
        let text = format!(
            "{LINK_PREFIX}{}?endpoints={}",
            identity(),
            render_id(endpoint(1).as_bytes())
        )
        .to_ascii_uppercase();
        let parsed = link(&text);
        assert_eq!(parsed.identity(), identity());
        assert_eq!(parsed.endpoints(), [endpoint(1)]);
        assert_eq!(
            parsed.to_string(),
            format!(
                "{LINK_PREFIX}{}?endpoints={}",
                identity(),
                render_id(endpoint(1).as_bytes())
            ),
            "rendering is lowercase whatever the input was"
        );
    }

    #[test]
    fn an_empty_path_and_a_single_slash_are_the_same_link() {
        let bare = format!("{LINK_PREFIX}{}", identity());
        assert_eq!(link(&format!("{bare}/")), link(&bare));
        assert_eq!(
            link(&format!(
                "{bare}/?endpoints={}",
                render_id(endpoint(1).as_bytes())
            ))
            .endpoints(),
            [endpoint(1)]
        );
    }

    #[test]
    fn every_refusal_rule_of_section_7_refuses_the_whole_string() {
        let id = identity().to_string();
        let one = render_id(endpoint(1).as_bytes());
        let two = render_id(endpoint(2).as_bytes());
        for (text, clause) in [
            (format!("https://{id}"), "it does not begin with mabel://"),
            (format!("mabel:/{id}"), "it does not begin with mabel://"),
            (format!("{LINK_PREFIX}{id}#top"), "it holds a fragment"),
            (format!("{LINK_PREFIX}{id}%2f"), "it holds percent-encoding"),
            (format!("{LINK_PREFIX}{id} "), "it holds whitespace"),
            (
                format!("{LINK_PREFIX}{id}?endpoints={one},{two} "),
                "it holds whitespace",
            ),
            (
                format!("{LINK_PREFIX}user@{id}"),
                "the authority holds userinfo",
            ),
            (
                format!("{LINK_PREFIX}{id}:443"),
                "the authority holds a port",
            ),
            (
                format!("{LINK_PREFIX}{id}{id}"),
                "the authority is not one identity id",
            ),
            (
                format!("{LINK_PREFIX}{id}/profile"),
                "it holds a path segment",
            ),
            (format!("{LINK_PREFIX}{id}//"), "it holds a path segment"),
            (format!("{LINK_PREFIX}{id}?"), "the query is empty"),
            (
                format!("{LINK_PREFIX}{id}?witness={one}"),
                "the query holds a key other than endpoints",
            ),
            (
                format!("{LINK_PREFIX}{id}?endpoints={one}&endpoints={two}"),
                "the query holds endpoints more than once",
            ),
            (
                format!("{LINK_PREFIX}{id}?endpoints="),
                "endpoints names nothing",
            ),
            (
                format!("{LINK_PREFIX}{id}?endpoints={one},,{two}"),
                "endpoints holds an empty element",
            ),
            (
                format!("{LINK_PREFIX}{id}?endpoints={one},{one}"),
                "endpoints names one endpoint twice",
            ),
            (
                format!(
                    "{LINK_PREFIX}{id}?endpoints={}",
                    (1..=5)
                        .map(|seed| render_id(endpoint(seed).as_bytes()))
                        .collect::<Vec<String>>()
                        .join(",")
                ),
                "endpoints names more than 4 endpoints, the link cap",
            ),
            (
                format!("{LINK_PREFIX}{id}?endpoints=nope"),
                "an endpoint is not 52 base32 characters under the id codec",
            ),
            (
                format!("{LINK_PREFIX}{id}?endpoints={}", render_id(&[0x02; 32])),
                "an endpoint does not decompress to an ed25519 point",
            ),
        ] {
            assert_eq!(refusal(&text), clause, "{text}");
            assert_eq!(
                MabelLink::parse(&text).expect_err(&text).reason(),
                INVALID_MABEL_LINK
            );
        }
    }

    #[test]
    fn three_good_endpoints_and_one_bad_one_are_refused_together() {
        let text = format!(
            "{LINK_PREFIX}{}?endpoints={},{},{},nope",
            identity(),
            render_id(endpoint(1).as_bytes()),
            render_id(endpoint(2).as_bytes()),
            render_id(endpoint(3).as_bytes())
        );
        assert_eq!(
            refusal(&text),
            "an endpoint is not 52 base32 characters under the id codec"
        );
    }

    #[test]
    fn a_link_attempt_is_told_apart_from_an_alias_or_an_id() {
        assert!(MabelLink::looks_like_link("mabel://x"));
        assert!(MabelLink::looks_like_link("MABEL://x"));
        assert!(MabelLink::looks_like_link("https://alice.example"));
        assert!(MabelLink::looks_like_link("mabel:x"));
        assert!(!MabelLink::looks_like_link("alice"));
        assert!(!MabelLink::looks_like_link(&identity().to_string()));
        assert!(!MabelLink::looks_like_link("alice.example"));
    }

    #[test]
    fn a_builder_refuses_what_the_parser_refuses() {
        let five: Vec<EndpointId> = (1..=5).map(endpoint).collect();
        assert_eq!(
            MabelLink::new(identity(), &five)
                .expect_err("five")
                .clause(),
            "endpoints names more than 4 endpoints, the link cap"
        );
        assert_eq!(MAX_LINK_ENDPOINTS, 4);
        assert_eq!(
            MabelLink::new(identity(), &[endpoint(1), endpoint(1)])
                .expect_err("a repeat")
                .clause(),
            "endpoints names one endpoint twice"
        );
    }
}

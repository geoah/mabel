//! Golden vectors for the `mabel://` link (proposal 006 section 7).
//!
//! `test-vectors/links.json` is a literal: the tests here read it and compare,
//! both directions, and never write it. The only writer is `gen_links`, which
//! is `#[ignore]`d and gated behind the `gen-vectors` feature:
//!
//! ```text
//! cargo test -p mabel-core --features gen-vectors -- --ignored gen_links
//! ```
//!
//! An accepted vector pins what one input parses to and what that parse renders
//! back as; the rendering is authoritative, so an input spelled in uppercase or
//! with a trailing slash renders as the one canonical form. A refused vector
//! pins the whole-string refusal: `code` is the contract, `reason` is English
//! and may be reworded.

use std::path::{Path, PathBuf};

use iroh_base::{EndpointId, SecretKey};
use mabel_core::{INVALID_MABEL_LINK, IdentityId, LINK_PREFIX, MabelLink, render_id};
use serde_json::{Value, json};

/// Alice's ledger, the identity every vector names (`test-vectors/README.md`).
fn alice() -> IdentityId {
    "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq"
        .parse()
        .expect("the scenario identity id")
}

/// The endpoints the vectors hint at, the same keys the event vectors use.
fn endpoint(seed: u8) -> EndpointId {
    SecretKey::from_bytes(&[seed; 32]).public()
}

fn spelled(seed: u8) -> String {
    render_id(endpoint(seed).as_bytes())
}

fn vectors_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-vectors/links.json")
}

/// The accepted cases: one input, what it parses to, what it renders as.
fn accepted() -> Vec<Value> {
    let id = alice().to_string();
    let bare = format!("{LINK_PREFIX}{id}");
    let four = [spelled(0x44), spelled(0x55), spelled(0x66), spelled(0x77)].join(",");
    vec![
        json!({
            "case": "identity-only",
            "description": "One identity id and nothing else, the shortest link.",
            "input": bare,
        }),
        json!({
            "case": "one-endpoint",
            "description": "One machine to ask, which is what a bootstrap needs.",
            "input": format!("{bare}?endpoints={}", spelled(0x44)),
        }),
        json!({
            "case": "four-endpoints",
            "description": "The link cap: four endpoints, in the order the link names them.",
            "input": format!("{bare}?endpoints={four}"),
        }),
        json!({
            "case": "uppercase",
            "description": "An uppercase link parses and renders lowercase; the codec is \
                            case-insensitive and the rendering is not.",
            "input": format!("{bare}?endpoints={}", spelled(0x44)).to_ascii_uppercase(),
        }),
        json!({
            "case": "trailing-slash",
            "description": "The path may be a single slash, which renders away.",
            "input": format!("{bare}/"),
        }),
        json!({
            "case": "slash-then-query",
            "description": "A single slash before the query is the same link.",
            "input": format!("{bare}/?endpoints={}", spelled(0x55)),
        }),
    ]
    .into_iter()
    .map(|case| {
        let input = case["input"].as_str().expect("an input").to_owned();
        let link = MabelLink::parse(&input).unwrap_or_else(|error| panic!("{input}: {error}"));
        let mut document = case;
        document["identity_id"] = json!(link.identity().to_string());
        document["endpoints"] = json!(
            link.endpoints()
                .iter()
                .map(|endpoint| render_id(endpoint.as_bytes()))
                .collect::<Vec<String>>()
        );
        document["rendered"] = json!(link.to_string());
        document
    })
    .collect()
}

/// The refused cases, one per refusal rule of section 7.
fn refused() -> Vec<Value> {
    let id = alice().to_string();
    let bare = format!("{LINK_PREFIX}{id}");
    let one = spelled(0x44);
    let two = spelled(0x55);
    let five = [
        spelled(0x44),
        spelled(0x55),
        spelled(0x66),
        spelled(0x77),
        spelled(0x88),
    ]
    .join(",");
    [
        ("other-scheme", format!("https://{id}")),
        ("scheme-without-authority", format!("mabel:/{id}")),
        ("bare-identity-id", id.clone()),
        ("fragment", format!("{bare}#machines")),
        ("percent-encoded-slash", format!("{bare}%2f")),
        ("double-encoded-slash", format!("{bare}%252f")),
        ("trailing-space", format!("{bare} ")),
        ("inner-tab", format!("{LINK_PREFIX}\t{id}")),
        ("userinfo", format!("{LINK_PREFIX}alice@{id}")),
        ("port", format!("{bare}:443")),
        ("two-ids-in-the-authority", format!("{LINK_PREFIX}{id}{id}")),
        ("path-segment", format!("{bare}/profile")),
        ("empty-second-segment", format!("{bare}//")),
        ("empty-query", format!("{bare}?")),
        ("unknown-query-key", format!("{bare}?witness={one}")),
        (
            "repeated-endpoints-key",
            format!("{bare}?endpoints={one}&endpoints={two}"),
        ),
        ("empty-endpoints", format!("{bare}?endpoints=")),
        ("empty-element", format!("{bare}?endpoints={one},,{two}")),
        (
            "duplicate-endpoint",
            format!("{bare}?endpoints={one},{one}"),
        ),
        ("five-endpoints", format!("{bare}?endpoints={five}")),
        (
            "three-good-and-one-bad",
            format!("{bare}?endpoints={one},{two},{},nope", spelled(0x66)),
        ),
        (
            "endpoint-not-a-point",
            format!("{bare}?endpoints={}", render_id(&[0x02; 32])),
        ),
    ]
    .into_iter()
    .map(|(case, input)| {
        let error = MabelLink::parse(&input).expect_err(&input);
        json!({
            "case": case,
            "input": input,
            "code": error.reason(),
            "reason": error.clause(),
        })
    })
    .collect()
}

fn document() -> Value {
    json!({
        "file": "links.json",
        "description": "The mabel:// link grammar of proposal 006 section 7: what parses, what \
                        it renders back as, and every rule that refuses the whole string.",
        "prefix": LINK_PREFIX,
        "max_endpoints": mabel_core::MAX_LINK_ENDPOINTS,
        "refusal_code": INVALID_MABEL_LINK,
        "accepted": accepted(),
        "refused": refused(),
    })
}

fn read_vectors() -> Value {
    let path = vectors_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing vector {}: {error}. Regenerate with `cargo test -p mabel-core \
             --features gen-vectors -- --ignored gen_links` and review the diff.",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("links.json is valid JSON")
}

fn array(document: &Value, key: &str) -> Vec<Value> {
    document[key]
        .as_array()
        .unwrap_or_else(|| panic!("links.json has no {key} array"))
        .clone()
}

fn string(case: &Value, key: &str) -> String {
    case[key]
        .as_str()
        .unwrap_or_else(|| panic!("{case} has no string {key}"))
        .to_owned()
}

#[test]
fn the_parser_matches_the_checked_in_link_vectors() {
    assert_eq!(
        read_vectors(),
        document(),
        "links.json no longer matches what the parser does"
    );
}

/// Both directions, read off the file rather than off the parser: every
/// accepted input parses to the identity and endpoints the vector names, and
/// its rendering parses back to the same link.
#[test]
fn every_accepted_vector_parses_and_renders_as_the_file_says() {
    let vectors = read_vectors();
    for case in array(&vectors, "accepted") {
        let input = string(&case, "input");
        let link = MabelLink::parse(&input).unwrap_or_else(|error| panic!("{input}: {error}"));
        assert_eq!(
            link.identity().to_string(),
            string(&case, "identity_id"),
            "{input}"
        );
        let endpoints: Vec<String> = link
            .endpoints()
            .iter()
            .map(|endpoint| render_id(endpoint.as_bytes()))
            .collect();
        assert_eq!(json!(endpoints), case["endpoints"], "{input}");

        let rendered = string(&case, "rendered");
        assert_eq!(link.to_string(), rendered, "{input}");
        assert_eq!(
            MabelLink::parse(&rendered).expect("a rendered link parses"),
            link,
            "{rendered} does not parse back to itself"
        );
        assert_eq!(
            rendered,
            rendered.to_lowercase(),
            "a rendered link is lowercase"
        );
    }
}

#[test]
fn every_refused_vector_is_refused_whole_with_one_reason() {
    let vectors = read_vectors();
    let refusals = array(&vectors, "refused");
    assert!(refusals.len() >= 20, "the refusal rules are all covered");
    for case in refusals {
        let input = string(&case, "input");
        let error = MabelLink::parse(&input).expect_err(&input);
        assert_eq!(error.reason(), INVALID_MABEL_LINK, "{input}");
        assert_eq!(string(&case, "code"), INVALID_MABEL_LINK, "{input}");
        assert_eq!(error.clause(), string(&case, "reason"), "{input}");
    }
}

/// Rewrites `test-vectors/links.json`. The only writer; run it deliberately
/// and commit the diff for review.
#[cfg(feature = "gen-vectors")]
#[test]
#[ignore = "writes test-vectors/links.json; run explicitly and review the diff"]
fn gen_links() {
    let mut text = serde_json::to_string_pretty(&document()).expect("serializes");
    text.push('\n');
    std::fs::write(vectors_path(), text).expect("write links.json");
}

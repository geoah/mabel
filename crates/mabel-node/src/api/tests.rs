//! Contract tests: one per file under `contracts/http/`, plus the loopback
//! boundary.
//!
//! Each fixture test builds the request from the fixture's own `route`,
//! `method` and `request`, runs it against the stub, and compares the response
//! body to the fixture's `response` key for key. The fixtures are the frozen
//! contract, so a diff here is a bug in this module (`contracts/README.md`).

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::documents::{DeclaredKind, RoleName};
use super::error::{ErrorLayer, ServiceError};
use super::service::{
    EventPageRequest, FetchIdentity, ForkQuery, LookupRequest, PageRequest, PushRequest,
    ReplaceProfile, ResolveInput, SetContact,
};
use super::stub::{FIXTURES, Fixture, NodeCall, StubNodeService};
use super::{ApiOptions, UiSource, documents::Id, node_router};

const ALICE: &str = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
const BOB: &str = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
const CAROL: &str = "jqtnsb2me7mj5xsze4gavqklohqhdmkshfiz65khjmxtxjruqh2q";
const ACME: &str = "2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a";
const ATTESTATION: &str = "65cssg5tnr3gyxe2rwhsgqc3nct3pwg2bqxr2oxpelejuoorlsnq";
/// The two machines `wallet-get-witnesses.json` names, one per witness.
const ENDPOINT_ONE: &str = "zbj22dym2k3btlvjftxmj7kwujgwjgovqthhsjl6ixh5qe43mctq";
const ENDPOINT_TWO: &str = "5yy7qpeiu4jbtjx47g7obwu3yitcaweplik2mfcvknie36letzoa";
/// The two witness identities Alice's `WitnessSet` names. A witness is an
/// identity and never an endpoint id (proposal 006 section 1).
const WITNESS_ONE: &str = "ovfp3btcnjyhwmyw3ldk3wmt2ppb5w5c5adyzcavswmyq7xkg7fq";
const WITNESS_TWO: &str = "q7hnsnk6ycwjyzwbmqjcaxwlmxvvfjbmwzq4gz4dbtvpojjuh3fq";
/// This node's own endpoint id, the first machine `node-get-node.json` names
/// and the first Alice advertises.
const NODE_ENDPOINT: &str = "fd2ijzgxe3qk64jeqbgwjgqcg2cnmyyrfwghb6oar2wbg5ddxvla";
/// The second machine of `wallet-post-identity-endpoints.json`.
const SECOND_MACHINE: &str = "pl3jspahmwxbfiulckl5kqptsazcvqjiajo47ruerssx7vdfrgcq";
/// The hostname `wallet-get-resolve.json` looks up, and the one Alice's
/// profile claims.
const HOSTNAME: &str = "alice.example";

/// The host the loopback rules expect at the default bind.
const HOST: &str = "127.0.0.1:9080";
/// The matching origin.
const ORIGIN: &str = "http://127.0.0.1:9080";

/// The reasons this module produces itself, before any service is called.
/// Every other reason in a fixture's `errors` array comes from a service, and
/// the round-trip test below drives those through the stub.
const API_OWNED_REASONS: [&str; 16] = [
    "host_not_loopback",
    "origin_mismatch",
    "content_type_not_json",
    "missing_field",
    "unknown_enum_value",
    "unsupported_declared_kind",
    "malformed_identity_id",
    "malformed_ledger_id",
    "malformed_endpoint_id",
    "malformed_hostname",
    "malformed_query_parameter",
    "unknown_query_parameter",
    "invalid_mabel_link",
    "malformed_base64",
    "duplicate_witness",
    "duplicate_endpoint",
];

fn id(raw: &str) -> Id {
    Id::parse(raw).expect("a fixture id")
}

fn options() -> ApiOptions {
    // The UI has its own tests; here it must not swallow an API path.
    ApiOptions::default().with_ui(UiSource::Disabled)
}

/// The one router every node serves (proposal 006 section 8).
fn node(stub: &Arc<StubNodeService>) -> Router {
    node_router(Arc::clone(stub) as Arc<dyn super::NodeService>, &options())
}

/// A well-formed loopback request, with the headers a mutating route needs.
fn request(method: &str, uri: &str, body: &Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST);
    if method != "GET" && method != "HEAD" {
        builder = builder
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, "application/json");
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        Body::from(body.to_string())
    };
    builder.body(body).expect("a request")
}

async fn send(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .oneshot(request)
        .await
        .expect("the router is infallible");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "a response body that is not JSON: {error}: {}",
                String::from_utf8_lossy(&bytes)
            )
        })
    };
    (status, body)
}

/// The fixture's `route` with its path parameters filled in from
/// `test-vectors/`.
fn concrete_route(route: &str) -> String {
    route
        .replace(":identity_id", ALICE)
        .replace(":ledger_id", ALICE)
        .replace(":event_id", ATTESTATION)
        .replace(":endpoint_id", ENDPOINT_ONE)
        .replace(":hostname", HOSTNAME)
}

/// Runs a fixture's own request against the stub.
async fn run(name: &str, stub: &Arc<StubNodeService>) -> (StatusCode, Value) {
    let fixture = Fixture::named(name);
    let uri = concrete_route(&fixture.route());
    send(
        node(stub),
        request(&fixture.method(), &uri, &fixture.request()),
    )
    .await
}

fn expect_response(name: &str, status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::OK, "{name}: {body}");
    assert_eq!(body, &Fixture::named(name).response(), "{name}");
}

/// The fixture's error example for one reason.
fn fixture_error(name: &str, reason: &str) -> (StatusCode, Value) {
    Fixture::named(name)
        .errors()
        .into_iter()
        .find(|(_, body)| body["details"]["reason"] == json!(reason))
        .map(|(status, body)| (StatusCode::from_u16(status).expect("a status"), body))
        .unwrap_or_else(|| panic!("{name} has no {reason} example"))
}

/// Rebuilds a [`ServiceError`] from a fixture error body, so the layer, the
/// prefix and the status all go through the same mapping a service uses.
fn error_from_fixture(status: StatusCode, body: &Value) -> ServiceError {
    let code = u16::try_from(body["code"].as_u64().expect("a code")).expect("a small code");
    let message = body["message"].as_str().expect("a message");
    let details = body["details"].as_object().expect("details");
    let reason = details["reason"].as_str().expect("a reason");
    let layer = ErrorLayer::ALL
        .into_iter()
        .find(|layer| {
            layer.code() == code
                && !layer.prefix().is_empty()
                && message.starts_with(layer.prefix())
        })
        .or_else(|| {
            ErrorLayer::ALL
                .into_iter()
                .find(|layer| layer.code() == code && layer.prefix().is_empty())
        })
        .unwrap_or_else(|| panic!("no layer for code {code} and message {message}"));
    let sentence = message
        .strip_prefix(layer.prefix())
        .expect("the prefix of the layer that was just matched");
    let mut error = ServiceError::new(layer, reason, sentence).with_status(status);
    for (key, value) in details {
        if key != "reason" {
            error = error.with_detail(key.clone(), value.clone());
        }
    }
    error
}

// ---------------------------------------------------------------- wallet ----

#[tokio::test]
async fn wallet_get_identities_matches_the_fixture_and_lists_organizations() {
    let name = "wallet-get-identities.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::Identities);

    // An organization is an identity with `declared_kind: "organization"`;
    // there is no separate collection (proposal 001, clarifications).
    let identities = body["identities"].as_array().expect("an array");
    let organization = identities
        .iter()
        .find(|identity| identity["declared_kind"] == json!("organization"))
        .expect("the fixture lists acme");
    assert_eq!(organization["identity_id"], json!(ACME));
    let organization = organization.as_object().expect("an object");
    assert!(
        !organization.contains_key("active_key") && !organization.contains_key("reserve_commit"),
        "only a person carries keys: {organization:?}"
    );
    let person = identities
        .iter()
        .find(|identity| identity["identity_id"] == json!(ALICE))
        .expect("the fixture lists alice");
    assert_eq!(person["active_key"].as_str().map(str::len), Some(52));
}

#[tokio::test]
async fn wallet_get_known_identities_matches_the_fixture() {
    let name = "wallet-get-known-identities.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::KnownIdentities(PageRequest {
            offset: 0,
            limit: 100
        })
    );

    // The rows are ascending by `identity_id`, and neither of them is an
    // identity `GET /api/identities` lists.
    let identities = body["identities"].as_array().expect("an array");
    let ids: Vec<&str> = identities
        .iter()
        .map(|row| row["identity_id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, vec![CAROL, BOB, WITNESS_ONE]);
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    assert!(!ids.contains(&ALICE) && !ids.contains(&ACME));

    // Absence of a verdict and a verdict of "no record" are separate words, so
    // one row can tell a reader which of the two it is looking at (issue 042).
    assert_eq!(identities[1]["hostname"], json!("bob.example"));
    assert_eq!(identities[1]["verification_status"], json!("unverified"));
    assert_eq!(identities[2]["hostname"], json!("keeper.example"));
    assert_eq!(identities[2]["verification_status"], json!("unchecked"));
    assert_eq!(identities[0]["hostname"], Value::Null);
    assert_eq!(identities[0]["verification_status"], json!("unclaimed"));

    // A stored row carries the two fields only a stored copy answers; an
    // unstored one nulls both and still reports its distance.
    let bob = &identities[1];
    assert_eq!(bob["stored"], json!(true));
    assert_eq!(bob["trusted"], json!(true));
    assert_eq!(bob["declared_kind"], json!("person"));
    assert_eq!(bob["head_seq"], json!(2));
    assert_eq!(bob["degrees"], json!(1));
    let carol = &identities[0];
    assert_eq!(carol["stored"], json!(false));
    assert_eq!(carol["declared_kind"], Value::Null);
    assert_eq!(carol["head_seq"], Value::Null);
    assert_eq!(carol["degrees"], json!(2));
    // The name a note gave is an alias, never a display name.
    assert_eq!(carol["display_name"], Value::Null);
    assert_eq!(carol["alias"], json!("carol at the co-op"));

    // Every row carries every key, `null` where nothing applies.
    for row in identities {
        let row = row.as_object().expect("a known identity row");
        assert_eq!(row.len(), 11, "{row:?}");
        for key in [
            "identity_id",
            "display_name",
            "alias",
            "email",
            "hostname",
            "verification_status",
            "declared_kind",
            "stored",
            "trusted",
            "degrees",
            "head_seq",
        ] {
            assert!(row.contains_key(key), "{key} is absent from {row:?}");
        }
    }
}

/// `known` is a static segment, so it is matched before `{identity_id}` and no
/// request for the list is read as a malformed id.
#[tokio::test]
async fn the_known_route_is_matched_before_the_identity_route() {
    let stub = Arc::new(StubNodeService::new());
    let request = request("GET", "/api/identities/known", &Value::Null);
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        stub.call(),
        NodeCall::KnownIdentities(PageRequest {
            offset: 0,
            limit: 100
        })
    );
}

#[tokio::test]
async fn wallet_post_identities_matches_the_fixture() {
    let name = "wallet-post-identities.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        NodeCall::CreateIdentity(request) => {
            assert_eq!(request.alias, "alice");
            assert_eq!(request.declared_kind, DeclaredKind::Person);
            assert_eq!(request.display_name.as_deref(), Some("Alice Ashworth"));
            assert_eq!(request.email.as_deref(), Some("alice@alice.example"));
        }
        call => panic!("{call:?}"),
    }
}

/// The two optional profile keys are optional: a body without them creates an
/// identity that publishes nothing (proposal 005).
#[tokio::test]
async fn a_create_body_may_omit_the_display_name_and_the_email() {
    let stub = Arc::new(StubNodeService::new());
    let request = request("POST", "/api/identities", &json!({"alias": "alice"}));
    let (status, _) = send(node(&stub), request).await;
    assert_eq!(status, StatusCode::OK);
    match stub.call() {
        NodeCall::CreateIdentity(request) => {
            assert_eq!(request.display_name, None);
            assert_eq!(request.email, None);
        }
        call => panic!("{call:?}"),
    }
}

#[tokio::test]
async fn wallet_get_identity_matches_the_fixture() {
    let name = "wallet-get-identity.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::Identity(id(ALICE)));
}

#[tokio::test]
async fn wallet_get_identity_ledger_matches_the_fixture_and_passes_since_through() {
    let name = "wallet-get-identity-ledger.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::IdentityLedger(
            id(ALICE),
            EventPageRequest {
                since: 1,
                limit: 512
            }
        )
    );
    // `?since=` is inclusive: the fixture asks for 1 and gets seq 1 first.
    assert_eq!(body["since"], json!(1));
    assert_eq!(body["events"][0]["seq"], json!(1));
    // The page starts at the `WitnessSet`, so the route renders payload tag 19
    // (proposal 006 section 3).
    assert_eq!(body["events"][0]["payload_kind"], json!("witness_set"));
    assert_eq!(
        body["events"][0]["payload"]["witnesses"],
        json!([WITNESS_ONE, WITNESS_TWO])
    );
}

#[tokio::test]
async fn wallet_get_identity_keys_matches_the_fixture() {
    let name = "wallet-get-identity-keys.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::IdentityKeys(id(ALICE)));

    // A 200 carries all four key values, the two secrets included, in the one
    // base32 spelling every byte field uses: 52 characters for 32 bytes
    // (`contracts/README.md`, "Ids and byte fields").
    for key in [
        "active_secret_key",
        "reserve_secret_key",
        "active_key",
        "reserve_commit",
    ] {
        let value = body[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key}: {body}"));
        assert_eq!(value.len(), 52, "{key}");
        assert_eq!(value, value.to_ascii_lowercase(), "{key}");
    }
}

#[tokio::test]
async fn wallet_get_identity_keys_answers_the_fixture_rejections() {
    let name = "wallet-get-identity-keys.json";
    for reason in ["unknown_ledger", "no_keys_held"] {
        let (expected_status, expected) = fixture_error(name, reason);
        let stub = Arc::new(StubNodeService::new());
        stub.fail_with(error_from_fixture(expected_status, &expected));
        let (status, body) = run(name, &stub).await;
        assert_eq!(status, expected_status, "{reason}");
        assert_eq!(body, expected, "{reason}");
    }
}

#[tokio::test]
async fn a_keyless_identity_has_no_keys_to_hand_back() {
    // The keys of a keyless identity's controller belong to the controller's
    // own page, so this route refuses rather than resolving the link.
    let name = "wallet-get-identity-keys.json";
    let (expected_status, expected) = fixture_error(name, "no_keys_held");
    assert_eq!(expected_status, StatusCode::CONFLICT);
    assert_eq!(expected["code"], json!(20));
    assert_eq!(expected["details"]["identity_id"], json!(ACME));

    let stub = Arc::new(StubNodeService::new());
    stub.fail_with(error_from_fixture(expected_status, &expected));
    let request = request("GET", &format!("/api/identities/{ACME}/keys"), &Value::Null);
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, expected);
    assert_eq!(stub.call(), NodeCall::IdentityKeys(id(ACME)));
}

#[tokio::test]
async fn wallet_post_identity_profile_matches_the_fixture_and_requires_both_keys() {
    let name = "wallet-post-identity-profile.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::ReplaceProfile(ReplaceProfile {
            identity_id: id(ALICE),
            display_name: Some("Alice Ashworth".to_owned()),
            hostname: Some("alice.example".to_owned()),
            email: Some("alice@alice.example".to_owned()),
        })
    );

    // The operation is replacement, so a body naming one key would clear the
    // others by accident (proposal 003 section 1, proposal 005).
    let (expected_status, expected) = fixture_error(name, "missing_field");
    let stub = Arc::new(StubNodeService::new());
    let request = request(
        "POST",
        &format!("/api/identities/{ALICE}/profile"),
        &json!({"display_name": "Alice Ashworth"}),
    );
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_profile_body_may_null_any_key() {
    for (request_body, display_name, hostname, email) in [
        (
            json!({"display_name": null, "hostname": "alice.example", "email": null}),
            None,
            Some("alice.example"),
            None,
        ),
        (
            json!({"display_name": "Alice Ashworth", "hostname": null, "email": null}),
            Some("Alice Ashworth"),
            None,
            None,
        ),
        (
            json!({"display_name": null, "hostname": null, "email": "alice@alice.example"}),
            None,
            None,
            Some("alice@alice.example"),
        ),
        (
            json!({"display_name": null, "hostname": null, "email": null}),
            None,
            None,
            None,
        ),
    ] {
        let stub = Arc::new(StubNodeService::new());
        let request = request(
            "POST",
            &format!("/api/identities/{ALICE}/profile"),
            &request_body,
        );
        let (status, _) = send(node(&stub), request).await;
        assert_eq!(status, StatusCode::OK, "{request_body}");
        assert_eq!(
            stub.call(),
            NodeCall::ReplaceProfile(ReplaceProfile {
                identity_id: id(ALICE),
                display_name: display_name.map(ToOwned::to_owned),
                hostname: hostname.map(ToOwned::to_owned),
                email: email.map(ToOwned::to_owned),
            }),
            "{request_body}"
        );
    }
}

#[tokio::test]
async fn wallet_post_identity_verification_matches_the_fixture() {
    let name = "wallet-post-identity-verification.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::CheckVerification(id(ALICE)));
    assert_eq!(body["verification"]["status"], json!("verified"));
    assert_eq!(body["verification"]["stale"], json!(false));
}

/// The contact fixtures are about Bob, so their requests name Bob rather than
/// the `ALICE` [`concrete_route`] fills in for every other path parameter: a
/// private note is most itself on a foreign identity.
#[tokio::test]
async fn wallet_get_identity_contact_matches_the_fixture_for_a_foreign_identity() {
    let name = "wallet-get-identity-contact.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request(
            "GET",
            &format!("/api/identities/{BOB}/contact"),
            &Value::Null,
        ),
    )
    .await;
    expect_response(name, status, &body);
    assert_eq!(body["identity_id"], json!(BOB));
    assert_eq!(stub.call(), NodeCall::Contact(id(BOB)));
}

#[tokio::test]
async fn wallet_put_identity_contact_matches_the_fixture_and_caps_a_nickname() {
    let name = "wallet-put-identity-contact.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request(
            "PUT",
            &format!("/api/identities/{BOB}/contact"),
            &Fixture::named(name).request(),
        ),
    )
    .await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::SetContact(SetContact {
            identity_id: id(BOB),
            nickname: Some("bob at the print shop".to_owned()),
            note: Some("met at the 2023 zine fair; verifies his own hostname".to_owned()),
        })
    );

    let (expected_status, expected) = fixture_error(name, "contact_field_too_long");
    let cap = expected["details"]["cap"].as_u64().expect("a cap") as usize;
    let len = expected["details"]["len"].as_u64().expect("a length") as usize;
    let stub = Arc::new(StubNodeService::new());
    let request = request(
        "PUT",
        &format!("/api/identities/{BOB}/contact"),
        &json!({"nickname": "n".repeat(len), "note": null}),
    );
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected, "a nickname over {cap} bytes is refused");
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn wallet_get_lookup_matches_the_fixture_and_reads_from() {
    let name = "wallet-get-lookup.json";
    // The fixture looks Carol up from Alice, so the target is Carol rather
    // than the `ALICE` `concrete_route` fills in elsewhere.
    let carol = Fixture::named(name).response()["identity"]["identity_id"]
        .as_str()
        .expect("the fixture names a target")
        .to_owned();
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request(
            "GET",
            &format!("/api/lookup/{carol}?from={ALICE}"),
            &Value::Null,
        ),
    )
    .await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::Lookup(LookupRequest {
            identity_id: id(&carol),
            from: Some(id(ALICE)),
        })
    );
    // Who trusts an identity is who this crawl read, and says so every time.
    assert_eq!(body["reverse"]["best_effort"], json!(true));
    assert_eq!(body["paths"][0]["hops"][0]["stale"], json!(false));
}

#[tokio::test]
async fn a_lookup_without_from_leaves_the_default_to_the_service() {
    let stub = Arc::new(StubNodeService::new());
    let request = request("GET", &format!("/api/lookup/{BOB}"), &Value::Null);
    let (status, _) = send(node(&stub), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::Lookup(LookupRequest {
            identity_id: id(BOB),
            from: None,
        })
    );
}

#[tokio::test]
async fn wallet_get_graph_matches_the_fixture() {
    let name = "wallet-get-graph.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::Graph);
    assert_eq!(body["graph"]["truncated_by"], json!("depth"));
}

#[tokio::test]
async fn wallet_post_graph_sync_matches_the_fixture() {
    let name = "wallet-post-graph-sync.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::SyncGraph);
    assert_eq!(
        body["graph"]["sync_id"],
        Fixture::named("wallet-get-graph.json").response()["graph"]["sync_id"],
        "both graph routes return one object"
    );
}

#[tokio::test]
async fn wallet_post_identity_witnesses_matches_the_fixture() {
    let name = "wallet-post-identity-witnesses.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::SetWitnesses(id(ALICE), vec![id(WITNESS_ONE), id(WITNESS_TWO)])
    );
}

/// The advertisement route appends one `EndpointAdvertisement`, whole
/// replacement, and answers the same `Appended` document the witness route
/// does with the other payload (proposal 006 section 8).
#[tokio::test]
async fn wallet_post_identity_endpoints_matches_the_fixture() {
    let name = "wallet-post-identity-endpoints.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::SetEndpoints(id(ALICE), vec![id(NODE_ENDPOINT), id(SECOND_MACHINE)])
    );
    assert_eq!(
        body["event"]["payload_kind"],
        json!("endpoint_advertisement")
    );
    assert_eq!(
        body["event"]["payload"]["endpoints"],
        json!([NODE_ENDPOINT, SECOND_MACHINE])
    );
}

#[tokio::test]
async fn wallet_get_identity_memberships_matches_the_fixture() {
    let name = "wallet-get-identity-memberships.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::Memberships(id(ALICE)));
    // Every ledger carries a principal set, raw-rooted or identity-rooted
    // (proposal 002 section 1).
    assert_eq!(body["root"], json!("raw"));
    assert_eq!(body["principals"][0]["is_root"], json!(true));
    assert_eq!(body["invitations"][0]["status"], json!("open"));
}

#[tokio::test]
async fn wallet_post_membership_invitations_matches_the_fixture() {
    let name = "wallet-post-membership-invitations.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        NodeCall::Invite(request) => {
            assert_eq!(request.ledger_id, id(ALICE));
            assert_eq!(request.by, id(ALICE));
            assert_eq!(request.role, RoleName::Controller);
            // The descriptor reaches the service decoded, never as base64.
            assert_eq!(request.invitee_descriptor.len(), 72);
        }
        call => panic!("{call:?}"),
    }
    assert_eq!(
        body["event"]["payload_kind"],
        json!("membership_invitation")
    );
}

#[tokio::test]
async fn wallet_post_membership_acceptances_matches_the_fixture_and_warns_on_a_raw_root() {
    let name = "wallet-post-membership-acceptances.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        NodeCall::AcceptInvitation(request) => {
            assert_eq!(request.identity_id, id(ALICE));
            assert_eq!(request.invitation_bundle.len(), 96);
        }
        call => panic!("{call:?}"),
    }
    // Accepting a controller role on a raw-rooted ledger means signing as
    // that identity, and the surface says so (proposal 002 section 4).
    assert_eq!(body["controller_on_raw_root"], json!(true));
    assert!(
        body["warning"]
            .as_str()
            .expect("a warning beside the flag")
            .contains("signing as")
    );
}

#[tokio::test]
async fn wallet_post_membership_admissions_matches_the_fixture() {
    let name = "wallet-post-membership-admissions.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        NodeCall::AdmitAcceptance(request) => {
            assert_eq!(request.ledger_id, id(ALICE));
            assert_eq!(request.by, id(ALICE));
            assert_eq!(request.acceptance.len(), 72);
        }
        call => panic!("{call:?}"),
    }
    assert_eq!(
        body["event"]["payload_kind"],
        json!("membership_acceptance")
    );
}

#[tokio::test]
async fn wallet_post_membership_removals_matches_the_fixture() {
    let name = "wallet-post-membership-removals.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        NodeCall::RemoveMembership(request) => {
            assert_eq!(request.ledger_id, id(ALICE));
            assert_eq!(request.by, id(ALICE));
            assert_eq!(request.target, id(BOB));
        }
        call => panic!("{call:?}"),
    }
    assert_eq!(body["event"]["payload_kind"], json!("membership_removal"));
    // A removal that cancels no open invitation says so with null, not by
    // dropping the key (`contracts/README.md`, "Nullability").
    assert!(body["invitation_cancelled"].is_null());
}

#[tokio::test]
async fn wallet_post_trust_matches_the_fixture() {
    let name = "wallet-post-trust.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        NodeCall::AddTrust(request) => {
            assert_eq!(request.issuer, id(ALICE));
            assert_eq!(request.subject, id(BOB));
        }
        call => panic!("{call:?}"),
    }
}

#[tokio::test]
async fn wallet_post_trust_revoke_matches_the_fixture() {
    let name = "wallet-post-trust-revoke.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::RevokeTrust(id(ATTESTATION), id(ALICE))
    );
}

#[tokio::test]
async fn wallet_post_sync_push_matches_the_fixture() {
    let name = "wallet-post-sync-push.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::Push(PushRequest {
            identity_id: id(ALICE),
            to: None
        })
    );
    // One witness unreachable still answers 200 with the failure in results.
    assert_eq!(body["results"][1]["status"], json!("unreachable"));
}

#[tokio::test]
async fn wallet_post_identity_fetch_matches_the_fixture() {
    let name = "wallet-post-identity-fetch.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::FetchIdentity(FetchIdentity {
            from_witness: None,
            identity_id: id(ALICE),
            from: Some(id(ENDPOINT_ONE)),
        })
    );
    // The route answers the document `mabel sync fetch --json` prints, down
    // to the key that says whether this home may now append.
    assert_eq!(body["stored"], json!(4));
    assert!(body["controlled_by"].is_null());
}

#[tokio::test]
async fn a_fetch_without_a_source_leaves_the_witness_choice_to_the_service() {
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{BOB}/fetch");
    let (status, _) = send(node(&stub), request("POST", &uri, &json!({"from": null}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::FetchIdentity(FetchIdentity {
            from_witness: None,
            identity_id: id(BOB),
            from: None,
        })
    );
}

#[tokio::test]
async fn wallet_get_resolve_matches_the_fixture_and_passes_the_hostname_through() {
    let name = "wallet-get-resolve.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::Resolve(ResolveInput::Hostname(HOSTNAME.to_owned()))
    );
    assert_eq!(body["input_kind"], json!("hostname"));
    assert_eq!(body["status"], json!("resolved"));
    assert_eq!(body["identity_id"], json!(ALICE));

    // A hostname the profile rule refuses never reaches the resolver.
    let (expected_status, expected) = fixture_error(name, "malformed_hostname");
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request(
            "GET",
            "/api/resolve?input=alice_ashworth.example",
            &Value::Null,
        ),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
    assert!(stub.calls().is_empty());
}

/// The other two input kinds reach the service as themselves, and the link's
/// hints reach it in the order the link named them (proposal 006 section 7).
#[tokio::test]
async fn wallet_get_resolve_takes_an_identity_id_and_a_link() {
    let stub = Arc::new(StubNodeService::new());
    let (status, _) = send(
        node(&stub),
        request("GET", &format!("/api/resolve?input={ALICE}"), &Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::Resolve(ResolveInput::Identity(id(ALICE)))
    );

    let stub = Arc::new(StubNodeService::new());
    let uri = format!(
        "/api/resolve?input=mabel%3A%2F%2F{ALICE}%3Fendpoints%3D{ENDPOINT_ONE}%2C{ENDPOINT_TWO}"
    );
    let (status, _) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::Resolve(ResolveInput::Link {
            identity_id: id(ALICE),
            endpoints: vec![id(ENDPOINT_ONE), id(ENDPOINT_TWO)],
        })
    );
}

/// `%252f` decodes once to `%2f` and is refused as a link, not decoded again
/// into a path separator. The refusal names the string the layer received.
#[tokio::test]
async fn wallet_get_resolve_refuses_a_double_encoded_link_and_a_repeated_input() {
    let name = "wallet-get-resolve.json";
    for (reason, query) in [
        (
            "invalid_mabel_link",
            format!("input=mabel%3A%2F%2F{ALICE}%252f"),
        ),
        (
            "unknown_query_parameter",
            format!("input={ALICE}&input=alice.example"),
        ),
        ("missing_field", "input=".to_owned()),
    ] {
        let (expected_status, expected) = fixture_error(name, reason);
        let stub = Arc::new(StubNodeService::new());
        let uri = format!("/api/resolve?{query}");
        let (status, body) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
        assert_eq!(status, expected_status, "{reason}");
        assert_eq!(body, expected, "{reason}");
        assert!(stub.calls().is_empty(), "{reason}");
    }
}

#[tokio::test]
async fn wallet_get_witnesses_matches_the_fixture_and_says_where_each_is_known_from() {
    let name = "wallet-get-witnesses.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::Witnesses);

    let witnesses = body["witnesses"].as_array().expect("an array");
    let identities: Vec<&str> = witnesses
        .iter()
        .map(|witness| witness["identity_id"].as_str().expect("an identity id"))
        .collect();
    let mut sorted = identities.clone();
    sorted.sort_unstable();
    assert_eq!(
        identities, sorted,
        "witnesses sort by ascending identity id"
    );
    // A witness identity two ledgers name carries both, and each machine
    // carries the binding of proposal 006 section 4.2.
    let shared = witnesses
        .iter()
        .find(|witness| witness["named_by"] == json!([ACME, ALICE]))
        .expect("the fixture lists a witness two ledgers name");
    assert_eq!(shared["endpoints"][0]["endpoint_id"], json!(ENDPOINT_ONE));
    assert_eq!(shared["endpoints"][0]["binding"], json!("verified"));
    assert!(
        witnesses.iter().any(|witness| witness["endpoints"]
            .as_array()
            .expect("an array")
            .iter()
            .any(|machine| machine["binding"] == json!("hinted"))),
        "{witnesses:?}"
    );
}

#[tokio::test]
async fn wallet_get_witness_holdings_matches_the_fixture_and_pages_like_a_list() {
    let name = "wallet-get-witness-holdings.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::WitnessHoldings(
            id(ALICE),
            PageRequest {
                offset: 0,
                limit: 256
            }
        )
    );
    // The proxy carries what `List` serves, so the three fields that come
    // from the answering node's own meta.json are absent.
    let entry = body["ledgers"][0].as_object().expect("a row");
    assert_eq!(entry.len(), 6, "{entry:?}");
    for absent in ["source_endpoint", "first_seen_ms", "forks_truncated"] {
        assert!(!entry.contains_key(absent), "{absent} is in {entry:?}");
    }

    let (expected_status, expected) = fixture_error(name, "malformed_identity_id");
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request("GET", "/api/witnesses/witness-one/holdings", &Value::Null),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
    assert!(stub.calls().is_empty());
}

/// The old drill-in path is gone, so a client sending an endpoint id where an
/// identity id belongs gets a 404 rather than a dial that finds nothing
/// (proposal 006 section 8).
#[tokio::test]
async fn the_old_witness_ledgers_path_is_no_longer_a_route() {
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/witnesses/{ENDPOINT_ONE}/ledgers");
    let (status, body) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["details"]["reason"], json!("unknown_route"));
    assert!(stub.calls().is_empty());
}

/// `GET /api/ledgers` and its two drill-ins are answered by the identity routes
/// (proposal 006 section 8).
#[tokio::test]
async fn the_ledger_routes_are_gone() {
    let stub = Arc::new(StubNodeService::new());
    for path in [
        "/api/ledgers".to_owned(),
        format!("/api/ledgers/{ALICE}"),
        format!("/api/ledgers/{ALICE}/events"),
    ] {
        let (status, body) = send(node(&stub), request("GET", &path, &Value::Null)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body["details"]["reason"], json!("unknown_route"), "{path}");
    }
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_holdings_limit_over_the_maximum_reaches_the_service_clamped() {
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/witnesses/{ALICE}/holdings?offset=8&limit=100000");
    let (status, _) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::WitnessHoldings(
            id(ALICE),
            PageRequest {
                offset: 8,
                limit: 256
            }
        ),
        "the proxy clamps to the maximum a List answers"
    );
}

#[tokio::test]
async fn there_is_no_verify_route() {
    // Proposal 004 removed `POST /api/verify` with the verify tab; verifying
    // trust and ledgers is a CLI concern.
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(node(&stub), request("POST", "/api/verify", &json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["details"]["reason"], json!("unknown_route"));
    assert!(stub.calls().is_empty());
}

// ------------------------------------------------------- one node router ----

/// One document for `GET /api/node`, with no role in it: what this node can do
/// is `identity_count` and `witness_for` (proposal 006 section 8).
#[tokio::test]
async fn node_get_node_matches_the_fixture_and_names_no_role() {
    let name = "node-get-node.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), NodeCall::Node);
    assert_eq!(body["storage_capacity"], json!(2_147_483_648_u64));
    let document = body.as_object().expect("a document");
    assert!(!document.contains_key("role"), "{document:?}");
    assert!(document.contains_key("identity_count") && document.contains_key("witness_for"));
    let entry = &body["witness_for"][0];
    assert_eq!(entry["advertised"], json!(true));
    assert!(entry["reason"].is_null());
}

/// `GET /api/forks` is a node route on every node: a fork is a fact about a
/// stored ledger, and no other route reports it.
#[tokio::test]
async fn node_get_forks_matches_the_fixture() {
    let name = "node-get-forks.json";
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = run(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        NodeCall::Forks(ForkQuery {
            ledger_id: None,
            page: PageRequest {
                offset: 0,
                limit: 64
            }
        })
    );
}

/// Paging on `GET /api/identities/known`: the default, the maximum, and a value
/// over it, which is clamped rather than refused (proposal 006 section 8).
#[tokio::test]
async fn known_identities_pages_at_the_default_the_maximum_and_over_it() {
    for (query, expected) in [
        (
            "",
            PageRequest {
                offset: 0,
                limit: 100,
            },
        ),
        (
            "?limit=256",
            PageRequest {
                offset: 0,
                limit: 256,
            },
        ),
        (
            "?offset=100&limit=100000",
            PageRequest {
                offset: 100,
                limit: 256,
            },
        ),
    ] {
        let stub = Arc::new(StubNodeService::new());
        let uri = format!("/api/identities/known{query}");
        let (status, body) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        assert_eq!(stub.call(), NodeCall::KnownIdentities(expected), "{query}");
        assert!(body["identities"].is_array(), "{query}");
        for key in ["offset", "limit", "more"] {
            assert!(body.get(key).is_some(), "{key} is absent for {query}");
        }
    }
}

// ------------------------------------------------------------- coverage -----

#[test]
fn every_file_under_contracts_http_has_a_fixture_and_a_test() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/http");
    let mut on_disk: Vec<String> = std::fs::read_dir(&directory)
        .expect("contracts/http exists")
        .map(|entry| entry.expect("a dir entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json"))
        .collect();
    on_disk.sort();
    let mut compiled_in: Vec<String> = FIXTURES
        .iter()
        .map(|fixture| fixture.name.to_owned())
        .collect();
    compiled_in.sort();
    assert_eq!(
        on_disk, compiled_in,
        "a fixture was added or removed without a test in this file"
    );
}

/// The index of `contracts/README.md` names every fixture and no file that is
/// gone: a renamed fixture with a stale row sends a reader to a file that does
/// not exist.
#[test]
fn the_readme_index_names_every_fixture_and_nothing_else() {
    const README: &str = include_str!("../../../../contracts/README.md");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts");

    let mut on_disk: Vec<String> = ["http", "cli"]
        .into_iter()
        .flat_map(|directory| {
            std::fs::read_dir(root.join(directory))
                .unwrap_or_else(|error| panic!("contracts/{directory}: {error}"))
                .map(move |entry| {
                    let name = entry.expect("a dir entry").file_name();
                    format!("{directory}/{}", name.to_string_lossy())
                })
        })
        .filter(|name| name.ends_with(".json"))
        .collect();
    on_disk.sort();

    // Every index row opens `| `<directory>/<file>.json` |`.
    let mut indexed: Vec<String> = README
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split_once("` |").map(|(name, _)| name))
        .filter(|name| {
            (name.starts_with("http/") || name.starts_with("cli/")) && name.ends_with(".json")
        })
        .map(ToOwned::to_owned)
        .collect();
    indexed.sort();

    assert_eq!(
        on_disk, indexed,
        "contracts/README.md and contracts/ disagree about which fixtures exist"
    );
}

#[test]
fn both_identity_routes_return_one_document_with_explicit_nulls() {
    // Proposal 003 section 5: the list rows and the show document are one
    // shape, so the UI has one type and one renderer.
    let show = Fixture::named("wallet-get-identity.json").response()["identity"].clone();
    let list = Fixture::named("wallet-get-identities.json").response()["identities"].clone();
    let created = Fixture::named("wallet-post-identities.json").response()["identity"].clone();
    let rows = list.as_array().expect("an array");

    let keys = |value: &Value| {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an identity document")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    // `active_key` and `reserve_commit` are the documented exception: a ledger
    // holding no key of its own omits them rather than nulling them.
    let raw_rooted = |value: &Value| {
        keys(value)
            .into_iter()
            .filter(|key| key != "active_key" && key != "reserve_commit")
            .collect::<Vec<String>>()
    };
    for row in rows {
        assert_eq!(raw_rooted(row), raw_rooted(&show), "{row}");
    }
    assert_eq!(raw_rooted(&created), raw_rooted(&show));

    for document in rows.iter().chain([&show, &created]) {
        let object = document.as_object().expect("an identity document");
        for key in ["profile", "verification", "contact"] {
            assert!(object.contains_key(key), "{key} is absent from {document}");
        }
        let verification = object["verification"]
            .as_object()
            .expect("verification is always an object");
        assert_eq!(
            keys(&object["verification"]).len(),
            7,
            "{verification:?} must carry every key with an explicit null"
        );
    }
}

#[test]
fn the_envelope_reproduces_every_case_of_the_cli_error_fixture() {
    // One table covers both surfaces, so the layer, the prefix and the exit
    // code this module uses must reproduce `contracts/cli/errors.json`
    // exactly. That file pins no HTTP status, so the status below is ignored.
    const ERRORS: &str = include_str!("../../../../contracts/cli/errors.json");
    let fixture: Value = serde_json::from_str(ERRORS).expect("valid JSON");
    let cases = fixture["cases"].as_array().expect("cases");
    assert_eq!(
        cases.len(),
        15,
        "one case per code and layer prefix, plus the six code 2 reasons \
         proposal 006 added"
    );
    for case in cases {
        let name = case["case"].as_str().expect("a case name");
        let document = &case["document"];
        let exit_code = case["exit_code"].as_u64().expect("an exit code");
        assert_eq!(document["code"].as_u64(), Some(exit_code), "{name}");
        let error = error_from_fixture(StatusCode::OK, document);
        assert_eq!(u64::from(error.code()), exit_code, "{name}");
        assert_eq!(error.to_document(), *document, "{name}");
    }
}

#[test]
fn every_fixture_carries_the_route_and_method_its_test_uses() {
    for fixture in FIXTURES {
        let route = fixture.route();
        assert!(route.starts_with("/api/"), "{}: {route}", fixture.name);
        assert!(
            matches!(fixture.method().as_str(), "GET" | "POST" | "PUT"),
            "{}: {}",
            fixture.name,
            fixture.method()
        );
        // Nothing in the frozen set needs a path parameter this file cannot
        // fill in.
        let concrete = concrete_route(&route);
        assert!(!concrete.contains(':'), "{}: {concrete}", fixture.name);
    }
}

#[tokio::test]
async fn every_service_error_example_renders_as_the_fixture_pins_it() {
    let mut checked = 0;
    for fixture in FIXTURES {
        for (status, expected) in fixture.errors() {
            let reason = expected["details"]["reason"]
                .as_str()
                .expect("a reason")
                .to_owned();
            if API_OWNED_REASONS.contains(&reason.as_str()) {
                continue;
            }
            let status = StatusCode::from_u16(status).expect("a status");
            let error = error_from_fixture(status, &expected);
            let stub = Arc::new(StubNodeService::new());
            stub.fail_with(error);
            let (answered, body) = run(fixture.name, &stub).await;
            assert_eq!(answered, status, "{} {reason}", fixture.name);
            assert_eq!(body, expected, "{} {reason}", fixture.name);
            checked += 1;
        }
    }
    assert!(checked >= 12, "only {checked} service errors were checked");
}

// ------------------------------------------------- errors this layer owns ----

#[tokio::test]
async fn a_host_that_is_not_loopback_answers_the_fixture_rejection() {
    for (fixture, host) in [
        ("node-get-node.json", "evil.example"),
        ("wallet-get-identities.json", "localhost.example"),
    ] {
        let (expected_status, expected) = fixture_error(fixture, "host_not_loopback");
        let stub = Arc::new(StubNodeService::new());
        let uri = concrete_route(&Fixture::named(fixture).route());
        let request = Request::builder()
            .uri(uri)
            .header(header::HOST, host)
            .body(Body::empty())
            .expect("a request");
        let (status, body) = send(node(&stub), request).await;
        assert_eq!(status, expected_status, "{host}");
        assert_eq!(body, expected, "{host}");
        assert!(stub.calls().is_empty(), "the service must not be reached");
    }
}

/// An operator who allowed a host reaches every route under that name, and the
/// hosts they did not allow are refused exactly as before (decision 018).
#[tokio::test]
async fn an_allowed_host_reaches_the_node_routes_and_no_other_host_does() {
    let allowing = || options().with_allowed_hosts(["wallet.tailnet.example"]);
    let stub = Arc::new(StubNodeService::new());
    let router = || {
        node_router(
            Arc::clone(&stub) as Arc<dyn super::NodeService>,
            &allowing(),
        )
    };

    let read = Request::builder()
        .uri("/api/node")
        .header(header::HOST, "wallet.tailnet.example")
        .body(Body::empty())
        .expect("a request");
    let (status, _) = send(router(), read).await;
    assert_eq!(status, StatusCode::OK);

    // The https origin a reverse proxy sends is accepted on a mutating route.
    let write = Request::builder()
        .method("POST")
        .uri("/api/identities")
        .header(header::HOST, "wallet.tailnet.example")
        .header(header::ORIGIN, "https://wallet.tailnet.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"alias": "alice"}).to_string()))
        .expect("a request");
    let (status, _) = send(router(), write).await;
    assert_ne!(status, StatusCode::FORBIDDEN);

    let refused = Request::builder()
        .uri("/api/node")
        .header(header::HOST, "evil.example")
        .body(Body::empty())
        .expect("a request");
    let (status, body) = send(router(), refused).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["details"]["reason"], json!("host_not_loopback"));
    assert_eq!(
        body["message"],
        json!(
            "request rejected: Host header must be 127.0.0.1:9080, localhost:9080 \
             or wallet.tailnet.example"
        )
    );
}

#[tokio::test]
async fn the_right_host_on_the_wrong_port_is_rejected() {
    let (expected_status, expected) = fixture_error("node-get-forks.json", "host_not_loopback");
    assert_eq!(expected["details"]["host"], json!("127.0.0.1:9999"));
    let stub = Arc::new(StubNodeService::new());
    let request = Request::builder()
        .uri("/api/node")
        .header(header::HOST, "127.0.0.1:9999")
        .body(Body::empty())
        .expect("a request");
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn a_mismatched_origin_on_a_mutating_route_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identities.json", "origin_mismatch");
    let stub = Arc::new(StubNodeService::new());
    let request = Request::builder()
        .method("POST")
        .uri("/api/identities")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "https://attacker.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"alias": "alice"}).to_string()))
        .expect("a request");
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_content_type_that_is_not_json_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identities.json", "content_type_not_json");
    let stub = Arc::new(StubNodeService::new());
    let request = Request::builder()
        .method("POST")
        .uri("/api/identities")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("alias=alice"))
        .expect("a request");
    let (status, body) = send(node(&stub), request).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn every_mutating_route_enforces_the_origin_and_content_type_rules() {
    let routes = [
        "/api/identities",
        &format!("/api/identities/{ALICE}/profile"),
        &format!("/api/identities/{ALICE}/verification"),
        &format!("/api/identities/{ALICE}/contact"),
        "/api/graph/sync",
        &format!("/api/identities/{ALICE}/witnesses"),
        &format!("/api/identities/{ALICE}/endpoints"),
        &format!("/api/identities/{ALICE}/fetch"),
        &format!("/api/identities/{ALICE}/memberships/invitations"),
        &format!("/api/identities/{ALICE}/memberships/acceptances"),
        &format!("/api/identities/{ALICE}/memberships/admissions"),
        &format!("/api/identities/{ALICE}/memberships/removals"),
        "/api/trust",
        &format!("/api/trust/{ATTESTATION}/revoke"),
        "/api/sync/push",
    ]
    .map(ToOwned::to_owned);

    // Every mutating route the fixtures freeze is in the table.
    for fixture in FIXTURES {
        if fixture.method() != "GET" {
            let route = concrete_route(&fixture.route());
            assert!(routes.contains(&route), "{route} is not in the table");
        }
    }

    for route in &routes {
        let stub = Arc::new(StubNodeService::new());
        let absent_origin = Request::builder()
            .method("POST")
            .uri(route)
            .header(header::HOST, HOST)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("a request");
        let (status, body) = send(node(&stub), absent_origin).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}");
        assert_eq!(
            body["details"]["reason"],
            json!("origin_missing"),
            "{route}"
        );
        assert_eq!(body["code"], json!(2), "{route}");

        let wrong_origin = Request::builder()
            .method("POST")
            .uri(route)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, "http://localhost:9999")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("a request");
        let (status, body) = send(node(&stub), wrong_origin).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}");
        assert_eq!(
            body["details"]["reason"],
            json!("origin_mismatch"),
            "{route}"
        );

        let form_post = Request::builder()
            .method("POST")
            .uri(route)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from("{}"))
            .expect("a request");
        let (status, body) = send(node(&stub), form_post).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{route}");
        assert_eq!(
            body["details"]["reason"],
            json!("content_type_not_json"),
            "{route}"
        );

        let no_content_type = Request::builder()
            .method("POST")
            .uri(route)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, ORIGIN)
            .body(Body::from("{}"))
            .expect("a request");
        let (status, body) = send(node(&stub), no_content_type).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{route}");
        assert_eq!(
            body["details"]["reason"],
            json!("content_type_missing"),
            "{route}"
        );

        // The same route with the right headers reaches the handler. Whether
        // the empty body then validates is each route's own business.
        let allowed = request("POST", route, &json!({}));
        let (status, _) = send(node(&stub), allowed).await;
        assert!(
            status != StatusCode::FORBIDDEN && status != StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{route} answered {status} to a well-formed request"
        );
    }
}

#[tokio::test]
async fn a_malformed_path_id_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-get-identity.json", "malformed_identity_id");
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request("GET", "/api/identities/alice", &Value::Null),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn a_malformed_fork_filter_answers_the_fixture_rejection() {
    let (expected_status, expected) = fixture_error("node-get-forks.json", "malformed_ledger_id");
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request("GET", "/api/forks?ledger_id=sfttwjzd", &Value::Null),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn a_malformed_query_parameter_answers_the_fixture_rejection() {
    let (expected_status, expected) = fixture_error(
        "wallet-get-identity-ledger.json",
        "malformed_query_parameter",
    );
    assert_eq!(expected["details"]["value"], json!("-1"));
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{ALICE}/ledger?since=-1");
    let (status, body) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);

    let (expected_status, expected) = fixture_error(
        "wallet-get-known-identities.json",
        "malformed_query_parameter",
    );
    assert_eq!(expected["details"]["parameter"], json!("limit"));
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(
        node(&stub),
        request("GET", "/api/identities/known?limit=all", &Value::Null),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn a_body_the_fixture_refuses_answers_the_fixture_rejection() {
    let invitations = format!("/api/identities/{ALICE}/memberships/invitations");
    let acceptances = format!("/api/identities/{ALICE}/memberships/acceptances");
    let admissions = format!("/api/identities/{ALICE}/memberships/admissions");
    let removals = format!("/api/identities/{ALICE}/memberships/removals");
    let cases: [(&str, &str, &str, Value); 8] = [
        (
            "wallet-post-identities.json",
            "missing_field",
            "/api/identities",
            json!({"declared_kind": "person"}),
        ),
        (
            "wallet-post-identities.json",
            "unknown_enum_value",
            "/api/identities",
            json!({"alias": "alice", "declared_kind": "human"}),
        ),
        (
            "wallet-post-identities.json",
            "unsupported_declared_kind",
            "/api/identities",
            json!({"alias": "ada", "declared_kind": "agent"}),
        ),
        (
            "wallet-post-trust.json",
            "subject_equals_ledger",
            "/api/trust",
            json!({"issuer": ALICE, "subject": ALICE}),
        ),
        (
            "wallet-post-membership-invitations.json",
            "missing_field",
            &invitations,
            json!({"role": "controller", "invitee_descriptor_base64": "AAAA"}),
        ),
        (
            "wallet-post-membership-acceptances.json",
            "missing_field",
            &acceptances,
            json!({}),
        ),
        (
            "wallet-post-membership-admissions.json",
            "missing_field",
            &admissions,
            json!({"by": ALICE}),
        ),
        (
            "wallet-post-membership-removals.json",
            "missing_field",
            &removals,
            json!({"by": ALICE}),
        ),
    ];
    for (fixture, reason, route, body) in cases {
        let (expected_status, expected) = fixture_error(fixture, reason);
        let stub = Arc::new(StubNodeService::new());
        let (status, answered) = send(node(&stub), request("POST", route, &body)).await;
        assert_eq!(status, expected_status, "{fixture} {reason}");
        assert_eq!(answered, expected, "{fixture} {reason}");
        assert!(stub.calls().is_empty(), "{fixture} {reason}");
    }
}

#[tokio::test]
async fn a_duplicate_witness_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identity-witnesses.json", "duplicate_witness");
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{ALICE}/witnesses");
    let body = json!({"witnesses": [WITNESS_ONE, WITNESS_ONE]});
    let (status, answered) = send(node(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, expected_status);
    assert_eq!(answered, expected);
}

#[tokio::test]
async fn a_duplicate_endpoint_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identity-endpoints.json", "duplicate_endpoint");
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{ALICE}/endpoints");
    let body = json!({"endpoints": [NODE_ENDPOINT, NODE_ENDPOINT]});
    let (status, answered) = send(node(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, expected_status);
    assert_eq!(answered, expected);
    assert!(stub.calls().is_empty(), "the service must not be reached");
}

/// The list is a whole replacement, so an absent `endpoints` key is refused
/// rather than read as "change nothing".
#[tokio::test]
async fn an_advertisement_with_no_endpoints_key_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identity-endpoints.json", "missing_field");
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{ALICE}/endpoints");
    let (status, answered) = send(node(&stub), request("POST", &uri, &json!({}))).await;
    assert_eq!(status, expected_status);
    assert_eq!(answered, expected);
    assert!(stub.calls().is_empty(), "the service must not be reached");
}

// ------------------------------------------------------- route boundaries ----

#[tokio::test]
async fn there_is_no_orgs_collection() {
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(node(&stub), request("GET", "/api/orgs", &Value::Null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], json!(2));
    assert_eq!(body["details"]["reason"], json!("unknown_route"));
    assert_eq!(body["details"]["path"], json!("/api/orgs"));

    let stub = Arc::new(StubNodeService::new());
    let (status, _) = send(
        node(&stub),
        request("GET", "/api/organizations", &Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_membership_routes_spell_memberships_and_answer_the_frozen_documents() {
    // The four verbs live under one path segment, and nothing under `/orgs`
    // or `/organizations` answers (proposal 002 section 6).
    for (route, fixture) in [
        (
            "memberships/invitations",
            "wallet-post-membership-invitations.json",
        ),
        (
            "memberships/acceptances",
            "wallet-post-membership-acceptances.json",
        ),
        (
            "memberships/admissions",
            "wallet-post-membership-admissions.json",
        ),
        (
            "memberships/removals",
            "wallet-post-membership-removals.json",
        ),
    ] {
        let stub = Arc::new(StubNodeService::new());
        let uri = format!("/api/identities/{ALICE}/{route}");
        let body = Fixture::named(fixture).request();
        let (status, answered) = send(node(&stub), request("POST", &uri, &body)).await;
        assert_eq!(status, StatusCode::OK, "{route}: {answered}");
        assert_eq!(answered, Fixture::named(fixture).response(), "{route}");
        assert_eq!(stub.calls().len(), 1, "{route}");
    }
}

#[tokio::test]
async fn a_membership_artifact_that_is_not_base64_answers_the_fixture_rejection() {
    let (expected_status, expected) = fixture_error(
        "wallet-post-membership-invitations.json",
        "malformed_base64",
    );
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{ALICE}/memberships/invitations");
    let body = json!({
        "by": ALICE,
        "role": "controller",
        "invitee_descriptor_base64": "not base64!"
    });
    let (status, answered) = send(node(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, expected_status);
    assert_eq!(answered, expected);
    assert!(stub.calls().is_empty(), "the service must not be reached");
}

#[tokio::test]
async fn a_membership_artifact_over_its_cap_is_refused_before_it_is_decoded() {
    let stub = Arc::new(StubNodeService::new());
    let uri = format!("/api/identities/{ALICE}/memberships/admissions");
    // An `AcceptanceFile` is capped at 4 KiB (proposal 001 section 3.8).
    let oversize = "A".repeat(4 * 4096 / 3 + 8);
    let body = json!({"by": ALICE, "acceptance_base64": oversize});
    let (status, answered) = send(node(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(answered["code"], json!(10));
    assert_eq!(answered["details"]["reason"], json!("message_too_large"));
    assert_eq!(answered["details"]["artifact"], json!("AcceptanceFile"));
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_read_only_route_refuses_a_mutation_with_405() {
    let stub = Arc::new(StubNodeService::new());
    let (status, body) = send(node(&stub), request("POST", "/api/forks", &json!({}))).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["code"], json!(2));
    assert_eq!(body["details"]["reason"], json!("method_not_allowed"));
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_since_at_the_head_sequence_reaches_the_service_unchanged() {
    let stub = Arc::new(StubNodeService::new());
    let head_seq = stub.identity_ledger.head_seq;
    let uri = format!("/api/identities/{ALICE}/ledger?since={head_seq}");
    let (status, _) = send(node(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::IdentityLedger(
            id(ALICE),
            EventPageRequest {
                since: head_seq,
                limit: 512
            }
        )
    );
}

#[tokio::test]
async fn a_limit_over_the_maximum_reaches_the_service_clamped() {
    let stub = Arc::new(StubNodeService::new());
    let (status, _) = send(
        node(&stub),
        request("GET", "/api/forks?limit=100000", &Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        NodeCall::Forks(ForkQuery {
            ledger_id: None,
            page: PageRequest {
                offset: 0,
                limit: 64
            }
        })
    );
}

#[tokio::test]
async fn the_ui_bundle_serves_outside_api_and_never_inside_it() {
    let directory = tempfile::tempdir().expect("a temp dir");
    std::fs::write(directory.path().join("index.html"), "<!doctype html>").expect("write");
    let options = ApiOptions::default().with_ui(UiSource::Directory(directory.path().into()));
    let service: Arc<dyn super::NodeService> = Arc::new(StubNodeService::new());

    for path in ["/", "/wallet", "/witness"] {
        let router = node_router(Arc::clone(&service), &options);
        let response = router
            .oneshot(request("GET", path, &Value::Null))
            .await
            .expect("infallible");
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let bytes = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("a body");
        assert_eq!(&bytes[..], b"<!doctype html>", "{path}");
    }

    let router = node_router(Arc::clone(&service), &options);
    let (status, body) = send(router, request("GET", "/api/nope", &Value::Null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["details"]["reason"], json!("unknown_route"));
}

#[tokio::test]
async fn the_loopback_rules_guard_the_ui_and_the_unknown_routes_too() {
    let stub = Arc::new(StubNodeService::new());
    for path in ["/", "/api/nope"] {
        let request = Request::builder()
            .uri(path)
            .header(header::HOST, "evil.example")
            .body(Body::empty())
            .expect("a request");
        let (status, body) = send(node(&stub), request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            body["details"]["reason"],
            json!("host_not_loopback"),
            "{path}"
        );
    }
}

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
    EventPageRequest, ForkQuery, LookupRequest, PageRequest, PushRequest, ReplaceProfile,
    SetContact, VerifyRequest,
};
use super::stub::{
    FIXTURES, Fixture, StubWalletService, StubWitnessService, WalletCall, WitnessCall,
};
use super::{ApiOptions, UiSource, documents::Id, wallet_router, witness_router};

const ALICE: &str = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
const BOB: &str = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
const ACME: &str = "2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a";
const ATTESTATION: &str = "65cssg5tnr3gyxe2rwhsgqc3nct3pwg2bqxr2oxpelejuoorlsnq";
const WITNESS_ONE: &str = "zbj22dym2k3btlvjftxmj7kwujgwjgovqthhsjl6ixh5qe43mctq";
const WITNESS_TWO: &str = "5yy7qpeiu4jbtjx47g7obwu3yitcaweplik2mfcvknie36letzoa";

/// The host the loopback rules expect at the default bind.
const HOST: &str = "127.0.0.1:9080";
/// The matching origin.
const ORIGIN: &str = "http://127.0.0.1:9080";

/// The reasons this module produces itself, before any service is called.
/// Every other reason in a fixture's `errors` array comes from a service, and
/// the round-trip test below drives those through the stub.
const API_OWNED_REASONS: [&str; 11] = [
    "host_not_loopback",
    "origin_mismatch",
    "content_type_not_json",
    "missing_field",
    "unknown_enum_value",
    "unsupported_declared_kind",
    "malformed_identity_id",
    "malformed_ledger_id",
    "malformed_query_parameter",
    "malformed_base64",
    "duplicate_witness",
];

fn id(raw: &str) -> Id {
    Id::parse(raw).expect("a fixture id")
}

fn options() -> ApiOptions {
    // The UI has its own tests; here it must not swallow an API path.
    ApiOptions::default().with_ui(UiSource::Disabled)
}

fn wallet(stub: &Arc<StubWalletService>) -> Router {
    wallet_router(
        Arc::clone(stub) as Arc<dyn super::WalletService>,
        &options(),
    )
}

fn witness(stub: &Arc<StubWitnessService>) -> Router {
    witness_router(
        Arc::clone(stub) as Arc<dyn super::WitnessService>,
        &options(),
    )
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
}

/// Runs a fixture's own request against a wallet stub.
async fn run_wallet(name: &str, stub: &Arc<StubWalletService>) -> (StatusCode, Value) {
    let fixture = Fixture::named(name);
    let uri = concrete_route(&fixture.route());
    send(
        wallet(stub),
        request(&fixture.method(), &uri, &fixture.request()),
    )
    .await
}

/// Runs a fixture's own request against a witness stub.
async fn run_witness(name: &str, stub: &Arc<StubWitnessService>) -> (StatusCode, Value) {
    let fixture = Fixture::named(name);
    let uri = concrete_route(&fixture.route());
    send(
        witness(stub),
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
async fn wallet_get_node_matches_the_fixture() {
    let name = "wallet-get-node.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(body["storage_capacity"], json!(2_147_483_648_u64));
    assert_eq!(stub.call(), WalletCall::Node);
}

#[tokio::test]
async fn wallet_get_identities_matches_the_fixture_and_lists_organizations() {
    let name = "wallet-get-identities.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WalletCall::Identities);

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
async fn wallet_post_identities_matches_the_fixture() {
    let name = "wallet-post-identities.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        WalletCall::CreateIdentity(request) => {
            assert_eq!(request.alias, "alice");
            assert_eq!(request.declared_kind, DeclaredKind::Person);
        }
        call => panic!("{call:?}"),
    }
}

#[tokio::test]
async fn wallet_get_identity_matches_the_fixture() {
    let name = "wallet-get-identity.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WalletCall::Identity(id(ALICE)));
}

#[tokio::test]
async fn wallet_get_identity_ledger_matches_the_fixture_and_passes_since_through() {
    let name = "wallet-get-identity-ledger.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WalletCall::IdentityLedger(
            id(ALICE),
            EventPageRequest {
                since: 2,
                limit: 512
            }
        )
    );
    // `?since=` is inclusive: the fixture asks for 2 and gets seq 2 first.
    assert_eq!(body["since"], json!(2));
    assert_eq!(body["events"][0]["seq"], json!(2));
}

#[tokio::test]
async fn wallet_post_identity_profile_matches_the_fixture_and_requires_both_keys() {
    let name = "wallet-post-identity-profile.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WalletCall::ReplaceProfile(ReplaceProfile {
            identity_id: id(ALICE),
            display_name: Some("Alice Ashworth".to_owned()),
            hostname: Some("alice.example".to_owned()),
        })
    );

    // The operation is replacement, so a body naming one key would clear the
    // other by accident (proposal 003 section 1).
    let (expected_status, expected) = fixture_error(name, "missing_field");
    let stub = Arc::new(StubWalletService::new());
    let request = request(
        "POST",
        &format!("/api/identities/{ALICE}/profile"),
        &json!({"display_name": "Alice Ashworth"}),
    );
    let (status, body) = send(wallet(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_profile_body_may_null_either_key() {
    for (request_body, display_name, hostname) in [
        (
            json!({"display_name": null, "hostname": "alice.example"}),
            None,
            Some("alice.example"),
        ),
        (
            json!({"display_name": "Alice Ashworth", "hostname": null}),
            Some("Alice Ashworth"),
            None,
        ),
        (json!({"display_name": null, "hostname": null}), None, None),
    ] {
        let stub = Arc::new(StubWalletService::new());
        let request = request(
            "POST",
            &format!("/api/identities/{ALICE}/profile"),
            &request_body,
        );
        let (status, _) = send(wallet(&stub), request).await;
        assert_eq!(status, StatusCode::OK, "{request_body}");
        assert_eq!(
            stub.call(),
            WalletCall::ReplaceProfile(ReplaceProfile {
                identity_id: id(ALICE),
                display_name: display_name.map(ToOwned::to_owned),
                hostname: hostname.map(ToOwned::to_owned),
            }),
            "{request_body}"
        );
    }
}

#[tokio::test]
async fn wallet_post_identity_verification_matches_the_fixture() {
    let name = "wallet-post-identity-verification.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WalletCall::CheckVerification(id(ALICE)));
    assert_eq!(body["verification"]["status"], json!("verified"));
    assert_eq!(body["verification"]["stale"], json!(false));
}

/// The contact fixtures are about Bob, so their requests name Bob rather than
/// the `ALICE` [`concrete_route`] fills in for every other path parameter: a
/// private note is most itself on a foreign identity.
#[tokio::test]
async fn wallet_get_identity_contact_matches_the_fixture_for_a_foreign_identity() {
    let name = "wallet-get-identity-contact.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = send(
        wallet(&stub),
        request(
            "GET",
            &format!("/api/identities/{BOB}/contact"),
            &Value::Null,
        ),
    )
    .await;
    expect_response(name, status, &body);
    assert_eq!(body["identity_id"], json!(BOB));
    assert_eq!(stub.call(), WalletCall::Contact(id(BOB)));
}

#[tokio::test]
async fn wallet_put_identity_contact_matches_the_fixture_and_caps_a_nickname() {
    let name = "wallet-put-identity-contact.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = send(
        wallet(&stub),
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
        WalletCall::SetContact(SetContact {
            identity_id: id(BOB),
            nickname: Some("bob at the print shop".to_owned()),
            note: Some("met at the 2023 zine fair; verifies his own hostname".to_owned()),
        })
    );

    let (expected_status, expected) = fixture_error(name, "contact_field_too_long");
    let cap = expected["details"]["cap"].as_u64().expect("a cap") as usize;
    let len = expected["details"]["len"].as_u64().expect("a length") as usize;
    let stub = Arc::new(StubWalletService::new());
    let request = request(
        "PUT",
        &format!("/api/identities/{BOB}/contact"),
        &json!({"nickname": "n".repeat(len), "note": null}),
    );
    let (status, body) = send(wallet(&stub), request).await;
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
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = send(
        wallet(&stub),
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
        WalletCall::Lookup(LookupRequest {
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
    let stub = Arc::new(StubWalletService::new());
    let request = request("GET", &format!("/api/lookup/{BOB}"), &Value::Null);
    let (status, _) = send(wallet(&stub), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        WalletCall::Lookup(LookupRequest {
            identity_id: id(BOB),
            from: None,
        })
    );
}

#[tokio::test]
async fn wallet_get_graph_matches_the_fixture() {
    let name = "wallet-get-graph.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WalletCall::Graph);
    assert_eq!(body["graph"]["truncated_by"], json!("depth"));
}

#[tokio::test]
async fn wallet_post_graph_sync_matches_the_fixture() {
    let name = "wallet-post-graph-sync.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WalletCall::SyncGraph);
    assert_eq!(
        body["graph"]["sync_id"],
        Fixture::named("wallet-get-graph.json").response()["graph"]["sync_id"],
        "both graph routes return one object"
    );
}

#[tokio::test]
async fn wallet_post_identity_witnesses_matches_the_fixture() {
    let name = "wallet-post-identity-witnesses.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WalletCall::SetWitnesses(id(ALICE), vec![id(WITNESS_ONE), id(WITNESS_TWO)])
    );
}

#[tokio::test]
async fn wallet_get_identity_memberships_matches_the_fixture() {
    let name = "wallet-get-identity-memberships.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WalletCall::Memberships(id(ALICE)));
    // Every ledger carries a principal set, raw-rooted or identity-rooted
    // (proposal 002 section 1).
    assert_eq!(body["root"], json!("raw"));
    assert_eq!(body["principals"][0]["is_root"], json!(true));
    assert_eq!(body["invitations"][0]["status"], json!("open"));
}

#[tokio::test]
async fn wallet_post_membership_invitations_matches_the_fixture() {
    let name = "wallet-post-membership-invitations.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        WalletCall::Invite(request) => {
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
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        WalletCall::AcceptInvitation(request) => {
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
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        WalletCall::AdmitAcceptance(request) => {
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
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        WalletCall::RemoveMembership(request) => {
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
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    match stub.call() {
        WalletCall::AddTrust(request) => {
            assert_eq!(request.issuer, id(ALICE));
            assert_eq!(request.subject, id(BOB));
        }
        call => panic!("{call:?}"),
    }
}

#[tokio::test]
async fn wallet_post_trust_revoke_matches_the_fixture() {
    let name = "wallet-post-trust-revoke.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WalletCall::RevokeTrust(id(ATTESTATION), id(ALICE))
    );
}

#[tokio::test]
async fn wallet_post_sync_push_matches_the_fixture() {
    let name = "wallet-post-sync-push.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WalletCall::Push(PushRequest {
            identity_id: id(ALICE),
            to: None
        })
    );
    // One witness unreachable still answers 200 with the failure in results.
    assert_eq!(body["results"][1]["status"], json!("unreachable"));
}

#[tokio::test]
async fn wallet_post_verify_matches_the_fixture() {
    let name = "wallet-post-verify.json";
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = run_wallet(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WalletCall::Verify(VerifyRequest::Trust {
            issuer: id(ALICE),
            subject: id(BOB),
            from: None
        })
    );
    assert_eq!(
        body["subject_control"],
        json!(super::documents::SUBJECT_CONTROL_SENTENCE)
    );
    assert_eq!(
        body["verified_means"],
        json!(super::documents::VERIFIED_MEANS_SENTENCE)
    );
}

// --------------------------------------------------------------- witness ----

#[tokio::test]
async fn witness_get_node_matches_the_fixture() {
    let name = "witness-get-node.json";
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = run_witness(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(body["role"], json!("witness"));
    assert_eq!(stub.call(), WitnessCall::Node);
}

#[tokio::test]
async fn witness_get_ledgers_matches_the_fixture() {
    let name = "witness-get-ledgers.json";
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = run_witness(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WitnessCall::Ledgers(PageRequest {
            offset: 0,
            limit: 256
        })
    );
    assert_eq!(body["entries"][0]["declared_kind"], json!("organization"));
}

#[tokio::test]
async fn witness_get_ledger_matches_the_fixture() {
    let name = "witness-get-ledger.json";
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = run_witness(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(stub.call(), WitnessCall::Ledger(id(ALICE)));
}

#[tokio::test]
async fn witness_get_ledger_events_matches_the_fixture() {
    let name = "witness-get-ledger-events.json";
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = run_witness(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WitnessCall::LedgerEvents(
            id(ALICE),
            EventPageRequest {
                since: 0,
                limit: 512
            }
        )
    );
    // A seq-0 event carries `ledger_id` and `prev` as null, not absent.
    let inception = &body["events"][0];
    assert!(inception["ledger_id"].is_null() && inception["prev"].is_null());
}

#[tokio::test]
async fn witness_get_forks_matches_the_fixture() {
    let name = "witness-get-forks.json";
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = run_witness(name, &stub).await;
    expect_response(name, status, &body);
    assert_eq!(
        stub.call(),
        WitnessCall::Forks(ForkQuery {
            ledger_id: None,
            page: PageRequest {
                offset: 0,
                limit: 64
            }
        })
    );
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
    assert_eq!(cases.len(), 9, "one case per code and layer prefix");
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
            let (answered, body) = if fixture.name.starts_with("wallet") {
                let stub = Arc::new(StubWalletService::new());
                stub.fail_with(error);
                run_wallet(fixture.name, &stub).await
            } else {
                let stub = Arc::new(StubWitnessService::new());
                stub.fail_with(error);
                run_witness(fixture.name, &stub).await
            };
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
        ("wallet-get-node.json", "evil.example"),
        ("wallet-get-identities.json", "localhost.example"),
    ] {
        let (expected_status, expected) = fixture_error(fixture, "host_not_loopback");
        let stub = Arc::new(StubWalletService::new());
        let uri = concrete_route(&Fixture::named(fixture).route());
        let request = Request::builder()
            .uri(uri)
            .header(header::HOST, host)
            .body(Body::empty())
            .expect("a request");
        let (status, body) = send(wallet(&stub), request).await;
        assert_eq!(status, expected_status, "{host}");
        assert_eq!(body, expected, "{host}");
        assert!(stub.calls().is_empty(), "the service must not be reached");
    }
}

#[tokio::test]
async fn the_right_host_on_the_wrong_port_is_rejected() {
    let (expected_status, expected) = fixture_error("witness-get-node.json", "host_not_loopback");
    assert_eq!(expected["details"]["host"], json!("127.0.0.1:9999"));
    let stub = Arc::new(StubWitnessService::new());
    let request = Request::builder()
        .uri("/api/node")
        .header(header::HOST, "127.0.0.1:9999")
        .body(Body::empty())
        .expect("a request");
    let (status, body) = send(witness(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn a_mismatched_origin_on_a_mutating_route_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identities.json", "origin_mismatch");
    let stub = Arc::new(StubWalletService::new());
    let request = Request::builder()
        .method("POST")
        .uri("/api/identities")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "https://attacker.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"alias": "alice"}).to_string()))
        .expect("a request");
    let (status, body) = send(wallet(&stub), request).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_content_type_that_is_not_json_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identities.json", "content_type_not_json");
    let stub = Arc::new(StubWalletService::new());
    let request = Request::builder()
        .method("POST")
        .uri("/api/identities")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("alias=alice"))
        .expect("a request");
    let (status, body) = send(wallet(&stub), request).await;
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
        &format!("/api/identities/{ALICE}/memberships/invitations"),
        &format!("/api/identities/{ALICE}/memberships/acceptances"),
        &format!("/api/identities/{ALICE}/memberships/admissions"),
        &format!("/api/identities/{ALICE}/memberships/removals"),
        "/api/trust",
        &format!("/api/trust/{ATTESTATION}/revoke"),
        "/api/sync/push",
        "/api/verify",
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
        let stub = Arc::new(StubWalletService::new());
        let absent_origin = Request::builder()
            .method("POST")
            .uri(route)
            .header(header::HOST, HOST)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("a request");
        let (status, body) = send(wallet(&stub), absent_origin).await;
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
        let (status, body) = send(wallet(&stub), wrong_origin).await;
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
        let (status, body) = send(wallet(&stub), form_post).await;
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
        let (status, body) = send(wallet(&stub), no_content_type).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{route}");
        assert_eq!(
            body["details"]["reason"],
            json!("content_type_missing"),
            "{route}"
        );

        // The same route with the right headers reaches the handler. Whether
        // the empty body then validates is each route's own business.
        let allowed = request("POST", route, &json!({}));
        let (status, _) = send(wallet(&stub), allowed).await;
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
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = send(
        wallet(&stub),
        request("GET", "/api/identities/alice", &Value::Null),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);

    let (expected_status, expected) =
        fixture_error("witness-get-ledger.json", "malformed_ledger_id");
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = send(
        witness(&stub),
        request("GET", "/api/ledgers/sfttwjzd", &Value::Null),
    )
    .await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);
}

#[tokio::test]
async fn a_malformed_fork_filter_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("witness-get-forks.json", "malformed_ledger_id");
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = send(
        witness(&stub),
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
    let stub = Arc::new(StubWalletService::new());
    let uri = format!("/api/identities/{ALICE}/ledger?since=-1");
    let (status, body) = send(wallet(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);

    let (expected_status, expected) = fixture_error(
        "witness-get-ledger-events.json",
        "malformed_query_parameter",
    );
    assert_eq!(expected["details"]["value"], json!("head"));
    let stub = Arc::new(StubWitnessService::new());
    let uri = format!("/api/ledgers/{ALICE}/events?since=head");
    let (status, body) = send(witness(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, expected_status);
    assert_eq!(body, expected);

    let (expected_status, expected) =
        fixture_error("witness-get-ledgers.json", "malformed_query_parameter");
    assert_eq!(expected["details"]["parameter"], json!("limit"));
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = send(
        witness(&stub),
        request("GET", "/api/ledgers?limit=all", &Value::Null),
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
    let cases: [(&str, &str, &str, Value); 9] = [
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
            "wallet-post-verify.json",
            "unknown_enum_value",
            "/api/verify",
            json!({"kind": "identity", "issuer": ALICE, "subject": BOB}),
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
        let stub = Arc::new(StubWalletService::new());
        let (status, answered) = send(wallet(&stub), request("POST", route, &body)).await;
        assert_eq!(status, expected_status, "{fixture} {reason}");
        assert_eq!(answered, expected, "{fixture} {reason}");
        assert!(stub.calls().is_empty(), "{fixture} {reason}");
    }
}

#[tokio::test]
async fn a_duplicate_witness_answers_the_fixture_rejection() {
    let (expected_status, expected) =
        fixture_error("wallet-post-identity-witnesses.json", "duplicate_witness");
    let stub = Arc::new(StubWalletService::new());
    let uri = format!("/api/identities/{ALICE}/witnesses");
    let body = json!({"witnesses": [WITNESS_ONE, WITNESS_ONE]});
    let (status, answered) = send(wallet(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, expected_status);
    assert_eq!(answered, expected);
}

// ------------------------------------------------------- route boundaries ----

#[tokio::test]
async fn there_is_no_orgs_collection() {
    let stub = Arc::new(StubWalletService::new());
    let (status, body) = send(wallet(&stub), request("GET", "/api/orgs", &Value::Null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], json!(2));
    assert_eq!(body["details"]["reason"], json!("unknown_route"));
    assert_eq!(body["details"]["path"], json!("/api/orgs"));

    let stub = Arc::new(StubWalletService::new());
    let (status, _) = send(
        wallet(&stub),
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
        let stub = Arc::new(StubWalletService::new());
        let uri = format!("/api/identities/{ALICE}/{route}");
        let body = Fixture::named(fixture).request();
        let (status, answered) = send(wallet(&stub), request("POST", &uri, &body)).await;
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
    let stub = Arc::new(StubWalletService::new());
    let uri = format!("/api/identities/{ALICE}/memberships/invitations");
    let body = json!({
        "by": ALICE,
        "role": "controller",
        "invitee_descriptor_base64": "not base64!"
    });
    let (status, answered) = send(wallet(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, expected_status);
    assert_eq!(answered, expected);
    assert!(stub.calls().is_empty(), "the service must not be reached");
}

#[tokio::test]
async fn a_membership_artifact_over_its_cap_is_refused_before_it_is_decoded() {
    let stub = Arc::new(StubWalletService::new());
    let uri = format!("/api/identities/{ALICE}/memberships/admissions");
    // An `AcceptanceFile` is capped at 4 KiB (proposal 001 section 3.8).
    let oversize = "A".repeat(4 * 4096 / 3 + 8);
    let body = json!({"by": ALICE, "acceptance_base64": oversize});
    let (status, answered) = send(wallet(&stub), request("POST", &uri, &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(answered["code"], json!(10));
    assert_eq!(answered["details"]["reason"], json!("message_too_large"));
    assert_eq!(answered["details"]["artifact"], json!("AcceptanceFile"));
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn the_witness_api_refuses_a_mutation_with_405() {
    let stub = Arc::new(StubWitnessService::new());
    let (status, body) = send(witness(&stub), request("POST", "/api/ledgers", &json!({}))).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["code"], json!(2));
    assert_eq!(body["details"]["reason"], json!("method_not_allowed"));
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_since_at_the_head_sequence_reaches_the_service_unchanged() {
    let stub = Arc::new(StubWalletService::new());
    let head_seq = stub.identity_ledger.head_seq;
    let uri = format!("/api/identities/{ALICE}/ledger?since={head_seq}");
    let (status, _) = send(wallet(&stub), request("GET", &uri, &Value::Null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        WalletCall::IdentityLedger(
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
    let stub = Arc::new(StubWitnessService::new());
    let (status, _) = send(
        witness(&stub),
        request("GET", "/api/ledgers?limit=100000", &Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        stub.call(),
        WitnessCall::Ledgers(PageRequest {
            offset: 0,
            limit: 256
        })
    );
}

#[tokio::test]
async fn the_ui_bundle_serves_outside_api_and_never_inside_it() {
    let directory = tempfile::tempdir().expect("a temp dir");
    std::fs::write(directory.path().join("index.html"), "<!doctype html>").expect("write");
    let options = ApiOptions::default().with_ui(UiSource::Directory(directory.path().into()));
    let service: Arc<dyn super::WalletService> = Arc::new(StubWalletService::new());

    for path in ["/", "/wallet", "/witness"] {
        let router = wallet_router(Arc::clone(&service), &options);
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

    let router = wallet_router(Arc::clone(&service), &options);
    let (status, body) = send(router, request("GET", "/api/nope", &Value::Null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["details"]["reason"], json!("unknown_route"));
}

#[tokio::test]
async fn the_loopback_rules_guard_the_ui_and_the_unknown_routes_too() {
    let stub = Arc::new(StubWalletService::new());
    for path in ["/", "/api/nope"] {
        let request = Request::builder()
            .uri(path)
            .header(header::HOST, "evil.example")
            .body(Body::empty())
            .expect("a request");
        let (status, body) = send(wallet(&stub), request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            body["details"]["reason"],
            json!("host_not_loopback"),
            "{path}"
        );
    }
}

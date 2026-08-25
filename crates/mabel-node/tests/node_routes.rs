//! The one node HTTP surface over real storage, on a home that witnesses
//! (proposal 006 section 8, tickets 010, 012 and 037).
//!
//! These are the tests the witness-only surface used to hold. There is one
//! router and one store now, so a witness's holdings are read from `GET
//! /api/identities/known` and one stored ledger from `GET
//! /api/identities/{id}/ledger`, exactly as a wallet's are; `GET /api/forks`
//! and `GET /api/node` keep their own documents and are checked against the
//! frozen fixtures key for key.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use common::{
    Chain, Home, from_endpoint, home, home_witnessing_for_nobody, rendered, secret, subject,
    witness_identity,
};
use mabel_core::LedgerId;
use mabel_node::api::documents::Id;
use mabel_node::api::service::{EventPageRequest, ForkQuery, NodeService, PageRequest};
use mabel_node::api::stub::Fixture;
use mabel_node::api::{ApiOptions, DEFAULT_HTTP_BIND, UiSource, node_router};
use mabel_node::wallet::{NodeApiService, WalletCore, WalletSync};
use mabel_node::{DEFAULT_STORAGE_CAPACITY, LedgerStorage, RelayMode, StorageCaps};
use serde::Serialize;
use serde_json::{Value, json};
use tower::ServiceExt;

/// The `Host` the loopback rules demand for the default bind.
const HOST: &str = "127.0.0.1:9080";

/// A node witnessing for one identity and holding two strangers' ledgers, one
/// of them forked at seq 3, which is the shape the fixtures describe.
struct Fixed {
    home: Home,
    service: Arc<NodeApiService>,
    alice: Chain,
    bob: Chain,
    /// The endpoint that offered the conflicting event.
    conflicting_source: iroh_base::EndpointId,
    /// The conflicting event's bytes.
    conflicting: Vec<u8>,
}

impl Fixed {
    async fn new() -> Self {
        let home = home();
        let storage = home.storage(StorageCaps::default());

        let mut alice = Chain::new(31);
        alice.add_witness_set(&[witness_identity(), subject(50)]);
        let attestation = alice.add_attestation(9);
        // Two valid events for seq 3; the revocation is the one stored first.
        let conflicting = alice.attestation(11).signed_event;
        alice.add_revocation(attestation);
        storage
            .push(alice.ledger, &alice.all(), from_endpoint(4))
            .expect("the chain names this witness");
        storage
            .push(
                alice.ledger,
                std::slice::from_ref(&conflicting),
                from_endpoint(6),
            )
            .expect_err("seq 3 already holds another valid event");

        let mut bob = Chain::new(32);
        bob.add_witness();
        storage
            .push(bob.ledger, &bob.all(), from_endpoint(4))
            .expect("the chain names this witness");

        let service = Arc::new(service(&home, Arc::clone(&storage)).await);
        Self {
            home,
            service,
            alice,
            bob,
            conflicting_source: secret(6).public(),
            conflicting,
        }
    }

    /// The three ledgers this home holds, by ascending id, as the documents
    /// spell them: the two pushed chains and the witness identity's own, which
    /// the home stores so it may take a ledger at all (proposal 006 section
    /// 4.1).
    fn sorted(&self) -> Vec<LedgerId> {
        let mut ledgers = vec![self.alice.ledger, self.bob.ledger, witness_identity()];
        ledgers.sort_unstable();
        ledgers
    }

    fn id(&self, ledger: LedgerId) -> Id {
        Id::parse(&ledger.to_string()).expect("a ledger id renders as an id")
    }

    fn router(&self) -> axum::Router {
        node_router(
            self.service.clone() as Arc<dyn NodeService>,
            &ApiOptions::default().with_ui(UiSource::Disabled),
        )
    }
}

/// The one service over `home` and its store, on an endpoint that is bound and
/// never dialled.
async fn service(home: &Home, storage: Arc<LedgerStorage>) -> NodeApiService {
    let secret = home.home.node_key().expect("the node key reads");
    let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, &[])
        .await
        .expect("the endpoint binds");
    let core = Arc::new(WalletCore::new(home.home.clone()).with_index(Arc::clone(&storage)));
    NodeApiService::new(
        core,
        storage,
        // Nothing here is expected to answer, so no test waits ten seconds for
        // a dial that cannot land.
        WalletSync::new(endpoint).with_timeout(Duration::from_secs(3)),
        DEFAULT_HTTP_BIND,
        RelayMode::Disabled,
    )
}

/// A ledger this home does not hold.
fn unheld() -> Id {
    Id::parse(&mabel_core::IdentityId::from_bytes([0x5a; 32]).to_string()).expect("an id")
}

fn document(value: &impl Serialize) -> Value {
    serde_json::to_value(value).expect("a document serializes")
}

/// The fixture's example 200 body, without the envelope's `ok`.
fn fixture_response(name: &str) -> Value {
    let mut response = Fixture::named(name).response();
    response
        .as_object_mut()
        .expect("an object")
        .remove("ok")
        .expect("every fixture response carries ok");
    response
}

/// Asserts `actual` carries the same keys, in the same nesting, as the frozen
/// `expected`.
///
/// `payload` is skipped: `contracts/README.md` lists the inception and
/// membership payload names as not frozen, so its fields are compared nowhere.
fn same_shape(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            let mut found: Vec<&String> = actual.keys().collect();
            let mut wanted: Vec<&String> = expected.keys().collect();
            found.sort();
            wanted.sort();
            assert_eq!(found, wanted, "the keys of {path} differ");
            for (key, expected) in expected {
                if key == "payload" {
                    continue;
                }
                same_shape(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            if let (Some(actual), Some(expected)) = (actual.first(), expected.first()) {
                same_shape(actual, expected, &format!("{path}[0]"));
            }
        }
        // A field that does not apply is null in either document; every other
        // pair must be the same JSON type.
        (Value::Null, _) | (_, Value::Null) => {}
        (actual, expected) => assert_eq!(
            std::mem::discriminant(actual),
            std::mem::discriminant(expected),
            "the type of {path} differs: {actual} against {expected}"
        ),
    }
}

/// One document for every node, with no role in it: what this node can do is
/// `identity_count` and `witness_for` (proposal 006 section 8).
#[tokio::test]
async fn the_node_document_matches_the_fixture_and_names_no_role() {
    let fixed = Fixed::new().await;
    let node = fixed.service.node().await.expect("the node answers");
    let actual = document(&node);
    same_shape(&actual, &fixture_response("node-get-node.json"), "node");

    assert!(
        actual.get("role").is_none(),
        "no document names a role: {actual}"
    );
    assert_eq!(actual["relay"], json!("disabled"));
    assert_eq!(
        actual["endpoint_id"],
        json!(rendered(&fixed.home.endpoint_id()))
    );
    assert_eq!(
        actual["identity_count"],
        json!(0),
        "this home holds no key, and still answers every route"
    );
    // The two pushed chains and the witness identity's own.
    assert_eq!(actual["ledger_count"], json!(3));
    assert_eq!(actual["fork_count"], json!(1));
    assert_eq!(actual["storage_capacity"], json!(DEFAULT_STORAGE_CAPACITY));
    let stored: u64 = fixed
        .alice
        .all()
        .iter()
        .chain(fixed.bob.all().iter())
        .map(|event| event.len() as u64)
        .sum();
    assert_eq!(
        actual["storage_used"],
        json!(stored + fixed.home.stored_bytes())
    );
    assert_eq!(actual["http_bind"], json!(HOST));

    // The `witness_for` entry this home takes pushes under, with the
    // advertisement invariant beside it (proposal 006 section 4.1).
    assert_eq!(
        actual["witness_for"],
        json!([{
            "identity": witness_identity().to_string(),
            "advertised": true,
            "reason": null
        }])
    );
}

/// Proposal 006 section 4.1: a home whose `witness_for` entry does not
/// advertise it still starts and still serves, and `GET /api/node` names the
/// entry with the reason it admits no new ledger.
#[tokio::test]
async fn the_node_document_names_a_witness_for_entry_that_does_not_advertise_this_home() {
    let home = Home::witnessing_for(DEFAULT_STORAGE_CAPACITY, vec![witness_identity()]);
    let storage = home.storage(StorageCaps::default());
    let service = service(&home, storage).await;
    let actual = document(&service.node().await.expect("the node answers"));

    assert_eq!(
        actual["witness_for"],
        json!([{
            "identity": witness_identity().to_string(),
            "advertised": false,
            "reason": "this home holds no copy of that identity's ledger"
        }])
    );
    assert_eq!(
        actual["ledger_count"],
        json!(0),
        "and it serves what it has"
    );
}

/// A witness's holdings are the ledgers it stores and cannot sign for, which is
/// what `GET /api/identities/known` answers: the route `GET /api/ledgers` used
/// to (proposal 006 section 8).
#[tokio::test]
async fn the_known_identities_are_this_homes_holdings_by_ascending_id() {
    let fixed = Fixed::new().await;
    let page = fixed
        .service
        .known_identities(PageRequest {
            offset: 0,
            limit: 100,
        })
        .await
        .expect("the list answers");
    let actual = document(&page);
    same_shape(
        &actual,
        &fixture_response("wallet-get-known-identities.json"),
        "known",
    );
    assert_eq!(actual["offset"], json!(0));
    assert_eq!(actual["limit"], json!(100));
    assert_eq!(actual["more"], json!(false));

    let sorted = fixed.sorted();
    let ids: Vec<String> = page
        .identities
        .iter()
        .map(|row| row.identity_id.to_string())
        .collect();
    assert_eq!(
        ids,
        sorted.iter().map(LedgerId::to_string).collect::<Vec<_>>(),
        "every stored ledger is known and none of them is signed for here"
    );
    assert!(page.identities.iter().all(|row| row.stored));
}

/// Paging on `GET /api/identities/known`: the default limit, the maximum, and a
/// value over it, which is clamped rather than refused (proposal 006 section 8).
#[tokio::test]
async fn the_known_route_pages_at_the_default_the_maximum_and_over_it() {
    let fixed = Fixed::new().await;
    let sorted = fixed.sorted();

    for (offset, ledger) in sorted.iter().enumerate() {
        let page = fixed
            .service
            .known_identities(PageRequest {
                offset: offset as u32,
                limit: 1,
            })
            .await
            .expect("the list answers");
        assert_eq!(page.identities.len(), 1);
        assert_eq!(page.identities[0].identity_id, fixed.id(*ledger));
        assert_eq!(page.more, offset < sorted.len() - 1, "offset {offset}");
    }

    let router = fixed.router();
    for (query, limit) in [
        ("", 100),
        ("?limit=256", 256),
        ("?limit=100000", 256),
        ("?offset=1&limit=1", 1),
    ] {
        let (status, body) = send(router.clone(), &format!("/api/identities/known{query}")).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["limit"], json!(limit), "{query}");
    }

    let (status, body) = send(router, "/api/identities/known?limit=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["details"]["reason"],
        json!("malformed_query_parameter")
    );
}

/// One stored ledger's events come from the identity route, which answers for
/// any ledger this home holds (proposal 006 section 8).
#[tokio::test]
async fn one_stored_ledger_reads_through_the_identity_route() {
    let fixed = Fixed::new().await;
    let alice = fixed.id(fixed.alice.ledger);
    let page = fixed
        .service
        .identity_ledger(
            alice.clone(),
            EventPageRequest {
                since: 0,
                limit: 512,
            },
        )
        .await
        .expect("the ledger is held");
    same_shape(
        &document(&page),
        &fixture_response("wallet-get-identity-ledger.json"),
        "ledger",
    );
    assert_eq!(page.ledger_id, alice);
    assert_eq!(page.head_seq, 3);
    assert_eq!(page.event_count, 4);
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.payload_kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "inception",
            "witness_set",
            "trust_attestation",
            "trust_revocation"
        ]
    );
    // A seq-0 event names no ledger and no predecessor.
    assert_eq!(page.events[0].ledger_id, None);
    assert_eq!(page.events[0].prev, None);
    assert_eq!(page.events[0].event_id, alice);

    // `?since=` is inclusive, and a short limit sets `more`.
    let page = fixed
        .service
        .identity_ledger(alice, EventPageRequest { since: 2, limit: 1 })
        .await
        .expect("the ledger is held");
    assert_eq!(page.since, 2);
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].seq, 2);
    assert!(page.more);
}

/// `unknown_ledger` is the one spelling for a ledger this home does not hold:
/// `ledger_not_held` died with the witness routes (proposal 006 section 8).
#[tokio::test]
async fn a_ledger_this_home_does_not_hold_answers_unknown_ledger() {
    let fixed = Fixed::new().await;
    let error = fixed
        .service
        .identity(unheld())
        .await
        .expect_err("the ledger is not held");
    assert_eq!(error.status(), StatusCode::NOT_FOUND);
    assert_eq!(error.reason(), "unknown_ledger");
}

#[tokio::test]
async fn the_fork_list_matches_the_fixture_and_filters_by_ledger() {
    let fixed = Fixed::new().await;
    let alice = fixed.id(fixed.alice.ledger);
    let page = fixed
        .service
        .forks(ForkQuery {
            ledger_id: None,
            page: PageRequest {
                offset: 0,
                limit: 64,
            },
        })
        .await
        .expect("the forks answer");
    let actual = document(&page);
    same_shape(&actual, &fixture_response("node-get-forks.json"), "forks");

    assert_eq!(page.entries.len(), 1);
    let record = &page.entries[0];
    assert_eq!(record.ledger_id, alice);
    assert_eq!(record.seq, 3);
    assert_eq!(
        record.source_endpoint.as_str(),
        rendered(&fixed.conflicting_source)
    );
    assert_eq!(record.kept.seq, 3);
    assert_eq!(record.kept.payload_kind, "trust_revocation");
    assert_eq!(record.conflicting.seq, 3);
    assert_eq!(record.conflicting.payload_kind, "trust_attestation");
    assert_eq!(
        record.conflicting.event_id.as_str(),
        mabel_net::wire::signed_event_id(&fixed.conflicting)
            .expect("the event id reads")
            .to_string()
    );
    assert_eq!(record.statement, mabel_node::fork_statement(&alice, 3));
    assert!(record.statement.contains("evidence of equivocation"));

    // One ledger at a time, and a ledger with no forks answers none.
    let filtered = fixed
        .service
        .forks(ForkQuery {
            ledger_id: Some(fixed.id(fixed.bob.ledger)),
            page: PageRequest {
                offset: 0,
                limit: 64,
            },
        })
        .await
        .expect("the forks answer");
    assert!(filtered.entries.is_empty());
    assert!(!filtered.more);
}

/// `GET /api/forks` is a node route: a home that witnesses for nobody answers
/// it, empty, rather than 404 (proposal 006 section 8).
#[tokio::test]
async fn the_fork_route_answers_on_a_home_that_witnesses_for_nobody() {
    let home = home_witnessing_for_nobody();
    let storage = home.storage(StorageCaps::default());
    let service = Arc::new(service(&home, storage).await);
    let router = node_router(
        service as Arc<dyn NodeService>,
        &ApiOptions::default().with_ui(UiSource::Disabled),
    );

    let (status, body) = send(router.clone(), "/api/forks").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["entries"], json!([]));
    assert_eq!(body["more"], json!(false));

    // And so does the wallet home page, on a home holding no key at all.
    let (status, body) = send(router, "/api/identities").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["identities"],
        json!([]),
        "emptiness is not a refusal (proposal 006 section 8)"
    );
}

/// An endpoint id where an identity id belongs is refused before any dial: both
/// render as 52 base32 characters (proposal 006 section 8).
#[tokio::test]
async fn an_endpoint_id_sent_to_holdings_answers_404() {
    let fixed = Fixed::new().await;
    let machine = rendered(&fixed.home.endpoint_id());
    let (status, body) = send(
        fixed.router(),
        &format!("/api/witnesses/{machine}/holdings"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], json!(2));
    assert_eq!(body["details"]["reason"], json!("endpoint_not_identity"));
    assert_eq!(body["details"]["value"], json!(machine));
}

/// A witness identity no machine answers for is `unresolvable_witness`, with
/// what was dialled in `details` (proposal 006 section 8).
#[tokio::test]
async fn a_witness_no_machine_answers_for_is_unresolvable() {
    let fixed = Fixed::new().await;
    let stranger = unheld();
    let (status, body) = send(
        fixed.router(),
        &format!("/api/witnesses/{stranger}/holdings"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["details"]["reason"], json!("unresolvable_witness"));
    assert_eq!(body["details"]["witness"], json!(stranger.as_str()));
    assert_eq!(body["details"]["endpoints_tried"], json!([]));
}

#[tokio::test]
async fn the_one_router_answers_from_this_service() {
    let fixed = Fixed::new().await;
    let router = fixed.router();

    let (status, body) = send(router.clone(), "/api/node").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], json!(true));
    assert!(body.get("role").is_none(), "{body}");
    assert_eq!(body["ledger_count"], json!(3));

    let (status, body) = send(
        router.clone(),
        &format!(
            "/api/identities/{}/ledger?since=1",
            fixed.id(fixed.bob.ledger)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["events"][0]["seq"], json!(1));

    let (status, body) = send(router.clone(), "/api/forks?limit=1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["entries"][0]["seq"], json!(3));

    let (status, body) = send(router, &format!("/api/identities/{}", unheld())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["ok"], json!(false));
    assert_eq!(body["details"]["reason"], json!("unknown_ledger"));
}

/// A mutating route exists on a home that holds no key, and refuses with
/// `no_local_signer` rather than 404 (proposal 006 section 8).
#[tokio::test]
async fn a_mutating_route_on_a_ledger_this_home_cannot_append_to_answers_no_local_signer() {
    let fixed = Fixed::new().await;
    let alice = fixed.id(fixed.alice.ledger);
    let body = json!({"witnesses": []}).to_string();
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/identities/{alice}/witnesses"))
        .header(header::HOST, HOST)
        .header(header::ORIGIN, format!("http://{HOST}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("a request");
    let response = fixed
        .router()
        .oneshot(request)
        .await
        .expect("the router is infallible");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    let answered: Value = serde_json::from_slice(&bytes).expect("a JSON body");
    assert_eq!(status, StatusCode::FORBIDDEN, "{answered}");
    assert_eq!(answered["code"], json!(2));
    assert_eq!(answered["details"]["reason"], json!("no_local_signer"));
    // The message names the ledger the way a person reads an identity, and the
    // detail beside it carries the bare id a caller matches on (decision 019).
    assert_eq!(
        answered["message"],
        json!(format!(
            "this home holds no key that may append to {}{alice}",
            mabel_core::LINK_PREFIX
        ))
    );
}

async fn send(router: axum::Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("a request");
    let response = router
        .oneshot(request)
        .await
        .expect("the router is infallible");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "a response body that is not JSON: {error}: {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, body)
}

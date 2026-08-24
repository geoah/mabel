//! The witness HTTP surface over real storage (ticket 010, ticket 012's
//! service trait).
//!
//! Every document is checked against the frozen fixture under
//! `contracts/http/` key for key, and the last test drives the real router so
//! the routes of ticket 012 answer from this service rather than the stub.

mod common;

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use common::{Chain, Home, from_endpoint, home, rendered, secret, subject, witness_identity};
use mabel_core::LedgerId;
use mabel_node::api::documents::Id;
use mabel_node::api::service::{EventPageRequest, ForkQuery, PageRequest, WitnessService};
use mabel_node::api::stub::Fixture;
use mabel_node::api::{ApiOptions, DEFAULT_HTTP_BIND, UiSource, witness_router};
use mabel_node::witness::{WitnessCaps, WitnessReadService};
use mabel_node::{DEFAULT_STORAGE_CAPACITY, RelayMode};
use serde::Serialize;
use serde_json::{Value, json};
use tower::ServiceExt;

/// The `Host` the loopback rules demand for the default bind.
const HOST: &str = "127.0.0.1:9080";

/// A witness holding two ledgers, one of them forked at seq 3, which is the
/// shape the fixtures describe.
struct Fixed {
    home: Home,
    service: Arc<WitnessReadService>,
    alice: Chain,
    bob: Chain,
    /// The endpoint that offered the conflicting event.
    conflicting_source: iroh_base::EndpointId,
    /// The conflicting event's bytes.
    conflicting: Vec<u8>,
}

impl Fixed {
    fn new() -> Self {
        let home = home();
        let storage = home.storage(WitnessCaps::default());

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

        let service = Arc::new(WitnessReadService::new(
            storage,
            DEFAULT_HTTP_BIND,
            RelayMode::Disabled,
        ));
        Self {
            home,
            service,
            alice,
            bob,
            conflicting_source: secret(6).public(),
            conflicting,
        }
    }

    /// The ledgers this witness holds, by ascending id, as the documents
    /// spell them.
    fn sorted(&self) -> Vec<LedgerId> {
        let mut ledgers = vec![self.alice.ledger, self.bob.ledger];
        ledgers.sort_unstable();
        ledgers
    }

    fn id(&self, ledger: LedgerId) -> Id {
        Id::parse(&ledger.to_string()).expect("a ledger id renders as an id")
    }
}

/// A ledger this witness does not hold.
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

#[tokio::test]
async fn the_node_document_matches_the_fixture() {
    let fixed = Fixed::new();
    let node = fixed.service.node().await.expect("the node answers");
    let actual = document(&node);
    same_shape(&actual, &fixture_response("witness-get-node.json"), "node");

    assert_eq!(actual["role"], json!("witness"));
    assert_eq!(actual["relay"], json!("disabled"));
    assert_eq!(actual["witnesses"], json!([]), "a witness pushes to nobody");
    assert_eq!(
        actual["endpoint_id"],
        json!(rendered(&fixed.home.endpoint_id()))
    );
    assert_eq!(actual["ledger_count"], json!(2));
    assert_eq!(actual["fork_count"], json!(1));
    assert_eq!(actual["storage_capacity"], json!(DEFAULT_STORAGE_CAPACITY));
    let stored: u64 = fixed
        .alice
        .all()
        .iter()
        .chain(fixed.bob.all().iter())
        .map(|event| event.len() as u64)
        .sum();
    assert_eq!(actual["storage_used"], json!(stored));
    assert_eq!(actual["http_bind"], json!(HOST));
}

#[tokio::test]
async fn the_ledger_list_matches_the_fixture_and_pages_by_ascending_id() {
    let fixed = Fixed::new();
    let page = fixed
        .service
        .ledgers(PageRequest {
            offset: 0,
            limit: 256,
        })
        .await
        .expect("the list answers");
    let actual = document(&page);
    same_shape(
        &actual,
        &fixture_response("witness-get-ledgers.json"),
        "ledgers",
    );
    assert_eq!(actual["offset"], json!(0));
    assert_eq!(actual["limit"], json!(256));
    assert_eq!(actual["more"], json!(false));

    let sorted = fixed.sorted();
    let ids: Vec<String> = page
        .entries
        .iter()
        .map(|entry| entry.ledger_id.to_string())
        .collect();
    assert_eq!(
        ids,
        sorted.iter().map(LedgerId::to_string).collect::<Vec<_>>()
    );

    // One row per page, in the same order.
    for (offset, ledger) in sorted.iter().enumerate() {
        let page = fixed
            .service
            .ledgers(PageRequest {
                offset: offset as u32,
                limit: 1,
            })
            .await
            .expect("the list answers");
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].ledger_id, fixed.id(*ledger));
        assert_eq!(page.more, offset == 0);
    }
}

#[tokio::test]
async fn the_ledger_document_carries_the_witness_set_and_the_row() {
    let fixed = Fixed::new();
    let alice = fixed.id(fixed.alice.ledger);
    let view = fixed
        .service
        .ledger(alice.clone())
        .await
        .expect("the ledger is held");
    let actual = document(&view);
    same_shape(
        &actual,
        &fixture_response("witness-get-ledger.json"),
        "ledger",
    );

    assert_eq!(view.entry.ledger_id, alice);
    assert_eq!(view.entry.declared_kind.as_str(), "person");
    assert_eq!(view.entry.head_seq, 3);
    assert_eq!(view.entry.event_count, 4);
    assert_eq!(view.entry.fork_count, 1);
    assert!(!view.entry.forks_truncated);
    assert_eq!(
        view.entry.source_endpoint.as_str(),
        rendered(&secret(4).public())
    );
    assert_eq!(
        view.witnesses.iter().map(Id::to_string).collect::<Vec<_>>(),
        vec![witness_identity().to_string(), subject(50).to_string()],
        "the set is the order the latest WitnessSet listed"
    );
}

#[tokio::test]
async fn a_ledger_this_witness_does_not_hold_answers_the_fixture_404() {
    let fixed = Fixed::new();
    let missing = unheld();
    let error = fixed
        .service
        .ledger(missing.clone())
        .await
        .expect_err("the ledger is not held");
    assert_eq!(error.status(), StatusCode::NOT_FOUND);
    assert_eq!(error.code(), 2);
    assert_eq!(error.reason(), "ledger_not_held");
    assert_eq!(
        error.to_document()["message"],
        json!(format!("this witness does not hold {missing}"))
    );
    assert_eq!(
        error.to_document()["details"]["ledger_id"],
        json!(missing.as_str())
    );

    let error = fixed
        .service
        .ledger_events(
            missing,
            EventPageRequest {
                since: 0,
                limit: 512,
            },
        )
        .await
        .expect_err("the ledger is not held");
    assert_eq!(error.reason(), "ledger_not_held");
}

#[tokio::test]
async fn the_event_page_matches_the_fixture_and_since_is_inclusive() {
    let fixed = Fixed::new();
    let alice = fixed.id(fixed.alice.ledger);
    let page = fixed
        .service
        .ledger_events(
            alice.clone(),
            EventPageRequest {
                since: 0,
                limit: 512,
            },
        )
        .await
        .expect("the ledger is held");
    let actual = document(&page);
    same_shape(
        &actual,
        &fixture_response("witness-get-ledger-events.json"),
        "events",
    );

    assert_eq!(page.ledger_id, alice);
    assert_eq!(page.since, 0);
    assert_eq!(page.limit, 512);
    assert_eq!(page.head_seq, 3);
    assert_eq!(page.event_count, 4);
    assert!(!page.more);
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
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
    assert_eq!(page.events[1].ledger_id, Some(alice.clone()));
    assert_eq!(page.events[1].prev, Some(alice.clone()));
    assert_eq!(page.events[3].event_id, page.head_event);

    // `?since=` is inclusive, and a short limit sets `more`.
    let page = fixed
        .service
        .ledger_events(alice, EventPageRequest { since: 2, limit: 1 })
        .await
        .expect("the ledger is held");
    assert_eq!(page.since, 2);
    assert_eq!(page.limit, 1);
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].seq, 2);
    assert!(page.more);
}

#[tokio::test]
async fn the_fork_list_matches_the_fixture_and_filters_by_ledger() {
    let fixed = Fixed::new();
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
    same_shape(
        &actual,
        &fixture_response("witness-get-forks.json"),
        "forks",
    );

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
    assert_eq!(
        record.statement,
        mabel_node::witness::fork_statement(&alice, 3)
    );
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

#[tokio::test]
async fn the_witness_router_answers_from_this_service() {
    let fixed = Fixed::new();
    let router = witness_router(
        fixed.service.clone() as Arc<dyn WitnessService>,
        &ApiOptions::default().with_ui(UiSource::Disabled),
    );

    let (status, body) = send(router.clone(), "/api/node").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["role"], json!("witness"));
    assert_eq!(body["ledger_count"], json!(2));

    let (status, body) = send(
        router.clone(),
        &format!("/api/ledgers/{}/events?since=1", fixed.id(fixed.bob.ledger)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["events"][0]["seq"], json!(1));

    let (status, body) = send(router, &format!("/api/ledgers/{}", unheld())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["ok"], json!(false));
    assert_eq!(body["code"], json!(2));
    assert_eq!(body["details"]["reason"], json!("ledger_not_held"));
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

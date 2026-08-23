//! Two in-process endpoints speaking `mabel/ledger/0` (proposal 001
//! section 11, research 002 section 8 level 1).
//!
//! Both endpoints bind with `presets::Minimal` and `RelayMode::Disabled` and
//! dial the loopback `EndpointAddr`, so no test here touches DNS, a relay or
//! the public internet. Every test carries an explicit timeout (decision
//! 013).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use iroh::protocol::Router;
use iroh::{Endpoint, EndpointAddr, TransportAddr};
use iroh_tickets::endpoint::EndpointTicket;
use mabel_core::proto::RejectCode;
use mabel_core::{IdentityId, LedgerId};
use mabel_net::store::{ForkRecord, Provenance};
use mabel_net::testing::{Call, MemoryStore, sample_chain, sample_events};
use mabel_net::wire::{self, Response};
use mabel_net::{
    ALPN, Client, EndpointConfig, Error, LedgerProtocol, MAX_EVENT_BYTES, MAX_FRAME_BYTES,
    MAX_PUSH_BYTES, RelayChoice, ServerConfig, bind_endpoint,
};

/// Decision 013: a networked test never waits longer than this.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Runs a test body under [`TIMEOUT`].
macro_rules! bounded {
    ($body:block) => {
        tokio::time::timeout(TIMEOUT, async move $body)
            .await
            .expect("the test timed out")
    };
}

struct Peers {
    _router: Router,
    client: Client,
    client_endpoint: Endpoint,
    server_addr: EndpointAddr,
    store: Arc<MemoryStore>,
}

impl Peers {
    /// A second connection to the same server.
    async fn dial(&self) -> Result<Client, Error> {
        Client::connect(&self.client_endpoint, self.server_addr.clone()).await
    }
}

/// The endpoint's loopback address, built without any address lookup.
fn loopback_addr(endpoint: &Endpoint) -> EndpointAddr {
    let addrs = endpoint
        .bound_sockets()
        .into_iter()
        .filter(|socket| socket.is_ipv4())
        .map(|socket| {
            let ip = if socket.ip().is_unspecified() {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            } else {
                socket.ip()
            };
            TransportAddr::Ip(SocketAddr::new(ip, socket.port()))
        })
        .collect();
    EndpointAddr {
        id: endpoint.id(),
        addrs,
    }
}

async fn offline_endpoint() -> Endpoint {
    bind_endpoint(EndpointConfig::new(RelayChoice::Disabled))
        .await
        .expect("the endpoint binds")
        .endpoint
}

async fn setup(config: ServerConfig) -> Peers {
    let store = Arc::new(MemoryStore::new());
    let server = offline_endpoint().await;
    let server_addr = loopback_addr(&server);
    let router = Router::builder(server)
        .accept(ALPN, LedgerProtocol::with_config(store.clone(), config))
        .spawn();
    let client_endpoint = offline_endpoint().await;
    let client = Client::connect(&client_endpoint, server_addr.clone())
        .await
        .expect("the client connects");
    Peers {
        _router: router,
        client,
        client_endpoint,
        server_addr,
        store,
    }
}

#[tokio::test]
async fn a_seeded_ticket_lets_a_client_dial_by_endpoint_id() {
    bounded!({
        let store = Arc::new(MemoryStore::new());
        let (ledger, events) = sample_chain(12, 2);
        store.insert(ledger, events);

        let server = offline_endpoint().await;
        let server_id = server.id();
        let ticket = EndpointTicket::new(loopback_addr(&server)).to_string();
        let _router = Router::builder(server)
            .accept(ALPN, LedgerProtocol::new(store))
            .spawn();

        // The compose topology's shape: no relays, no DNS, one `--peer`
        // ticket seeded into the lookup, then dialling by id alone.
        let addr = mabel_net::parse_peer_ticket(&ticket).expect("the ticket parses");
        let client_endpoint =
            bind_endpoint(EndpointConfig::new(RelayChoice::Disabled).with_peer(addr))
                .await
                .expect("the endpoint binds")
                .endpoint;
        let client = Client::connect(&client_endpoint, server_id)
            .await
            .expect("the lookup resolves the endpoint id");
        assert_eq!(client.remote_id(), server_id);
        assert_eq!(client.head(ledger).await.unwrap().unwrap().head_seq, 1);
    });
}

async fn with_events(count: usize) -> (Peers, LedgerId, Vec<Vec<u8>>) {
    let peers = setup(ServerConfig::default()).await;
    let (ledger, events) = sample_chain(1, count);
    peers.store.insert(ledger, events.clone());
    (peers, ledger, events)
}

fn unknown_ledger() -> LedgerId {
    IdentityId::from_bytes([42u8; 32])
}

/// Sends a frame the encoders would refuse to build and reads the rejection.
async fn reject_code_of(client: &Client, frame: &[u8]) -> RejectCode {
    let answer = client.send_frame(frame).await.expect("the peer answers");
    match wire::parse_response(&answer).expect("the answer is a Response") {
        Response::Rejected(rejection) => rejection.code,
        other => panic!("expected a rejection, got {}", other.name()),
    }
}

#[tokio::test]
async fn a_head_request_returns_the_stored_head() {
    bounded!({
        let (peers, ledger, events) = with_events(3).await;
        let head = peers
            .client
            .head(ledger)
            .await
            .expect("the request succeeds")
            .expect("the ledger is stored");
        assert_eq!(head.head_seq, 2);
        assert_eq!(Some(head.head_event), wire::signed_event_id(&events[2]));
    });
}

#[tokio::test]
async fn an_unknown_ledger_answers_not_found() {
    bounded!({
        let (peers, _, _) = with_events(1).await;
        assert!(peers.client.head(unknown_ledger()).await.unwrap().is_none());
        assert!(
            peers
                .client
                .get(unknown_ledger(), 0, 0)
                .await
                .unwrap()
                .is_none()
        );
    });
}

#[tokio::test]
async fn a_get_returns_events_from_since_inclusive() {
    bounded!({
        let (peers, ledger, events) = with_events(4).await;
        let page = peers
            .client
            .get(ledger, 1, 2)
            .await
            .unwrap()
            .expect("the ledger is stored");
        assert_eq!(page.events, events[1..3].to_vec());
        assert_eq!(page.head_seq, 3);
        assert!(page.more, "one event is left past this page");
    });
}

#[tokio::test]
async fn a_get_at_the_head_seq_returns_that_event() {
    bounded!({
        let (peers, ledger, events) = with_events(3).await;
        let page = peers.client.get(ledger, 2, 0).await.unwrap().unwrap();
        assert_eq!(page.events, vec![events[2].clone()], "since is inclusive");
        assert!(!page.more);
    });
}

#[tokio::test]
async fn a_get_past_the_head_returns_nothing() {
    bounded!({
        let (peers, ledger, _) = with_events(3).await;
        let page = peers.client.get(ledger, 9, 0).await.unwrap().unwrap();
        assert!(page.events.is_empty());
        assert_eq!(page.head_seq, 2);
        assert!(!page.more);
    });
}

#[tokio::test]
async fn get_all_pages_until_more_is_false() {
    bounded!({
        let events = sample_events(5);
        let budget = wire::entry_len(1, &events[0]) + wire::entry_len(1, &events[1]);
        let peers = setup(ServerConfig {
            response_budget_bytes: budget,
            ..ServerConfig::default()
        })
        .await;
        let (ledger, events) = sample_chain(1, 5);
        peers.store.insert(ledger, events.clone());

        let all = peers.client.get_all(ledger, 0).await.unwrap().unwrap();
        assert_eq!(all, events);
        let pages = peers
            .store
            .calls()
            .iter()
            .filter(|call| matches!(call, Call::Read { .. }))
            .count();
        assert!(pages > 1, "the budget forced paging, saw {pages} reads");
    });
}

#[tokio::test]
async fn the_byte_budget_sets_more_before_the_count_limit() {
    bounded!({
        let events = sample_events(4);
        let budget = wire::entry_len(1, &events[0]) + wire::entry_len(1, &events[1]);
        let peers = setup(ServerConfig {
            response_budget_bytes: budget,
            ..ServerConfig::default()
        })
        .await;
        let (ledger, events) = sample_chain(1, 4);
        peers.store.insert(ledger, events);

        let page = peers.client.get(ledger, 0, 0).await.unwrap().unwrap();
        assert_eq!(page.events.len(), 2, "the byte budget stopped the fill");
        assert!(page.more);
    });
}

#[tokio::test]
async fn a_push_stores_events_and_reports_the_head() {
    bounded!({
        let peers = setup(ServerConfig::default()).await;
        let (ledger, events) = sample_chain(2, 3);

        let outcome = peers.client.push(ledger, &events).await.unwrap();
        assert_eq!(outcome.head_seq, 2);
        assert_eq!(outcome.stored, 3);
        assert_eq!(
            peers.store.events(ledger),
            events,
            "the bytes are stored verbatim"
        );

        // Pushing the same events again stores nothing new, so a retry is
        // idempotent.
        let again = peers.client.push(ledger, &events).await.unwrap();
        assert_eq!(again.stored, 0);
        assert_eq!(again.head_seq, 2);
    });
}

#[tokio::test]
async fn a_push_reaches_the_store_with_the_peer_id_as_provenance() {
    bounded!({
        let peers = setup(ServerConfig::default()).await;
        let (ledger, events) = sample_chain(3, 2);
        peers.client.push(ledger, &events).await.unwrap();

        let expected = Provenance::from_endpoint(peers.client_endpoint.id());
        assert!(
            peers.store.calls().contains(&Call::Push {
                ledger,
                count: 2,
                provenance: expected,
            }),
            "the store sees the peer id as provenance, {:?}",
            peers.store.calls()
        );
    });
}

#[tokio::test]
async fn a_store_rejection_passes_through_unchanged() {
    bounded!({
        let peers = setup(ServerConfig::default()).await;
        let (ledger, events) = sample_chain(4, 3);
        // Pushing from seq 1 leaves a gap, which the store calls INVALID.
        let error = peers
            .client
            .push(ledger, &events[1..])
            .await
            .expect_err("the store refuses a gap");
        let Error::Rejected(rejection) = error else {
            panic!("expected a rejection, got {error}");
        };
        assert_eq!(rejection.code, RejectCode::Invalid);
        assert_eq!(rejection.at_seq, 1);
    });
}

#[tokio::test]
async fn a_list_orders_by_ascending_ledger_id() {
    bounded!({
        let peers = setup(ServerConfig::default()).await;
        let mut ledgers = Vec::new();
        for seed in [5u8, 6, 7] {
            let (ledger, events) = sample_chain(seed, 2);
            peers.store.insert(ledger, events);
            ledgers.push(ledger);
        }
        ledgers.sort();

        let page = peers.client.list(0, 0).await.unwrap();
        let seen: Vec<LedgerId> = page.items.iter().map(|entry| entry.ledger).collect();
        assert_eq!(seen, ledgers);
        assert!(!page.more);
        assert_eq!(page.items[0].event_count, 2);

        let second = peers.client.list(2, 0).await.unwrap();
        assert_eq!(second.items.len(), 1, "offset pages a stable order");
        assert_eq!(second.items[0].ledger, ledgers[2]);
    });
}

#[tokio::test]
async fn a_forks_request_returns_both_events_verbatim() {
    bounded!({
        let (peers, ledger, events) = with_events(3).await;
        let other = sample_chain(8, 3).1;
        peers.store.insert_fork(ForkRecord {
            ledger,
            seq: 1,
            kept: events[1].clone(),
            conflicting: other[1].clone(),
            observed_ms: 1_700_000_000_009,
            source_endpoint: Some(peers.client_endpoint.id()),
        });

        let page = peers.client.forks(Some(ledger), 0, 0).await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].kept, events[1]);
        assert_eq!(page.items[0].conflicting, other[1]);
        assert_eq!(page.items[0].seq, 1);

        let every = peers.client.forks(None, 0, 0).await.unwrap();
        assert_eq!(
            every.items, page.items,
            "an absent ledger means all of them"
        );
    });
}

#[tokio::test]
async fn limits_are_clamped_before_the_store_sees_them() {
    bounded!({
        let (peers, ledger, _) = with_events(2).await;
        peers.client.get(ledger, 0, 100_000).await.unwrap();
        peers.client.list(0, 100_000).await.unwrap();
        peers.client.forks(None, 0, 100_000).await.unwrap();
        peers.client.get(ledger, 0, 0).await.unwrap();

        let calls = peers.store.calls();
        assert!(calls.contains(&Call::Read {
            ledger,
            since: 0,
            limit: 512
        }));
        assert!(calls.contains(&Call::List {
            offset: 0,
            limit: 256
        }));
        assert!(calls.contains(&Call::Forks {
            ledger: None,
            offset: 0,
            limit: 64
        }));
    });
}

#[tokio::test]
async fn a_garbage_frame_answers_malformed() {
    bounded!({
        let (peers, _, _) = with_events(1).await;
        for garbage in [b"hello".to_vec(), vec![0x08, 0x01], vec![0xff; 7]] {
            assert_eq!(
                reject_code_of(&peers.client, &garbage).await,
                RejectCode::Malformed,
                "{garbage:?}"
            );
        }
    });
}

#[tokio::test]
async fn a_truncated_frame_answers_malformed() {
    bounded!({
        let (peers, ledger, _) = with_events(1).await;
        let full = wire::head_request(ledger);
        assert_eq!(
            reject_code_of(&peers.client, &full[..full.len() - 8]).await,
            RejectCode::Malformed
        );
    });
}

#[tokio::test]
async fn an_unknown_request_variant_answers_unsupported() {
    bounded!({
        let (peers, _, _) = with_events(1).await;
        // Field 6, length-delimited, empty: the shape a later version's
        // sixth request variant has.
        assert_eq!(
            reject_code_of(&peers.client, &[0x32, 0x00]).await,
            RejectCode::Unsupported
        );
    });
}

#[tokio::test]
async fn an_oversize_frame_answers_too_large() {
    bounded!({
        let (peers, _, _) = with_events(1).await;
        let frame = vec![0u8; MAX_FRAME_BYTES + 1];
        assert_eq!(
            reject_code_of(&peers.client, &frame).await,
            RejectCode::TooLarge
        );
    });
}

#[tokio::test]
async fn the_single_event_cap_is_the_boundary_between_malformed_and_too_large() {
    bounded!({
        let (peers, ledger, _) = with_events(1).await;

        let at_cap = wire::push_request(ledger, &[vec![0u8; MAX_EVENT_BYTES]]);
        assert_eq!(
            reject_code_of(&peers.client, &at_cap).await,
            RejectCode::Malformed,
            "an event inside the cap is judged by the field table"
        );

        let over_cap = wire::push_request(ledger, &[vec![0u8; MAX_EVENT_BYTES + 1]]);
        assert_eq!(
            reject_code_of(&peers.client, &over_cap).await,
            RejectCode::TooLarge
        );
    });
}

#[tokio::test]
async fn the_push_count_cap_is_the_boundary_at_512_events() {
    bounded!({
        let peers = setup(ServerConfig::default()).await;
        let (ledger, events) = sample_chain(9, 513);

        let accepted = peers.client.push(ledger, &events[..512]).await.unwrap();
        assert_eq!(accepted.stored, 512);

        let frame = wire::push_request(ledger, &events);
        assert_eq!(
            reject_code_of(&peers.client, &frame).await,
            RejectCode::TooLarge
        );
        assert_eq!(
            peers
                .client
                .push(ledger, &events)
                .await
                .unwrap_err()
                .to_string(),
            Error::PushTooLarge {
                events: 513,
                bytes: events.iter().map(Vec::len).sum(),
            }
            .to_string(),
            "the client refuses an oversize push before sending it"
        );
    });
}

#[tokio::test]
async fn the_push_byte_cap_answers_too_large() {
    bounded!({
        let (peers, ledger, _) = with_events(1).await;
        let frame = wire::push_request(ledger, &[vec![0u8; MAX_PUSH_BYTES + 1]]);
        assert!(
            frame.len() < MAX_FRAME_BYTES,
            "the frame cap is not what fires"
        );
        assert_eq!(
            reject_code_of(&peers.client, &frame).await,
            RejectCode::TooLarge
        );
    });
}

#[tokio::test]
async fn a_saturated_verification_semaphore_answers_busy() {
    bounded!({
        let peers = setup(ServerConfig {
            max_concurrent_verifications: 1,
            ..ServerConfig::default()
        })
        .await;
        let (ledger, events) = sample_chain(10, 2);
        peers.store.insert(ledger, events);

        let hold = peers.store.hold_reads().await;
        let second = peers.dial().await.expect("a second connection opens");

        let first = peers.client.clone();
        let pending = tokio::spawn(async move { first.head(ledger).await });
        // The store fires this the moment a read starts, which is after the
        // request took the only verification permit.
        peers.store.read_started().await;

        let error = second
            .head(ledger)
            .await
            .expect_err("the second request finds no permit");
        let Error::Rejected(rejection) = error else {
            panic!("expected a rejection, got {error}");
        };
        assert_eq!(rejection.code, RejectCode::Busy);

        drop(hold);
        pending
            .await
            .expect("the first request finishes")
            .expect("the first request succeeds");
    });
}

#[tokio::test]
async fn the_connection_limit_closes_further_connections() {
    bounded!({
        let peers = setup(ServerConfig {
            max_connections: 1,
            ..ServerConfig::default()
        })
        .await;
        // A served request proves the first connection holds the only slot.
        peers.client.head(unknown_ledger()).await.unwrap();

        let refused = async {
            let second = peers.dial().await?;
            second.head(unknown_ledger()).await
        }
        .await;
        assert!(refused.is_err(), "the second connection is closed");
    });
}

#[tokio::test]
async fn the_per_connection_request_limit_closes_the_connection() {
    bounded!({
        let peers = setup(ServerConfig {
            max_requests_per_connection: 2,
            ..ServerConfig::default()
        })
        .await;
        peers.client.head(unknown_ledger()).await.unwrap();
        peers.client.head(unknown_ledger()).await.unwrap();
        assert!(
            peers.client.head(unknown_ledger()).await.is_err(),
            "the third request finds the connection closed"
        );

        // A fresh connection starts a fresh budget.
        let next = peers.dial().await.expect("a new connection opens");
        next.head(unknown_ledger()).await.unwrap();
    });
}

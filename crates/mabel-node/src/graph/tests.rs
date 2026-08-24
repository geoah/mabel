//! Crawler, store and reader tests, all offline: the fetcher is a table and
//! the clock is tokio's, so nothing here opens a socket or sleeps.

use std::time::Duration;

use mabel_core::IdentityId;

use crate::api::documents::{DeclaredKind, Id};
use crate::graph::crawl::{CrawlOptions, crawl, mint_sync_id};
use crate::graph::fetcher::{
    FetchOutcome, LedgerSummary, PlannedSource, TrustEdge, chain_named_sources, decide,
    plan_sources, record_hint,
};
use crate::graph::model::{
    FetchSource, GraphEdge, GraphNode, GraphSummary, NodeStatus, STALE_AFTER_MS, SourceClass,
    TruncatedBy,
};
use crate::graph::resolve::{MAX_DIALS, Resolution};
use crate::graph::store::{Generation, GraphStore, KEPT_GENERATIONS};
use crate::graph::stub::{STUB_FETCHED_AT_MS, StubFetcher, stub_attestation, stub_identity};
use crate::wallet::{LoadedLedger, WalletCore, ids};
use crate::{HomeOptions, NodeConfig, NodeHome};

/// The identity named by one byte, which is how these tests spell nodes.
fn node(seed: u8) -> IdentityId {
    stub_identity(seed)
}

/// Its rendered id.
fn id(seed: u8) -> Id {
    ids::identity(node(seed))
}

/// Options with the clock and the name fixed, so two runs produce the same
/// documents.
fn options() -> CrawlOptions {
    CrawlOptions {
        started_at_ms: 1_700_000_000_000,
        sync_id: Some("1700000000000-aaaaaaaa".to_owned()),
        ..CrawlOptions::new()
    }
}

/// `1 -> {3, 5}`, `3 -> {7}`, `5 -> {2, 7}`, with 2 and 7 leaves.
fn small_graph() -> StubFetcher {
    StubFetcher::new()
        .trusting(node(1), &[node(5), node(3)])
        .trusting(node(3), &[node(7)])
        .trusting(node(5), &[node(7), node(2)])
        .trusting(node(2), &[])
        .trusting(node(7), &[])
}

/// A folded summary naming nothing, which a source-order test then fills in.
fn summary_of(ledger: IdentityId) -> LedgerSummary {
    LedgerSummary {
        ledger,
        declared_kind: DeclaredKind::Person,
        display_name: None,
        hostname: None,
        email: None,
        head_seq: 0,
        head_event: crate::graph::stub::stub_head(ledger),
        endpoints: Vec::new(),
        witness_identities: Vec::new(),
        legacy_witnesses: Vec::new(),
        trust: Vec::new(),
    }
}

fn subjects(node: &GraphNode) -> Vec<Id> {
    node.edges.iter().map(|edge| edge.subject.clone()).collect()
}

#[tokio::test]
async fn the_walk_is_breadth_first_with_ties_broken_by_ascending_id() {
    let fetcher = small_graph();
    let generation = crawl(&[node(1)], &options().with_depth(3), &fetcher).await;

    assert_eq!(
        fetcher.calls(),
        vec![node(1), node(3), node(5), node(2), node(7)],
        "each level is walked in ascending id order"
    );
    assert_eq!(generation.node(&id(1)).unwrap().depth, 0);
    assert_eq!(generation.node(&id(3)).unwrap().depth, 1);
    assert_eq!(generation.node(&id(5)).unwrap().depth, 1);
    assert_eq!(generation.node(&id(2)).unwrap().depth, 2);
    assert_eq!(generation.node(&id(7)).unwrap().depth, 2);
    assert_eq!(generation.summary.node_count, 5);
    assert_eq!(generation.summary.edge_count, 5);
    assert!(!generation.summary.truncated);
    assert_eq!(generation.summary.truncated_by, None);
}

#[tokio::test]
async fn edges_are_kept_in_the_order_the_ledger_recorded_them() {
    let fetcher = small_graph();
    let generation = crawl(&[node(1)], &options(), &fetcher).await;

    assert_eq!(
        subjects(generation.node(&id(1)).unwrap()),
        vec![id(5), id(3)],
        "an edge list is the ledger's order, not the crawl's"
    );
}

#[tokio::test]
async fn two_crawls_over_one_graph_produce_the_same_generation() {
    let first = crawl(&[node(1)], &options().with_depth(1), &small_graph()).await;
    let second = crawl(&[node(1)], &options().with_depth(1), &small_graph()).await;
    assert_eq!(first, second);

    let truncated = CrawlOptions {
        max_nodes: 2,
        ..options()
    };
    let first = crawl(&[node(1)], &truncated, &small_graph()).await;
    let second = crawl(&[node(1)], &truncated, &small_graph()).await;
    assert_eq!(first, second, "a truncated crawl is reproducible too");
}

#[tokio::test]
async fn depth_is_held_inside_one_to_four() {
    assert_eq!(options().with_depth(0).bounded_depth(), 1);
    assert_eq!(options().with_depth(1).bounded_depth(), 1);
    assert_eq!(options().with_depth(4).bounded_depth(), 4);
    assert_eq!(options().with_depth(9).bounded_depth(), 4);
    assert_eq!(CrawlOptions::new().bounded_depth(), 2);

    let generation = crawl(&[node(1)], &options().with_depth(0), &small_graph()).await;
    assert_eq!(generation.summary.depth, 1, "the summary reports the bound");
    assert_eq!(generation.summary.node_count, 3, "roots plus one level");
}

#[tokio::test]
async fn the_depth_cap_truncates_and_names_itself() {
    let fetcher = StubFetcher::new()
        .trusting(node(1), &[node(2)])
        .trusting(node(2), &[node(3)])
        .trusting(node(3), &[node(4)])
        .trusting(node(4), &[]);
    let generation = crawl(&[node(1)], &options().with_depth(2), &fetcher).await;

    assert_eq!(generation.summary.node_count, 3);
    assert!(generation.summary.truncated);
    assert_eq!(generation.summary.truncated_by, Some(TruncatedBy::Depth));
    assert!(
        generation.node(&id(4)).is_none(),
        "the fourth node is past the depth the run was given"
    );
    assert_eq!(
        subjects(generation.node(&id(3)).unwrap()),
        vec![id(4)],
        "the frontier still records the edges that pointed further"
    );
}

#[tokio::test]
async fn the_node_cap_truncates_and_names_itself() {
    let generation = crawl(
        &[node(1)],
        &CrawlOptions {
            max_nodes: 2,
            ..options()
        },
        &small_graph(),
    )
    .await;

    assert_eq!(generation.summary.node_count, 2);
    assert_eq!(generation.summary.truncated_by, Some(TruncatedBy::Nodes));
    assert!(generation.summary.truncated);
    assert!(
        generation.node(&id(3)).is_some() && generation.node(&id(5)).is_none(),
        "the cap keeps the lowest ids of the level"
    );
}

#[tokio::test]
async fn the_fetch_cap_truncates_and_names_itself() {
    let fetcher = small_graph();
    let generation = crawl(
        &[node(1)],
        &CrawlOptions {
            max_fetches: 2,
            ..options()
        },
        &fetcher,
    )
    .await;

    assert_eq!(fetcher.call_count(), 2);
    assert_eq!(generation.summary.fetch_count, 2);
    assert_eq!(generation.summary.truncated_by, Some(TruncatedBy::Fetches));
}

#[tokio::test(start_paused = true)]
async fn the_run_clock_truncates_and_names_itself() {
    // Each fetch takes the whole per-fetch allowance and the budget covers
    // two levels, so the third level meets the clock. Time is tokio's, so
    // this test finishes instantly.
    let fetcher = StubFetcher::new()
        .with_delay(Duration::from_secs(5))
        .trusting(node(1), &[node(2)])
        .trusting(node(2), &[node(3)])
        .trusting(node(3), &[node(4)])
        .trusting(node(4), &[]);
    let generation = crawl(
        &[node(1)],
        &CrawlOptions {
            budget: Duration::from_secs(12),
            ..options().with_depth(4)
        },
        &fetcher,
    )
    .await;

    assert_eq!(
        fetcher.call_count(),
        3,
        "three fetches fit in twelve seconds"
    );
    assert_eq!(generation.summary.node_count, 3);
    assert_eq!(generation.summary.truncated_by, Some(TruncatedBy::Time));
}

#[tokio::test(start_paused = true)]
async fn fetches_within_a_level_run_together() {
    // Eight in flight, five in the level: one level costs one fetch's time.
    let fetcher = StubFetcher::new()
        .with_delay(Duration::from_secs(1))
        .trusting(node(1), &[node(2), node(3), node(4), node(5), node(6)]);
    let started = tokio::time::Instant::now();
    let generation = crawl(&[node(1)], &options().with_depth(1), &fetcher).await;

    assert_eq!(generation.summary.node_count, 6);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the level was not walked one fetch at a time"
    );
}

#[tokio::test]
async fn an_unreachable_ledger_is_a_node_with_a_status_and_the_walk_carries_on() {
    let fetcher = StubFetcher::new()
        .trusting(node(1), &[node(2), node(3)])
        .unreachable(node(2))
        .trusting(node(3), &[]);
    let generation = crawl(&[node(1)], &options(), &fetcher).await;

    let unreachable = generation.node(&id(2)).unwrap();
    assert_eq!(unreachable.status, NodeStatus::Unreachable);
    assert_eq!(unreachable.fetched_at_ms, None);
    assert_eq!(unreachable.declared_kind, None);
    assert!(unreachable.edges.is_empty());
    assert!(
        unreachable.stale(STUB_FETCHED_AT_MS),
        "never fetched is stale"
    );
    assert_eq!(generation.node(&id(3)).unwrap().status, NodeStatus::Ok);
    assert_eq!(generation.summary.node_count, 3);
}

#[tokio::test]
async fn equivocation_is_recorded_on_the_node_and_does_not_fail_the_run() {
    let first = iroh_base::SecretKey::from_bytes(&[11u8; 32]).public();
    let second = iroh_base::SecretKey::from_bytes(&[12u8; 32]).public();
    let fetcher = StubFetcher::new()
        .trusting(node(1), &[node(2)])
        .equivocating(
            node(2),
            &[node(3)],
            4,
            [
                (first, stub_attestation(node(2), node(8))),
                (second, stub_attestation(node(2), node(9))),
            ],
        );
    let generation = crawl(&[node(1)], &options(), &fetcher).await;

    let diverged = generation.node(&id(2)).unwrap();
    assert_eq!(diverged.status, NodeStatus::Equivocation);
    let equivocation = diverged.equivocation.as_ref().expect("both branches");
    assert_eq!(equivocation.at_seq, 4);
    assert_eq!(equivocation.branches.len(), 2);
    assert_eq!(
        equivocation.branches[0].source.endpoint(),
        Some(&ids::key(&first))
    );
    assert_eq!(
        equivocation.branches[1].source.endpoint(),
        Some(&ids::key(&second))
    );
    assert_ne!(
        equivocation.branches[0].event, equivocation.branches[1].event,
        "both event ids are on the record"
    );
    assert_eq!(
        generation.summary.equivocations,
        vec![id(2)],
        "the summary surfaces it"
    );
    assert!(
        generation.node(&id(3)).is_some(),
        "the walk continued past the divergence"
    );
}

#[tokio::test]
async fn a_node_two_roots_reach_carries_both_with_its_depth_from_each() {
    // Root 1 trusts 4 directly; root 2 trusts 3, which trusts 4.
    let fetcher = StubFetcher::new()
        .trusting(node(1), &[node(4)])
        .trusting(node(2), &[node(3)])
        .trusting(node(3), &[node(4)])
        .trusting(node(4), &[]);
    let generation = crawl(&[node(1), node(2)], &options(), &fetcher).await;

    let shared = generation.node(&id(4)).unwrap();
    assert_eq!(
        shared
            .roots
            .iter()
            .map(|reach| (reach.root.clone(), reach.depth))
            .collect::<Vec<_>>(),
        vec![(id(1), 1), (id(2), 2)]
    );
    assert_eq!(shared.depth, 1, "the crawl reached it at its shortest");
    assert_eq!(
        shared.discovered_via.as_ref().unwrap().identity,
        id(1),
        "provenance names the attestation that put it in the frontier"
    );
    assert_eq!(generation.summary.roots, vec![id(1), id(2)]);
}

#[tokio::test]
async fn a_root_the_home_cannot_reach_is_still_a_node() {
    let generation = crawl(&[node(1)], &options(), &StubFetcher::new()).await;
    assert_eq!(generation.summary.node_count, 1);
    assert_eq!(
        generation.node(&id(1)).unwrap().status,
        NodeStatus::Unreachable
    );
    assert!(!generation.summary.truncated);
}

#[tokio::test]
async fn a_crawl_writes_nothing_under_ledgers() {
    let home = tempfile::tempdir().unwrap();
    let home = NodeHome::create(home.path(), &NodeConfig::default(), HomeOptions::default())
        .expect("a fresh home");
    let before = home.ledgers().unwrap();

    let generation = crawl(&[node(1)], &options(), &small_graph()).await;
    GraphStore::in_home(&home).publish(&generation).unwrap();

    assert_eq!(before, home.ledgers().unwrap());
    assert!(
        home.ledgers().unwrap().is_empty(),
        "a crawl reads ledgers, it does not become a replica of them"
    );
    assert!(home.root().join("graph/current.json").is_file());
}

// The store.

fn store() -> (tempfile::TempDir, GraphStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = GraphStore::new(dir.path().join("graph"));
    (dir, store)
}

/// A generation named `sync_id` holding the small graph.
async fn generation(sync_id: &str) -> Generation {
    crawl(
        &[node(1)],
        &CrawlOptions {
            sync_id: Some(sync_id.to_owned()),
            ..options()
        },
        &small_graph(),
    )
    .await
}

#[tokio::test]
async fn a_generation_survives_a_write_and_a_read() {
    let (_dir, store) = store();
    let written = generation("1700000000001-aaaaaaaa").await;
    store.publish(&written).unwrap();

    let read = store.current_generation().unwrap().expect("a live pointer");
    assert_eq!(read, written, "every node and the summary round trip");
    assert_eq!(
        store.current().unwrap().unwrap().sync_id,
        "1700000000001-aaaaaaaa"
    );
    assert_eq!(
        read.node(&id(1)).unwrap().source,
        Some(FetchSource::Local),
        "the node records which source served it"
    );
    assert_eq!(
        read.node(&id(1)).unwrap().fetched_at_ms,
        Some(STUB_FETCHED_AT_MS)
    );
}

#[tokio::test]
async fn a_reader_sees_the_old_generation_until_the_pointer_swaps() {
    let (_dir, store) = store();
    let old = generation("1700000000001-aaaaaaaa").await;
    store.publish(&old).unwrap();

    let new = crawl(
        &[node(1)],
        &CrawlOptions {
            sync_id: Some("1700000000002-bbbbbbbb".to_owned()),
            ..options().with_depth(1)
        },
        &small_graph(),
    )
    .await;
    store.write_generation(&new).unwrap();

    let during = store.current_generation().unwrap().unwrap();
    assert_eq!(during, old, "a half-published sync is invisible");
    assert_eq!(during.summary.node_count, 5);

    store
        .set_current(&new.summary.sync_id, new.summary.last_sync_ms)
        .unwrap();
    let after = store.current_generation().unwrap().unwrap();
    assert_eq!(after, new);
    assert_eq!(after.summary.node_count, 3);
}

#[tokio::test]
async fn the_third_oldest_generation_is_collected() {
    let (_dir, store) = store();
    for suffix in 1..=3u8 {
        let sync_id = format!("170000000000{suffix}-aaaaaaaa");
        store.publish(&generation(&sync_id).await).unwrap();
    }

    let kept = store.generation_ids().unwrap();
    assert_eq!(kept.len(), KEPT_GENERATIONS);
    assert_eq!(
        kept,
        vec![
            "1700000000002-aaaaaaaa".to_owned(),
            "1700000000003-aaaaaaaa".to_owned()
        ],
        "the oldest goes, the live one and its predecessor stay"
    );
    assert!(
        store
            .generation("1700000000001-aaaaaaaa")
            .unwrap()
            .is_none()
    );
    assert!(store.current_generation().unwrap().is_some());
}

#[test]
fn an_empty_home_has_no_generation() {
    let (_dir, store) = store();
    assert!(store.current().unwrap().is_none());
    assert!(store.current_generation().unwrap().is_none());
    assert!(store.generation_ids().unwrap().is_empty());
}

#[test]
fn a_sync_id_outside_the_alphabet_is_refused() {
    let (_dir, store) = store();
    for bad in ["../escape", "Upper", "with space", ""] {
        assert!(
            store.generation_dir(bad).is_err(),
            "{bad} is not a generation name"
        );
        assert!(store.generation(bad).is_err());
    }
    assert!(store.generation_dir("1700000000001-aaaaaaaa").is_ok());
}

#[test]
fn a_sync_id_is_the_start_time_and_a_suffix() {
    let minted = mint_sync_id(1_700_000_000_000);
    assert!(minted.starts_with("1700000000000-"), "{minted}");
    assert_ne!(minted, mint_sync_id(1_700_000_000_000), "the suffix varies");
    assert!(
        mint_sync_id(1) < mint_sync_id(2),
        "names sort by start time"
    );
}

// The reader.

#[tokio::test]
async fn reverse_edges_are_labelled_best_effort() {
    let generation = crawl(&[node(1)], &options().with_depth(3), &small_graph()).await;

    let reverse = generation.reverse_edges(&id(7));
    assert!(reverse.best_effort, "the label is always on");
    assert_eq!(
        reverse
            .entries
            .iter()
            .map(|entry| entry.identity.clone())
            .collect::<Vec<_>>(),
        vec![id(3), id(5)],
        "who, in this crawl, trusts 7"
    );
    assert_eq!(
        reverse.entries[0].attestation_event,
        ids::event(stub_attestation(node(3), node(7)))
    );
    assert!(
        generation.reverse_edges(&id(1)).entries.is_empty(),
        "nobody in the crawl trusts the root"
    );
}

#[tokio::test]
async fn paths_are_the_shortest_ones_and_degrees_are_absent_without_a_path() {
    // 1 trusts 3 and 5, both trust 7: two shortest paths of two hops.
    let generation = crawl(&[node(1)], &options().with_depth(3), &small_graph()).await;

    assert_eq!(generation.degrees(&id(1), &id(7)), Some(2));
    let paths = generation.paths(&id(1), &id(7));
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| path.degrees() == 2));
    assert_eq!(
        paths[0]
            .hops
            .iter()
            .map(|hop| hop.to.clone())
            .collect::<Vec<_>>(),
        vec![id(3), id(7)],
        "paths are listed in ascending id order"
    );
    assert_eq!(paths[0].hops[0].from, id(1));
    assert_eq!(
        paths[0].hops[0].attestation_event,
        ids::event(stub_attestation(node(1), node(3)))
    );

    assert_eq!(generation.degrees(&id(1), &id(1)), Some(0));
    assert_eq!(generation.paths(&id(1), &id(1)).len(), 1);
    assert_eq!(
        generation.degrees(&id(7), &id(1)),
        None,
        "trust is one way: no path back"
    );
    assert!(generation.paths(&id(7), &id(1)).is_empty());
    assert_eq!(
        generation.degrees(&id(1), &id(9)),
        None,
        "an identity this crawl never reached has no degrees"
    );
    assert!(generation.node(&id(9)).is_none());
}

#[tokio::test]
async fn at_most_three_shortest_paths_come_back() {
    let mut fetcher = StubFetcher::new().trusting(node(1), &[node(2), node(3), node(4), node(5)]);
    for hop in 2..=5u8 {
        fetcher = fetcher.trusting(node(hop), &[node(9)]);
    }
    let generation = crawl(
        &[node(1)],
        &options().with_depth(2),
        &fetcher.trusting(node(9), &[]),
    )
    .await;

    assert_eq!(generation.degrees(&id(1), &id(9)), Some(2));
    assert_eq!(generation.paths(&id(1), &id(9)).len(), 3);
    assert_eq!(generation.paths_up_to(&id(1), &id(9), 4).len(), 4);
    assert_eq!(generation.paths_up_to(&id(1), &id(9), 0).len(), 0);
}

#[test]
fn staleness_turns_over_at_twenty_four_hours() {
    let summary = GraphSummary {
        sync_id: "1700000000000-aaaaaaaa".to_owned(),
        last_sync_ms: 1_700_000_000_000,
        depth: 2,
        roots: vec![id(1)],
        node_count: 1,
        edge_count: 0,
        fetch_count: 1,
        truncated: false,
        truncated_by: None,
        equivocations: Vec::new(),
    };
    assert!(!summary.stale(summary.last_sync_ms));
    assert!(!summary.stale(summary.last_sync_ms + STALE_AFTER_MS));
    assert!(summary.stale(summary.last_sync_ms + STALE_AFTER_MS + 1));
    assert!(
        !summary.stale(summary.last_sync_ms - 1),
        "a clock that ran backwards does not age the crawl"
    );

    let node = GraphNode {
        identity_id: id(1),
        declared_kind: Some(DeclaredKind::Person),
        display_name: None,
        hostname: None,
        email: None,
        head_seq: Some(0),
        head_event: None,
        depth: 0,
        roots: Vec::new(),
        discovered_via: None,
        source: Some(FetchSource::Local),
        fetched_at_ms: Some(1_700_000_000_000),
        status: NodeStatus::Ok,
        equivocation: None,
        detail: None,
        edges: vec![GraphEdge {
            subject: id(2),
            attestation_event: ids::event(stub_attestation(node(1), node(2))),
            seq: 1,
        }],
    };
    assert!(!node.stale(1_700_000_000_000 + STALE_AFTER_MS));
    assert!(node.stale(1_700_000_000_000 + STALE_AFTER_MS + 1));
    assert_eq!(node.edge_to(&id(2)).unwrap().seq, 1);
    assert!(node.edge_to(&id(3)).is_none());
}

// The source order.

fn wallet_home() -> (tempfile::TempDir, WalletCore) {
    let dir = tempfile::tempdir().unwrap();
    let home =
        NodeHome::create(dir.path(), &NodeConfig::default(), HomeOptions::default()).unwrap();
    (dir, WalletCore::new(home))
}

#[test]
fn the_eight_sources_are_planned_in_the_order_the_proposal_gives() {
    let (_dir, core) = wallet_home();
    let held = core
        .create_identity("ada", DeclaredKind::Person, None, None, None)
        .expect("a local identity");
    let ledger = ids::parse_identity(&held.identity.identity_id).unwrap();

    let called = iroh_base::SecretKey::from_bytes(&[20u8; 32]).public();
    let hint = iroh_base::SecretKey::from_bytes(&[21u8; 32]).public();
    let witness = iroh_base::SecretKey::from_bytes(&[22u8; 32]).public();
    let learned = iroh_base::SecretKey::from_bytes(&[23u8; 32]).public();
    let advertised = iroh_base::SecretKey::from_bytes(&[24u8; 32]).public();
    let named = iroh_base::SecretKey::from_bytes(&[25u8; 32]).public();
    let legacy = iroh_base::SecretKey::from_bytes(&[26u8; 32]).public();

    let mut peers = core.home().peers().unwrap();
    peers.add_hint(ledger, hint);
    // The witness identity of source 6 is reachable through its own hint.
    peers.add_hint(node(9), named);
    core.home().write_peers(&peers).unwrap();
    let mut config = core.home().config().unwrap();
    config.witnesses = vec![crate::config::WitnessEntry::new(node(8), vec![witness])];
    core.home().write_config(&config).unwrap();

    let resolution = Resolution::for_operation().with_caller_hints(vec![called]);
    let planned = plan_sources(&core, ledger, &[learned], &resolution).unwrap();
    assert_eq!(
        planned
            .iter()
            .map(|source| source.source.clone())
            .collect::<Vec<_>>(),
        vec![
            FetchSource::Local,
            FetchSource::CallerHint {
                endpoint: ids::key(&called)
            },
            FetchSource::PeerHint {
                endpoint: ids::key(&hint)
            },
            FetchSource::PeerHint {
                endpoint: ids::key(&learned)
            },
            FetchSource::NodeWitness {
                witness: id(8),
                endpoint: ids::key(&witness)
            },
        ],
        "local copy, then the caller, then hints, then the node's witnesses"
    );
    assert_eq!(planned.iter().filter(|s| s.endpoint.is_none()).count(), 1);
    assert_eq!(
        planned
            .iter()
            .map(|source| source.source.order())
            .collect::<Vec<u8>>(),
        vec![1, 2, 3, 3, 4]
    );

    // Sources 5, 6 and 7 are reachable only once a copy verified, and never
    // repeat an endpoint that has already been asked.
    let summary = LedgerSummary {
        endpoints: vec![advertised, witness],
        witness_identities: vec![node(9)],
        legacy_witnesses: vec![legacy],
        ..summary_of(ledger)
    };
    let chain_named = chain_named_sources(&core, &planned, &summary, &resolution).unwrap();
    assert_eq!(
        chain_named,
        vec![
            PlannedSource {
                source: FetchSource::LedgerEndpoint {
                    endpoint: ids::key(&advertised)
                },
                endpoint: Some(advertised),
            },
            PlannedSource {
                source: FetchSource::WitnessIdentity {
                    witness: id(9),
                    endpoint: ids::key(&named)
                },
                endpoint: Some(named),
            },
            PlannedSource {
                source: FetchSource::LegacyWitnessHint {
                    endpoint: ids::key(&legacy)
                },
                endpoint: Some(legacy),
            },
        ],
        "the endpoint already asked as a node witness is not asked again"
    );
    assert_eq!(
        chain_named
            .iter()
            .map(|source| source.source.order())
            .collect::<Vec<u8>>(),
        vec![5, 6, 7]
    );
    for planned in &chain_named {
        assert_eq!(planned.source.class(), SourceClass::ChainNamed);
    }
    assert!(
        !chain_named[2].source.may_bind(),
        "a tag-11 endpoint never establishes a binding"
    );
    // Eight named endpoints, one of them asked twice, cost seven slots.
    assert_eq!(resolution.dialled(), 7);
}

/// An endpoint several sources name costs one slot, and 16 distinct endpoints
/// is the whole operation's ration (proposal 006 section 5.2).
#[test]
fn one_operation_stops_at_sixteen_distinct_endpoints() {
    let (_dir, core) = wallet_home();
    let hints: Vec<iroh_base::PublicKey> = (0..8)
        .map(|seed| iroh_base::SecretKey::from_bytes(&[100 + seed; 32]).public())
        .collect();
    let mut peers = core.home().peers().unwrap();
    for endpoint in &hints {
        peers.add_hint(node(1), *endpoint);
    }
    core.home().write_peers(&peers).unwrap();

    let resolution = Resolution::for_operation().with_caller_hints(hints[..4].to_vec());
    let planned = plan_sources(&core, node(1), &[], &resolution).unwrap();
    // The first four endpoints are named twice and cost four slots, not eight.
    assert_eq!(resolution.dialled(), 8);
    assert_eq!(planned.len(), 8);
    assert_eq!(resolution.spent(SourceClass::CallerHint), 4);
    assert_eq!(resolution.spent(SourceClass::PeerHint), 4);

    // A chain naming 16 witnesses spends the chain-named cap and no more, so
    // four dials are still there for `node.json.witnesses`.
    let witnesses: Vec<iroh_base::PublicKey> = (0..16)
        .map(|seed| iroh_base::SecretKey::from_bytes(&[150 + seed; 32]).public())
        .collect();
    let summary = LedgerSummary {
        endpoints: witnesses.clone(),
        ..summary_of(node(1))
    };
    let chain_named = chain_named_sources(&core, &planned, &summary, &resolution).unwrap();
    assert_eq!(
        chain_named.len(),
        4,
        "12 of the 16 are spent, 4 are reserved"
    );
    assert_eq!(resolution.dialled(), 12);
    for slot in 0..4u8 {
        assert!(
            resolution.admit(
                SourceClass::NodeWitness,
                iroh_base::SecretKey::from_bytes(&[200 + slot; 32]).public()
            ),
            "reserved slot {slot}"
        );
    }
    assert_eq!(resolution.dialled(), MAX_DIALS);
    assert!(resolution.exhausted());
}

/// A ledger naming itself in its own `WitnessSet` terminates, and a witness
/// named both in `node.json` and in the chain is resolved once (proposal 006
/// section 5.1).
#[test]
fn the_visited_set_terminates_self_reference_and_duplicate_defaults() {
    let (_dir, core) = wallet_home();
    let witness = iroh_base::SecretKey::from_bytes(&[30u8; 32]).public();
    let mut config = core.home().config().unwrap();
    config.witnesses = vec![crate::config::WitnessEntry::new(node(7), vec![witness])];
    core.home().write_config(&config).unwrap();

    let resolution = Resolution::for_operation();
    let planned = plan_sources(&core, node(1), &[], &resolution).unwrap();
    assert_eq!(resolution.resolutions(), 1);

    // The chain names the same witness the node configures, plus itself.
    let summary = LedgerSummary {
        witness_identities: vec![node(7), node(1)],
        ..summary_of(node(1))
    };
    let chain_named = chain_named_sources(&core, &planned, &summary, &resolution).unwrap();
    assert!(
        chain_named.is_empty(),
        "the configured witness is already asked and the self-reference names nothing: {chain_named:?}"
    );
    assert_eq!(
        resolution.resolutions(),
        2,
        "the configured witness is resolved once, the self-reference once"
    );
    assert_eq!(resolution.dialled(), 1);

    // Resolving either again runs nothing new.
    assert_eq!(
        resolution.witness_endpoints(&core, node(7)).unwrap(),
        [witness]
    );
    assert_eq!(resolution.resolutions(), 2);
}

/// The endpoints `node.json` records beside a witness id are what make it
/// reachable before anything is fetched (proposal 006 section 5.4).
#[test]
fn a_witness_resolves_through_its_bootstrap_endpoints() {
    let (_dir, core) = wallet_home();
    let bootstrap = iroh_base::SecretKey::from_bytes(&[40u8; 32]).public();
    let hinted = iroh_base::SecretKey::from_bytes(&[41u8; 32]).public();
    let called = iroh_base::SecretKey::from_bytes(&[42u8; 32]).public();
    let mut config = core.home().config().unwrap();
    config.witnesses = vec![crate::config::WitnessEntry::new(node(7), vec![bootstrap])];
    core.home().write_config(&config).unwrap();
    let mut peers = core.home().peers().unwrap();
    peers.add_hint(node(7), hinted);
    core.home().write_peers(&peers).unwrap();

    let resolution = Resolution::for_operation().with_caller_hints(vec![called]);
    assert_eq!(
        resolution.witness_endpoints(&core, node(7)).unwrap(),
        [called, hinted, bootstrap],
        "the caller first, then the hint, then the bootstrap record"
    );
}

#[test]
fn a_ledger_this_home_does_not_hold_has_no_local_source() {
    let (_dir, core) = wallet_home();
    let planned = plan_sources(&core, node(1), &[], &Resolution::for_operation()).unwrap();
    assert!(planned.is_empty(), "nothing to ask and nothing to invent");
}

#[test]
fn an_outcome_carries_its_summary_or_its_status() {
    let summary = LedgerSummary {
        ledger: node(1),
        declared_kind: DeclaredKind::Person,
        display_name: Some("Ada".to_owned()),
        hostname: Some("ada.example".to_owned()),
        email: Some("ada@ada.example".to_owned()),
        head_seq: 3,
        head_event: crate::graph::stub::stub_head(node(1)),
        endpoints: Vec::new(),
        witness_identities: Vec::new(),
        legacy_witnesses: Vec::new(),
        trust: vec![TrustEdge {
            subject: node(2),
            attestation_event: stub_attestation(node(1), node(2)),
            seq: 1,
        }],
    };
    let verified = FetchOutcome::verified(summary, FetchSource::Local, vec![FetchSource::Local]);
    assert_eq!(verified.status, NodeStatus::Ok);
    assert_eq!(
        verified.summary.as_ref().unwrap().display_name.as_deref(),
        Some("Ada")
    );

    let unreachable = FetchOutcome::unreachable(node(1), Vec::new());
    assert_eq!(unreachable.status, NodeStatus::Unreachable);
    assert!(unreachable.summary.is_none());

    let invalid = FetchOutcome::invalid(node(1), Vec::new(), "seq 2 does not verify");
    assert_eq!(invalid.status, NodeStatus::Invalid);
    assert_eq!(invalid.detail.as_deref(), Some("seq 2 does not verify"));
}

#[tokio::test]
async fn the_profile_a_ledger_publishes_lands_on_its_node() {
    let named = FetchOutcome::verified(
        LedgerSummary {
            ledger: node(2),
            declared_kind: DeclaredKind::Organization,
            display_name: Some("Acme".to_owned()),
            hostname: Some("acme.example".to_owned()),
            email: Some("hello@acme.example".to_owned()),
            head_seq: 7,
            head_event: crate::graph::stub::stub_head(node(2)),
            endpoints: Vec::new(),
            witness_identities: Vec::new(),
            legacy_witnesses: Vec::new(),
            trust: Vec::new(),
        },
        FetchSource::Local,
        vec![FetchSource::Local],
    );
    let fetcher = StubFetcher::new()
        .trusting(node(1), &[node(2)])
        .reply(named);
    let generation = crawl(&[node(1)], &options(), &fetcher).await;

    let organization = generation.node(&id(2)).unwrap();
    assert_eq!(organization.display_name.as_deref(), Some("Acme"));
    assert_eq!(organization.hostname.as_deref(), Some("acme.example"));
    assert_eq!(
        organization.declared_kind,
        Some(DeclaredKind::Organization),
        "the declared kind is carried as the chain declares it"
    );
    assert_eq!(organization.head_seq, Some(7));
}

#[test]
fn a_source_that_served_a_verified_copy_is_written_back_to_peers_json() {
    let (_dir, core) = wallet_home();
    let endpoint = iroh_base::SecretKey::from_bytes(&[31u8; 32]).public();

    record_hint(core.home(), node(1), endpoint);
    assert_eq!(core.home().peers().unwrap().hints(node(1)), [endpoint]);

    record_hint(core.home(), node(1), endpoint);
    assert_eq!(
        core.home().peers().unwrap().hints(node(1)),
        [endpoint],
        "the same source is recorded once"
    );
    assert!(core.home().peers().unwrap().hints(node(2)).is_empty());

    let planned = plan_sources(&core, node(1), &[], &Resolution::for_operation()).unwrap();
    assert_eq!(
        planned.first().map(|source| source.source.clone()),
        Some(FetchSource::PeerHint {
            endpoint: ids::key(&endpoint)
        }),
        "the next crawl asks it as a hint"
    );
}

#[test]
fn two_sources_that_diverge_record_both_branches_and_keep_the_first() {
    let shared = b"seq-0".to_vec();
    let first = LoadedLedger::fold(node(1), vec![shared.clone(), b"one".to_vec()]);
    let second = LoadedLedger::fold(node(1), vec![shared.clone(), b"other".to_vec()]);
    let hint = FetchSource::PeerHint {
        endpoint: ids::key(&iroh_base::SecretKey::from_bytes(&[41u8; 32]).public()),
    };
    let witness = FetchSource::NodeWitness {
        witness: id(8),
        endpoint: ids::key(&iroh_base::SecretKey::from_bytes(&[42u8; 32]).public()),
    };

    let diverged = decide(
        node(1),
        vec![hint.clone(), witness.clone()],
        vec![(hint.clone(), first.clone()), (witness.clone(), second)],
        None,
    );
    assert_eq!(diverged.status, NodeStatus::Equivocation);
    let equivocation = diverged.equivocation.as_ref().unwrap();
    assert_eq!(equivocation.at_seq, 1, "the sequence they disagree at");
    assert_eq!(
        equivocation
            .branches
            .iter()
            .map(|branch| branch.source.clone())
            .collect::<Vec<_>>(),
        vec![hint.clone(), witness.clone()],
        "both source endpoints are on the record"
    );
    assert_eq!(
        diverged.source.as_ref(),
        Some(&hint),
        "the first copy in source order is kept, and no branch is called true"
    );

    // One chain extending another is the same chain seen further along.
    let longer = LoadedLedger::fold(
        node(1),
        vec![shared.clone(), b"one".to_vec(), b"two".to_vec()],
    );
    let extended = decide(
        node(1),
        vec![hint.clone(), witness.clone()],
        vec![(hint, first), (witness.clone(), longer)],
        None,
    );
    assert_eq!(extended.status, NodeStatus::Ok);
    assert_eq!(extended.source.as_ref(), Some(&witness));
    assert!(extended.equivocation.is_none());

    let nothing = decide(node(1), vec![witness], Vec::new(), None);
    assert_eq!(nothing.status, NodeStatus::Unreachable);
}

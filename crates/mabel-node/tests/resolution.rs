//! Resolution against a real witness on loopback: the source order, the dial
//! budget, the source-8 gate and `peers.json` hygiene (proposal 006 section 5).
//!
//! Two Iroh endpoints in one process with relays disabled, so nothing here
//! touches DNS, a relay or the internet: the only resolver is a
//! [`StubResolver`], which is also how a test asserts that source 8 did not run.

#[macro_use]
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Served, TIMEOUT, rendered, witness_identity};
use mabel_core::IdentityId;
use mabel_node::api::documents::DeclaredKind;
use mabel_node::graph::{
    FetchSource, LedgerFetcher, NetLedgerFetcher, NodeStatus, Resolution, SourceClass,
};
use mabel_node::verification::{Resolver, StubResolver, TxtRecord, query_name};
use mabel_node::wallet::{WalletCore, WalletSync};
use mabel_node::{HomeOptions, NodeConfig, NodeHome, NodeRole, RelayMode, WitnessEntry};
use tempfile::TempDir;

/// A wallet home in a temp directory whose `node.json` names one witness
/// identity and the machines that answer for it, which is the whole of source 4.
struct Reader {
    _dir: TempDir,
    core: Arc<WalletCore>,
}

impl Reader {
    fn with_witness(endpoints: &[iroh_base::EndpointId]) -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let config = NodeConfig {
            role: NodeRole::Wallet,
            relay: RelayMode::Disabled,
            witnesses: vec![WitnessEntry::new(witness_identity(), endpoints.to_vec())],
            ..NodeConfig::default()
        };
        let home = NodeHome::create(dir.path(), &config, HomeOptions::default())
            .expect("the home is created");
        Self {
            _dir: dir,
            core: Arc::new(WalletCore::new(home)),
        }
    }

    fn identity(&self, alias: &str) -> IdentityId {
        self.core
            .create_identity(alias, DeclaredKind::Person, None, None, None)
            .expect("the identity is created")
            .identity
            .identity_id
            .as_str()
            .parse()
            .expect("a rendered identity id parses")
    }

    async fn sync(&self, peers: &[iroh::EndpointAddr]) -> WalletSync {
        let secret = self.core.home().node_key().expect("the node key reads");
        let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, peers)
            .await
            .expect("the endpoint binds");
        WalletSync::new(endpoint).with_timeout(Duration::from_secs(3))
    }
}

/// An endpoint id nothing answers at.
fn nowhere() -> iroh_base::EndpointId {
    iroh_base::SecretKey::from_bytes(&[0xADu8; 32]).public()
}

/// A push through the witness `node.json` alone names, then a fetch of the same
/// ledger by a home that holds nothing but that one config entry.
///
/// This is the push-path proof of ticket 035: the chain names a witness
/// identity, `node.json` names the endpoints that answer for it, and nothing
/// else in either home says where to dial.
#[tokio::test]
async fn a_push_and_a_fetch_resolve_the_witness_from_node_json_alone() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Reader::with_witness(&[witness.endpoint_id]);
        let alice = wallet.identity("alice");
        {
            let lock = wallet.core.append_lock(alice).await;
            wallet
                .core
                .set_witnesses(&lock, alice, &[witness_identity()])
                .expect("the witness set is appended");
        }

        // The endpoints a push dials come from resolution alone.
        let resolved = wallet
            .core
            .witnesses_of(alice)
            .expect("node.json and peers.json read");
        assert_eq!(resolved, [witness.endpoint_id]);

        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        let pushed = sync
            .push(&wallet.core, alice, &resolved)
            .await
            .expect("the push reports");
        assert_eq!(pushed.results.len(), 1);
        assert_eq!(
            pushed.results[0].status,
            mabel_node::api::documents::PushStatus::Accepted,
            "{pushed:?}"
        );

        // A second home that holds nothing of alice fetches her ledger through
        // the same one config entry.
        let reader = Reader::with_witness(&[witness.endpoint_id]);
        let reader_sync = reader.sync(std::slice::from_ref(&witness.addr)).await;
        let fetcher = NetLedgerFetcher::new((*reader.core).clone(), reader_sync);
        let resolution = Resolution::for_operation();
        let outcome = fetcher
            .fetch_candidate(alice, Vec::new(), &resolution)
            .await;
        assert_eq!(outcome.status, NodeStatus::Ok, "{outcome:?}");
        assert_eq!(
            outcome.source,
            Some(FetchSource::NodeWitness {
                witness: mabel_node::wallet::ids::identity(witness_identity()),
                endpoint: mabel_node::wallet::ids::key(&witness.endpoint_id),
            })
        );
        assert_eq!(resolution.spent(SourceClass::NodeWitness), 1);

        witness.stop().await;
    });
}

/// The same rule on the push path: an endpoint `--to` named accepted the push
/// and is still not written (proposal 006 section 5.3).
#[tokio::test]
async fn a_caller_hint_that_accepted_a_push_is_never_written_to_peers_json() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Reader::with_witness(&[]);
        let alice = wallet.identity("alice");
        {
            let lock = wallet.core.append_lock(alice).await;
            wallet
                .core
                .set_witnesses(&lock, alice, &[witness_identity()])
                .expect("the witness set is appended");
        }
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        let pushed = sync
            .push_from(
                &wallet.core,
                alice,
                &[witness.endpoint_id],
                &[witness.endpoint_id],
            )
            .await
            .expect("the push reports");
        assert_eq!(
            pushed.results[0].status,
            mabel_node::api::documents::PushStatus::Accepted,
            "{pushed:?}"
        );
        assert!(
            wallet
                .core
                .home()
                .peers()
                .expect("peers.json reads")
                .ledgers
                .is_empty(),
            "an endpoint a person named on a command line installs no durable hint"
        );

        witness.stop().await;
    });
}

/// An endpoint the caller named is asked and never written back (proposal 006
/// section 5.3): a link that reaches a paste into a search box must not install
/// a durable dial target for an identity the sender does not control.
#[tokio::test]
async fn a_caller_hint_that_served_the_copy_is_never_written_to_peers_json() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Reader::with_witness(&[witness.endpoint_id]);
        let alice = wallet.identity("alice");
        {
            let lock = wallet.core.append_lock(alice).await;
            wallet
                .core
                .set_witnesses(&lock, alice, &[witness_identity()])
                .expect("the witness set is appended");
        }
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push reports");

        // A reader with no configured witness at all: the caller's endpoint is
        // the only source it has.
        let reader = Reader::with_witness(&[]);
        let reader_sync = reader.sync(std::slice::from_ref(&witness.addr)).await;
        let fetcher = NetLedgerFetcher::new((*reader.core).clone(), reader_sync);
        let resolution = Resolution::for_operation().with_caller_hints(vec![witness.endpoint_id]);
        let outcome = fetcher
            .fetch_candidate(alice, Vec::new(), &resolution)
            .await;
        assert_eq!(outcome.status, NodeStatus::Ok, "{outcome:?}");
        assert_eq!(
            outcome.source,
            Some(FetchSource::CallerHint {
                endpoint: mabel_node::wallet::ids::key(&witness.endpoint_id),
            })
        );
        assert!(
            reader
                .core
                .home()
                .peers()
                .expect("peers.json reads")
                .ledgers
                .is_empty(),
            "a caller's endpoint served the operation it came with and nothing more"
        );

        witness.stop().await;
    });
}

/// Source 8 is queried only when sources 1 to 7 produced no reachable copy: a
/// DNS query tells a third-party resolver which identity this wallet is looking
/// for, and a fetch that already succeeded has nothing to ask.
#[tokio::test]
async fn source_eight_does_not_run_when_an_earlier_source_answered() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Reader::with_witness(&[witness.endpoint_id]);
        let alice = wallet.identity("alice");
        {
            let lock = wallet.core.append_lock(alice).await;
            wallet
                .core
                .replace_profile(&lock, alice, Some("Alice"), Some("alice.example"), None)
                .expect("the profile lands");
            wallet
                .core
                .set_witnesses(&lock, alice, &[witness_identity()])
                .expect("the witness set is appended");
        }
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push reports");

        // The wallet already holds alice, so source 1 answers.
        let resolver = Arc::new(StubResolver::new().with_records(
            &query_name("alice.example"),
            vec![
                TxtRecord::from_strings([format!("mabel={alice}")]),
                TxtRecord::from_strings([format!("mabel-endpoints={}", rendered(&nowhere()))]),
            ],
        ));
        let fetcher = NetLedgerFetcher::new((*wallet.core).clone(), sync)
            .with_resolver(resolver.clone() as Arc<dyn Resolver>);
        let outcome = fetcher
            .fetch_candidate(alice, Vec::new(), &Resolution::for_operation())
            .await;
        assert_eq!(outcome.status, NodeStatus::Ok, "{outcome:?}");
        assert!(
            resolver.queries().is_empty(),
            "an earlier source produced a reachable copy: {:?}",
            resolver.queries()
        );
        assert!(
            !outcome
                .sources_tried
                .iter()
                .any(|source| matches!(source, FetchSource::DnsEndpoint { .. })),
            "{outcome:?}"
        );

        witness.stop().await;
    });
}

/// With no source reachable, source 8 runs for the hostname a stale local copy
/// already claims, and only when the same response also names this identity
/// (row 2 of the applicability matrix, proposal 006 section 6).
///
/// This is the recovery path a rotation needs: the local copy is a copy but not
/// a reachable one, and every endpoint it records is dead.
#[tokio::test]
async fn source_eight_runs_when_no_endpoint_answered() {
    bounded!({
        let wallet = Reader::with_witness(&[nowhere()]);
        let alice = wallet.identity("alice");
        {
            let lock = wallet.core.append_lock(alice).await;
            wallet
                .core
                .replace_profile(&lock, alice, Some("Alice"), Some("alice.example"), None)
                .expect("the profile lands");
        }
        let named = iroh_base::SecretKey::from_bytes(&[0xB1u8; 32]).public();
        let resolver = Arc::new(StubResolver::new().with_records(
            &query_name("alice.example"),
            vec![
                TxtRecord::from_strings([format!("mabel={alice}")]),
                TxtRecord::from_strings([format!("mabel-endpoints={}", rendered(&named))]),
            ],
        ));
        let sync = wallet.sync(&[]).await;
        let fetcher = NetLedgerFetcher::new((*wallet.core).clone(), sync)
            .with_resolver(resolver.clone() as Arc<dyn Resolver>);
        let resolution = Resolution::for_operation();
        let outcome = fetcher
            .fetch_candidate(alice, Vec::new(), &resolution)
            .await;

        assert_eq!(
            resolver.queries(),
            [query_name("alice.example")],
            "one query, for the hostname the copy claims"
        );
        assert!(
            outcome.sources_tried.contains(&FetchSource::DnsEndpoint {
                hostname: "alice.example".to_owned(),
                endpoint: mabel_node::wallet::ids::key(&named),
            }),
            "{outcome:?}"
        );
        assert_eq!(resolution.spent(SourceClass::Dns), 1);
        // Nothing answers at either endpoint, so the local copy is what stands.
        assert_eq!(outcome.source, Some(FetchSource::Local));
    });
}

/// A zone that names other endpoints and not this identity offers this identity
/// nothing: the hostname came from the ledger's own claim, which is not
/// verification (row 2, proposal 006 section 6).
#[tokio::test]
async fn source_eight_ignores_a_zone_that_does_not_name_this_identity() {
    bounded!({
        let wallet = Reader::with_witness(&[nowhere()]);
        let alice = wallet.identity("alice");
        {
            let lock = wallet.core.append_lock(alice).await;
            wallet
                .core
                .replace_profile(&lock, alice, Some("Alice"), Some("alice.example"), None)
                .expect("the profile lands");
        }
        let named = iroh_base::SecretKey::from_bytes(&[0xB2u8; 32]).public();
        let resolver = Arc::new(StubResolver::new().with_records(
            &query_name("alice.example"),
            vec![TxtRecord::from_strings([format!(
                "mabel-endpoints={}",
                rendered(&named)
            )])],
        ));
        let sync = wallet.sync(&[]).await;
        let fetcher = NetLedgerFetcher::new((*wallet.core).clone(), sync)
            .with_resolver(resolver.clone() as Arc<dyn Resolver>);
        let resolution = Resolution::for_operation();
        let outcome = fetcher
            .fetch_candidate(alice, Vec::new(), &resolution)
            .await;
        assert_eq!(resolver.queries().len(), 1);
        assert_eq!(resolution.spent(SourceClass::Dns), 0);
        assert!(
            !outcome
                .sources_tried
                .iter()
                .any(|source| matches!(source, FetchSource::DnsEndpoint { .. })),
            "{outcome:?}"
        );
    });
}

/// The operation's deadline is shared, so a run that has spent it asks nothing
/// more (proposal 006 section 5.2).
#[tokio::test]
async fn a_run_stops_at_the_resolve_budget() {
    bounded!({
        let wallet = Reader::with_witness(&[nowhere()]);
        let alice = wallet.identity("alice");
        let sync = wallet.sync(&[]).await;
        let fetcher = NetLedgerFetcher::new((*wallet.core).clone(), sync);
        let spent = Resolution::with_budget(Duration::ZERO);
        assert!(spent.expired());
        let outcome = fetcher.fetch_candidate(alice, Vec::new(), &spent).await;
        assert!(
            outcome.sources_tried.is_empty(),
            "the deadline is gone before the first dial: {outcome:?}"
        );
        assert_eq!(outcome.status, NodeStatus::Unreachable);
    });
}

/// Every dial of one operation is charged to one budget, whichever ledger it was
/// for: 16 distinct endpoints is the whole operation's ration.
#[tokio::test]
async fn one_budget_covers_every_fetch_of_one_operation() {
    let wallet = Reader::with_witness(&[nowhere()]);
    let resolution = Resolution::for_operation();
    let mut peers = wallet.core.home().peers().expect("peers.json reads");
    for ledger in 0..4u8 {
        for hint in 0..4u8 {
            peers.record_success(
                common::subject(ledger),
                iroh_base::SecretKey::from_bytes(&[ledger * 16 + hint; 32]).public(),
                mabel_node::now_ms(),
            );
        }
    }
    wallet
        .core
        .home()
        .write_peers(&peers)
        .expect("peers.json is written");

    let mut planned = 0usize;
    for ledger in 0..4u8 {
        planned += mabel_node::graph::plan_sources(
            &wallet.core,
            common::subject(ledger),
            &[],
            &resolution,
        )
        .expect("the plan reads the home")
        .len();
    }
    // Four ledgers at four hints each is 16 endpoints, and the `PeerHint` cap of
    // 4 is what the second ledger onward runs into.
    assert_eq!(resolution.spent(SourceClass::PeerHint), 4);
    assert_eq!(resolution.spent(SourceClass::NodeWitness), 1);
    assert_eq!(resolution.dialled(), 5);
    assert_eq!(planned, 8, "one node witness per ledger, four hints once");
    assert_eq!(TIMEOUT, Duration::from_secs(10));
}

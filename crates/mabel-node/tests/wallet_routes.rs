//! The four wallet routes proposal 004 adds, over a real node home.
//!
//! `GET /api/witnesses` and `GET /api/resolve/{hostname}` run offline: the
//! first reads the home, the second reads a stub resolver. `GET
//! /api/witnesses/{endpoint_id}/ledgers` and `POST
//! /api/identities/{identity_id}/fetch` run against a witness serving
//! `mabel/ledger/0` on a loopback endpoint, with relays disabled, so nothing
//! here touches DNS, a relay or the internet (proposal 001 section 11).

#[macro_use]
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::Served;
use iroh_base::{EndpointId, SecretKey};
use mabel_core::IdentityId;
use mabel_node::api::documents::{DeclaredKind, Id, ResolveStatus};
use mabel_node::api::service::{FetchIdentity, WalletService};
use mabel_node::verification::{StubResolver, TxtRecord, VerificationStore};
use mabel_node::wallet::{WalletApiService, WalletCore, WalletSync};
use mabel_node::witness::WitnessCaps;
use mabel_node::{HomeOptions, NodeConfig, NodeHome, NodeRole, RelayMode};
use tempfile::TempDir;

/// A wallet home, the core over it and the HTTP service over that.
struct Wallet {
    _dir: TempDir,
    core: Arc<WalletCore>,
    service: WalletApiService,
}

impl Wallet {
    /// A fresh home whose `node.json` names `witnesses` as the node-wide
    /// default, dialling only the peers whose addresses are seeded.
    async fn new(witnesses: &[EndpointId], peers: &[iroh::EndpointAddr]) -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let config = NodeConfig {
            role: NodeRole::Wallet,
            relay: RelayMode::Disabled,
            witnesses: witnesses.to_vec(),
            ..NodeConfig::default()
        };
        let home = NodeHome::create(dir.path(), &config, HomeOptions::default())
            .expect("the home is created");
        let core = Arc::new(WalletCore::new(home));
        let secret = core.home().node_key().expect("the node key reads");
        let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, peers)
            .await
            .expect("the endpoint binds");
        let service = WalletApiService::new(
            core.clone(),
            // A peer that is expected not to answer must not hold the test
            // for the full ten seconds of a deliberate push.
            WalletSync::new(endpoint).with_timeout(Duration::from_secs(3)),
            "127.0.0.1:9080".parse().expect("a bind address"),
            RelayMode::Disabled,
        );
        Self {
            _dir: dir,
            core,
            service,
        }
    }

    /// A home with no node-wide witnesses and nothing to dial.
    async fn plain() -> Self {
        Self::new(&[], &[]).await
    }

    /// The same wallet, answering hostname lookups from `resolver`.
    fn with_resolver(mut self, resolver: Arc<StubResolver>) -> Self {
        self.service = self.service.with_resolver(resolver);
        self
    }

    fn identity(&self, alias: &str) -> IdentityId {
        let created = self
            .core
            .create_identity(alias, DeclaredKind::Person, None, None, None)
            .expect("the identity is created");
        created
            .identity
            .identity_id
            .as_str()
            .parse()
            .expect("a rendered identity id parses")
    }

    /// Names the witness identity every `Served` home here witnesses for, which
    /// is what admits a push (proposal 006 sections 1 and 4).
    async fn witnesses(&self, identity: IdentityId) {
        let lock = self.core.append_lock(identity).await;
        self.core
            .set_witnesses(&lock, identity, &[common::witness_identity()])
            .expect("the witness set is appended");
    }

    /// Appends a retired tag-11 `WitnessConfig` naming `endpoints`, the shape a
    /// chain written before proposal 006 carries. Nothing writes one any more;
    /// the fold and every read still accept it.
    async fn legacy_witnesses(&self, identity: IdentityId, endpoints: &[EndpointId]) {
        let lock = self.core.append_lock(identity).await;
        let mut loaded = self.core.load(identity).expect("the ledger loads");
        self.core
            .append(&lock, identity, &mut loaded, |signer, at, timestamp_ms| {
                mabel_core::sign::build_witness_config(signer, at, endpoints, timestamp_ms)
            })
            .expect("the witness config is appended");
    }
}

fn id(identity: IdentityId) -> Id {
    Id::parse(&identity.to_string()).expect("a rendered id")
}

/// An endpoint id from one seed byte. Nothing binds it, so dialling it fails.
fn endpoint(seed: u8) -> EndpointId {
    SecretKey::from_bytes(&[seed; 32]).public()
}

fn rendered(endpoint: EndpointId) -> Id {
    Id::parse(
        &data_encoding::BASE32_NOPAD
            .encode(endpoint.as_bytes())
            .to_ascii_lowercase(),
    )
    .expect("a rendered endpoint id")
}

// -------------------------------------------------------------- witnesses ----

#[tokio::test]
async fn the_witness_list_names_every_ledger_whose_legacy_config_holds_each_endpoint() {
    let shared = endpoint(1);
    let only_alice = endpoint(2);
    let only_default = endpoint(3);
    let wallet = Wallet::new(&[shared, only_default], &[]).await;
    let alice = wallet.identity("alice");
    let acme = wallet.identity("acme");
    wallet.legacy_witnesses(alice, &[shared, only_alice]).await;
    wallet.legacy_witnesses(acme, &[shared]).await;

    let listed = wallet.service.witnesses().await.expect("a witness list");
    let endpoints: Vec<&str> = listed
        .witnesses
        .iter()
        .map(|witness| witness.endpoint_id.as_str())
        .collect();
    let mut sorted = endpoints.clone();
    sorted.sort_unstable();
    assert_eq!(endpoints, sorted, "sorted by ascending endpoint id");

    let entry = |wanted: EndpointId| {
        listed
            .witnesses
            .iter()
            .find(|witness| witness.endpoint_id == rendered(wanted))
            .unwrap_or_else(|| panic!("{} is not listed", rendered(wanted)))
    };
    let mut both = vec![id(alice), id(acme)];
    both.sort();
    assert_eq!(entry(shared).named_by, both);
    assert!(entry(shared).is_node_default);
    assert_eq!(entry(only_alice).named_by, vec![id(alice)]);
    assert!(
        !entry(only_alice).is_node_default,
        "a witness only a ledger names is still listed"
    );
    assert!(entry(only_default).named_by.is_empty());
    assert!(entry(only_default).is_node_default);
}

#[tokio::test]
async fn a_wallet_that_configured_nothing_lists_no_witness() {
    let wallet = Wallet::plain().await;
    wallet.identity("alice");
    let listed = wallet.service.witnesses().await.expect("a witness list");
    assert!(listed.witnesses.is_empty(), "{listed:?}");
}

// ---------------------------------------------------------------- resolve ----

#[tokio::test]
async fn a_matching_txt_record_resolves_to_the_identity_it_names() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let resolver =
        Arc::new(StubResolver::new().with_text("_mabel.alice.example.", &format!("mabel={alice}")));
    let wallet = wallet.with_resolver(resolver.clone());

    let resolved = wallet
        .service
        .resolve("alice.example".to_owned())
        .await
        .expect("an answer");
    assert_eq!(resolved.status, ResolveStatus::Resolved);
    assert_eq!(resolved.identity_id, Some(id(alice)));
    assert_eq!(resolved.hostname, "alice.example");
    // One lookup, of the absolute label, and no CNAME chase.
    assert_eq!(resolver.queries(), vec!["_mabel.alice.example.".to_owned()]);
}

#[tokio::test]
async fn a_label_with_no_mabel_record_is_no_record_and_one_that_does_not_parse_is_mismatched() {
    let resolver = Arc::new(
        StubResolver::new()
            .with_text("_mabel.plain.example.", "v=spf1 -all")
            .with_records(
                "_mabel.broken.example.",
                vec![
                    TxtRecord::from_strings(["mabel=not-an-identity"]),
                    TxtRecord::from_strings(["mabel="]),
                ],
            )
            .with_failure("_mabel.down.example.", "SERVFAIL"),
    );
    let wallet = Wallet::plain().await.with_resolver(resolver);

    for (hostname, expected) in [
        ("plain.example", ResolveStatus::NoRecord),
        ("absent.example", ResolveStatus::NoRecord),
        ("broken.example", ResolveStatus::MismatchedRecords),
        ("down.example", ResolveStatus::Unreachable),
    ] {
        let resolved = wallet
            .service
            .resolve(hostname.to_owned())
            .await
            .expect("an answer");
        assert_eq!(resolved.status, expected, "{hostname}");
        assert_eq!(resolved.identity_id, None, "{hostname}");
    }
}

#[tokio::test]
async fn resolving_a_hostname_writes_no_verification_entry() {
    // Navigation is not verification: a hostname typed into a search box gets
    // no cached verdict and leaves none (proposal 004).
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let resolver =
        Arc::new(StubResolver::new().with_text("_mabel.alice.example.", &format!("mabel={alice}")));
    let wallet = wallet.with_resolver(resolver);

    wallet
        .service
        .resolve("alice.example".to_owned())
        .await
        .expect("an answer");

    let cached = VerificationStore::new(wallet.core.home())
        .read(alice)
        .expect("the cache reads");
    assert!(cached.is_none(), "{cached:?}");
}

// -------------------------------------------------- the witness ledger list ----

#[tokio::test]
async fn the_witness_ledger_proxy_lists_what_that_witness_holds() {
    bounded!({
        let witness = Served::start(WitnessCaps::default()).await;
        let wallet = Wallet::new(&[witness.endpoint_id], std::slice::from_ref(&witness.addr)).await;
        let alice = wallet.identity("alice");
        wallet.witnesses(alice).await;
        let pushed = wallet
            .service
            .push(mabel_node::api::service::PushRequest {
                identity_id: id(alice),
                to: None,
            })
            .await
            .expect("the push reports");
        assert_eq!(pushed.results.len(), 1);

        let page = wallet
            .service
            .witness_ledgers(
                rendered(witness.endpoint_id),
                mabel_node::api::service::PageRequest {
                    offset: 0,
                    limit: 256,
                },
            )
            .await
            .expect("the witness answers");

        assert_eq!(page.endpoint_id, rendered(witness.endpoint_id));
        assert_eq!(page.offset, 0);
        assert_eq!(page.limit, 256);
        assert!(!page.more);
        assert_eq!(page.ledgers.len(), 1);
        let row = &page.ledgers[0];
        assert_eq!(row.ledger_id, id(alice));
        assert_eq!(row.declared_kind, DeclaredKind::Person);
        assert_eq!(row.head_seq, 1, "inception plus the witness config");
        assert_eq!(row.event_count, 2);
        assert_eq!(row.fork_count, 0);

        // The proxy reads live and stores nothing: the wallet's own home is
        // unchanged by the browse.
        assert_eq!(
            wallet.core.home().ledgers().expect("the home lists"),
            vec![alice]
        );
        witness.stop().await;
    });
}

#[tokio::test]
async fn a_witness_that_cannot_be_dialled_answers_witness_unreachable() {
    bounded!({
        let wallet = Wallet::new(&[endpoint(9)], &[]).await;
        let error = wallet
            .service
            .witness_ledgers(
                rendered(endpoint(9)),
                mabel_node::api::service::PageRequest {
                    offset: 0,
                    limit: 256,
                },
            )
            .await
            .expect_err("nothing binds that endpoint");
        assert_eq!(error.reason(), "witness_unreachable");
        assert_eq!(error.code(), 30);
        assert_eq!(
            error
                .details()
                .get("endpoint_id")
                .and_then(|id| id.as_str()),
            Some(rendered(endpoint(9)).as_str())
        );
    });
}

// ------------------------------------------------------------------ fetch ----

#[tokio::test]
async fn a_fetch_stores_the_ledger_and_answers_the_cli_document() {
    bounded!({
        let witness = Served::start(WitnessCaps::default()).await;
        let owner = Wallet::new(&[witness.endpoint_id], std::slice::from_ref(&witness.addr)).await;
        let alice = owner.identity("alice");
        owner.witnesses(alice).await;
        owner
            .service
            .push(mabel_node::api::service::PushRequest {
                identity_id: id(alice),
                to: None,
            })
            .await
            .expect("the push reports");

        let reader = Wallet::new(&[witness.endpoint_id], std::slice::from_ref(&witness.addr)).await;
        let fetched = reader
            .service
            .fetch_identity(FetchIdentity {
                identity_id: id(alice),
                from: None,
            })
            .await
            .expect("the witness serves the ledger");

        assert_eq!(fetched.ledger_id, id(alice));
        assert_eq!(fetched.source, rendered(witness.endpoint_id));
        assert_eq!(fetched.event_count, 2);
        assert_eq!(fetched.stored, 2);
        assert_eq!(fetched.head_seq, 1);
        assert_eq!(
            fetched.controlled_by, None,
            "no key in this home signs for it"
        );
        assert!(
            reader.core.holds(alice).expect("the home reads"),
            "the fetch persisted the ledger like the CLI does"
        );

        // Fetching again is idempotent: the same chain, nothing newly stored.
        let again = reader
            .service
            .fetch_identity(FetchIdentity {
                identity_id: id(alice),
                from: Some(rendered(witness.endpoint_id)),
            })
            .await
            .expect("the second fetch answers");
        assert_eq!(again.stored, 0);
        assert_eq!(again.event_count, 2);
        witness.stop().await;
    });
}

#[tokio::test]
async fn a_fetch_from_an_endpoint_this_wallet_knows_no_witness_at_is_refused() {
    bounded!({
        let wallet = Wallet::new(&[endpoint(4)], &[]).await;
        let stranger = rendered(endpoint(77));
        let error = wallet
            .service
            .fetch_identity(FetchIdentity {
                identity_id: rendered(endpoint(5)),
                from: Some(stranger.clone()),
            })
            .await
            .expect_err("this wallet knows no witness there");
        assert_eq!(error.reason(), "unknown_witness");
        assert_eq!(error.code(), 2);
        assert_eq!(
            error
                .details()
                .get("endpoint_id")
                .and_then(|id| id.as_str()),
            Some(stranger.as_str())
        );
    });
}

#[tokio::test]
async fn a_fetch_with_no_witness_to_ask_says_so_before_dialling() {
    let wallet = Wallet::plain().await;
    let error = wallet
        .service
        .fetch_identity(FetchIdentity {
            identity_id: rendered(endpoint(5)),
            from: None,
        })
        .await
        .expect_err("no source is configured");
    assert_eq!(error.reason(), "no_witness_configured");
    assert_eq!(error.code(), 2);
}

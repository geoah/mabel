//! The four wallet routes proposal 004 adds, over a real node home.
//!
//! `GET /api/witnesses` and `GET /api/resolve?input=` run offline: the
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
use mabel_node::StorageCaps;
use mabel_node::api::documents::{DeclaredKind, Id, ResolveInputKind, ResolveStatus};
use mabel_node::api::service::{FetchIdentity, NodeService, ResolveInput};
use mabel_node::verification::{StubResolver, TxtRecord, VerificationStore};
use mabel_node::wallet::{NodeApiService, WalletCore, WalletSync};
use mabel_node::{HomeOptions, LedgerStorage, NodeConfig, NodeHome, RelayMode};
use tempfile::TempDir;

/// A wallet home, the core over it and the HTTP service over that.
struct Wallet {
    _dir: TempDir,
    core: Arc<WalletCore>,
    service: NodeApiService,
}

impl Wallet {
    /// A fresh home whose `node.json` names `witnesses` as the node-wide
    /// default, dialling only the peers whose addresses are seeded.
    async fn new(witnesses: &[EndpointId], peers: &[iroh::EndpointAddr]) -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let config = NodeConfig {
            relay: RelayMode::Disabled,
            witnesses: vec![mabel_node::WitnessEntry::new(
                common::witness_identity(),
                witnesses.to_vec(),
            )],
            ..NodeConfig::default()
        };
        let home = NodeHome::create(dir.path(), &config, HomeOptions::default())
            .expect("the home is created");
        let core = Arc::new(WalletCore::new(home));
        let secret = core.home().node_key().expect("the node key reads");
        let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, peers)
            .await
            .expect("the endpoint binds");
        let storage = Arc::new(
            LedgerStorage::open_from_config(core.home().clone(), endpoint.id())
                .expect("the index builds"),
        );
        let core = Arc::new(WalletCore::new(core.home().clone()).with_index(Arc::clone(&storage)));
        let service = NodeApiService::new(
            core.clone(),
            storage,
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

    /// A home whose `node.json` names the witness identity with no machine
    /// beside it, and nothing to dial.
    async fn plain() -> Self {
        Self::new(&[], &[]).await
    }

    /// A home whose `node.json` configures no witness at all.
    async fn with_no_configured_witness() -> Self {
        let wallet = Self::new(&[], &[]).await;
        let mut config = wallet.core.home().config().expect("node.json reads");
        config.witnesses = Vec::new();
        wallet
            .core
            .home()
            .write_config(&config)
            .expect("node.json is written");
        wallet
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
async fn the_witness_list_names_every_ledger_whose_witness_set_holds_each_identity() {
    let configured = common::witness_identity();
    let wallet = Wallet::new(&[endpoint(3)], &[]).await;
    let alice = wallet.identity("alice");
    let acme = wallet.identity("acme");
    // A witness set names identities (proposal 006 section 1), and the machines
    // that answer for one come from resolution.
    wallet.witnesses(alice).await;
    wallet.witnesses(acme).await;

    let listed = wallet.service.witnesses().await.expect("a witness list");
    let identities: Vec<&str> = listed
        .witnesses
        .iter()
        .map(|witness| witness.identity_id.as_str())
        .collect();
    let mut sorted = identities.clone();
    sorted.sort_unstable();
    assert_eq!(identities, sorted, "sorted by ascending identity id");

    let entry = |wanted: mabel_core::IdentityId| {
        listed
            .witnesses
            .iter()
            .find(|witness| witness.identity_id == id(wanted))
            .unwrap_or_else(|| panic!("{wanted} is not listed"))
    };
    let mut both = vec![id(alice), id(acme)];
    both.sort();
    assert_eq!(entry(configured).named_by, both);
    assert!(
        entry(configured).is_node_default,
        "node.json names this identity"
    );
    assert_eq!(
        entry(configured)
            .endpoints
            .iter()
            .map(|machine| machine.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec![rendered(endpoint(3)).as_str()],
        "the bootstrap endpoint node.json records beside the identity"
    );
    assert_eq!(
        entry(configured).endpoints[0].binding,
        mabel_node::Binding::Hinted,
        "nothing this home holds confirms the machine yet"
    );
    assert!(
        !entry(configured).stored,
        "this home holds no copy of the witness identity's ledger"
    );
}

/// A home whose ledgers name no witness and whose `node.json` configures none
/// lists none: the rows come from what a chain names and what `node.json`
/// names, and nothing else (proposal 006 section 8).
#[tokio::test]
async fn a_home_that_configured_nothing_lists_no_witness() {
    let wallet = Wallet::with_no_configured_witness().await;
    wallet.identity("alice");
    let listed = wallet.service.witnesses().await.expect("a witness list");
    assert!(listed.witnesses.is_empty(), "{listed:?}");
}

/// An id equal to an endpoint a stored ledger lists is refused before any dial,
/// including the retired tag-11 list a chain written before proposal 006 carries
/// (proposal 006 section 8).
#[tokio::test]
async fn an_endpoint_a_stored_ledger_names_is_not_a_witness_identity() {
    let machine = endpoint(7);
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    wallet.legacy_witnesses(alice, &[machine]).await;

    let error = wallet
        .service
        .witness_holdings(
            rendered(machine),
            mabel_node::api::service::PageRequest {
                offset: 0,
                limit: 256,
            },
        )
        .await
        .expect_err("that id names a machine, not an identity");
    assert_eq!(error.reason(), "endpoint_not_identity");
    assert_eq!(error.code(), 2);
    assert_eq!(error.status(), axum::http::StatusCode::NOT_FOUND);
}

/// A witness identity `node.json` names with no machine beside it is listed,
/// with no machines: it is a witness this home knows and cannot reach yet.
#[tokio::test]
async fn a_configured_witness_with_no_bootstrap_endpoint_is_listed_with_no_machine() {
    let wallet = Wallet::plain().await;
    let listed = wallet.service.witnesses().await.expect("a witness list");
    assert_eq!(listed.witnesses.len(), 1, "{listed:?}");
    let entry = &listed.witnesses[0];
    assert_eq!(entry.identity_id, id(common::witness_identity()));
    assert!(entry.endpoints.is_empty());
    assert!(entry.is_node_default);
    assert!(!entry.stored);
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
        .resolve(ResolveInput::Hostname("alice.example".to_owned()))
        .await
        .expect("an answer");
    assert_eq!(resolved.input_kind, ResolveInputKind::Hostname);
    assert_eq!(resolved.status, Some(ResolveStatus::Resolved));
    assert_eq!(resolved.identity_id, Some(id(alice)));
    assert_eq!(resolved.hostname.as_deref(), Some("alice.example"));
    assert!(resolved.endpoints.is_empty(), "the zone hints at nothing");
    // One lookup, of the absolute label, and no CNAME chase.
    assert_eq!(resolver.queries(), vec!["_mabel.alice.example.".to_owned()]);
}

/// An identity id and a link are answered from the string alone: neither
/// queries DNS, and a link's hints come back in the order it named them
/// (proposal 006 section 7).
#[tokio::test]
async fn an_identity_id_and_a_link_are_answered_without_a_lookup() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let resolver = Arc::new(StubResolver::new());
    let wallet = wallet.with_resolver(resolver.clone());

    let resolved = wallet
        .service
        .resolve(ResolveInput::Identity(id(alice)))
        .await
        .expect("an answer");
    assert_eq!(resolved.input_kind, ResolveInputKind::Identity);
    assert_eq!(resolved.identity_id, Some(id(alice)));
    assert_eq!(resolved.hostname, None);
    assert_eq!(resolved.status, None, "nothing was queried");
    assert!(resolved.endpoints.is_empty());

    let hints = vec![rendered(endpoint(1)), rendered(endpoint(2))];
    let resolved = wallet
        .service
        .resolve(ResolveInput::Link {
            identity_id: id(alice),
            endpoints: hints.clone(),
        })
        .await
        .expect("an answer");
    assert_eq!(resolved.input_kind, ResolveInputKind::Link);
    assert_eq!(resolved.identity_id, Some(id(alice)));
    assert_eq!(resolved.hostname, None);
    assert_eq!(resolved.status, None);
    assert_eq!(resolved.endpoints, hints);

    assert!(resolver.queries().is_empty(), "no lookup ran");
}

/// Row 1 of the applicability matrix: a hostname the caller supplied yields
/// the endpoints at that label for the identity the same response resolved to,
/// and a response that resolved to no identity reports none (proposal 006
/// section 6).
#[tokio::test]
async fn a_supplied_hostname_carries_the_endpoints_at_its_label() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let hints = format!(
        "mabel-endpoints={},{}",
        rendered(endpoint(1)),
        rendered(endpoint(2))
    );
    let resolver = Arc::new(
        StubResolver::new()
            .with_records(
                "_mabel.alice.example.",
                vec![
                    TxtRecord::from_strings([format!("mabel={alice}")]),
                    TxtRecord::from_strings([hints.clone()]),
                ],
            )
            // A zone naming machines and no identity resolves to nobody, so it
            // offers nobody's endpoints.
            .with_records(
                "_mabel.machines.example.",
                vec![TxtRecord::from_strings([hints])],
            ),
    );
    let wallet = wallet.with_resolver(resolver);

    let resolved = wallet
        .service
        .resolve(ResolveInput::Hostname("alice.example".to_owned()))
        .await
        .expect("an answer");
    assert_eq!(resolved.identity_id, Some(id(alice)));
    let mut expected = vec![rendered(endpoint(1)), rendered(endpoint(2))];
    expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    assert_eq!(resolved.endpoints, expected, "sorted by rendered base32");

    let resolved = wallet
        .service
        .resolve(ResolveInput::Hostname("machines.example".to_owned()))
        .await
        .expect("an answer");
    assert_eq!(resolved.status, Some(ResolveStatus::NoRecord));
    assert_eq!(resolved.identity_id, None);
    assert!(resolved.endpoints.is_empty(), "{resolved:?}");
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
            .resolve(ResolveInput::Hostname(hostname.to_owned()))
            .await
            .expect("an answer");
        assert_eq!(resolved.status, Some(expected), "{hostname}");
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
        .resolve(ResolveInput::Hostname("alice.example".to_owned()))
        .await
        .expect("an answer");

    let cached = VerificationStore::new(wallet.core.home())
        .read(alice)
        .expect("the cache reads");
    assert!(cached.is_none(), "{cached:?}");
}

// -------------------------------------------------- the witness ledger list ----

#[tokio::test]
async fn the_witness_holdings_proxy_lists_what_that_witness_holds() {
    bounded!({
        let witness = Served::start(StorageCaps::default()).await;
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
            .witness_holdings(
                id(common::witness_identity()),
                mabel_node::api::service::PageRequest {
                    offset: 0,
                    limit: 256,
                },
            )
            .await
            .expect("the witness answers");

        assert_eq!(page.identity_id, id(common::witness_identity()));
        assert_eq!(page.endpoint_id, rendered(witness.endpoint_id));
        assert_eq!(page.offset, 0);
        assert_eq!(page.limit, 256);
        assert!(!page.more);
        // Alice's ledger and the witness identity's own, which the witness home
        // stores so it may take a ledger at all (proposal 006 section 4.1).
        assert_eq!(page.ledgers.len(), 2);
        let row = page
            .ledgers
            .iter()
            .find(|row| row.ledger_id == id(alice))
            .expect("the witness holds alice's ledger");
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
        // The identity resolves to one machine, from the bootstrap endpoints
        // `node.json` records beside it, and nothing binds that machine.
        let wallet = Wallet::new(&[endpoint(9)], &[]).await;
        let witness = common::witness_identity();
        let error = wallet
            .service
            .witness_holdings(
                id(witness),
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
                .get("identity_id")
                .and_then(|id| id.as_str()),
            Some(id(witness).as_str())
        );
        assert_eq!(
            error.details().get("endpoints_tried"),
            Some(&serde_json::json!([rendered(endpoint(9))])),
            "the refusal names every machine that was dialled"
        );
    });
}

// ------------------------------------------------------------------ fetch ----

#[tokio::test]
async fn a_fetch_stores_the_ledger_and_answers_the_cli_document() {
    bounded!({
        let witness = Served::start(StorageCaps::default()).await;
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
                from_witness: None,
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
                from_witness: None,
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

/// `from` is a plain `CallerHint`: an endpoint this wallet has never heard of
/// is asked anyway, because a human named it for this request (proposal 006
/// section 5, source 2). The refusal is about the dial, not about the wallet's
/// address book.
#[tokio::test]
async fn a_fetch_from_an_endpoint_this_wallet_knows_nothing_about_is_still_asked() {
    bounded!({
        // No configured witness, so the caller's endpoint is the only source
        // and the refusal names it.
        let wallet = Wallet::plain().await;
        let stranger = rendered(endpoint(77));
        let error = wallet
            .service
            .fetch_identity(FetchIdentity {
                identity_id: rendered(endpoint(5)),
                from: Some(stranger.clone()),
                from_witness: None,
            })
            .await
            .expect_err("nothing answers at that endpoint");
        assert_eq!(error.reason(), "witness_unreachable");
        assert_eq!(error.code(), 30);
        assert_eq!(
            error
                .details()
                .get("endpoint_id")
                .and_then(|id| id.as_str()),
            Some(stranger.as_str())
        );
    });
}

/// `from` names an endpoint and `from_witness` names an identity. Both at once
/// is `conflicting_source`, refused before anything is dialled.
#[tokio::test]
async fn a_fetch_naming_both_a_source_and_a_witness_is_refused() {
    let wallet = Wallet::plain().await;
    let error = wallet
        .service
        .fetch_identity(FetchIdentity {
            identity_id: rendered(endpoint(5)),
            from: Some(rendered(endpoint(77))),
            from_witness: Some(mabel_node::wallet::ids::identity(common::witness_identity())),
        })
        .await
        .expect_err("one source or the other");
    assert_eq!(error.reason(), "conflicting_source");
    assert_eq!(error.code(), 2);
}

/// A `from_witness` this home can reach no endpoint for is `unresolvable_witness`
/// (proposal 006 section 5.1: witness resolution reads what this home holds).
#[tokio::test]
async fn a_fetch_from_a_witness_with_no_known_endpoint_is_refused() {
    let wallet = Wallet::plain().await;
    let witness = mabel_node::wallet::ids::identity(common::witness_identity());
    let error = wallet
        .service
        .fetch_identity(FetchIdentity {
            identity_id: rendered(endpoint(5)),
            from: None,
            from_witness: Some(witness.clone()),
        })
        .await
        .expect_err("no endpoint is known for it");
    assert_eq!(error.reason(), "unresolvable_witness");
    assert_eq!(error.code(), 2);
    assert_eq!(
        error.details().get("witness").and_then(|id| id.as_str()),
        Some(witness.as_str())
    );
    assert_eq!(
        error.details().get("endpoints_tried"),
        Some(&serde_json::json!([])),
        "nothing was dialled: no machine is known for it"
    );
}

#[tokio::test]
async fn a_fetch_with_no_witness_to_ask_says_so_before_dialling() {
    let wallet = Wallet::plain().await;
    let error = wallet
        .service
        .fetch_identity(FetchIdentity {
            from_witness: None,
            identity_id: rendered(endpoint(5)),
            from: None,
        })
        .await
        .expect_err("no source is configured");
    assert_eq!(error.reason(), "no_witness_configured");
    assert_eq!(error.code(), 2);
}

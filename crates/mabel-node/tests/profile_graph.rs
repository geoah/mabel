//! The profile, verification, contact, lookup and graph routes against a real
//! home (ticket 026, proposal 003).
//!
//! Every test here runs [`WalletApiService`] over a temp node home with a stub
//! resolver and a stub fetcher, so nothing opens a socket and nothing queries
//! DNS. The Iroh endpoint the service holds is bound with relays disabled and
//! is never dialled: the crawl reads the stub, and the hostname check reads the
//! stub.

use std::sync::Arc;

use mabel_core::IdentityId;
use mabel_node::api::documents::{DeclaredKind, Id, Provenance, VerificationStatus};
use mabel_node::api::service::{LookupRequest, ReplaceProfile, SetContact, WalletService};
use mabel_node::graph::{StubFetcher, stub_identity};
use mabel_node::verification::{StubResolver, VerificationStore};
use mabel_node::wallet::{WalletApiService, WalletCore, WalletSync};
use mabel_node::{HomeOptions, NodeConfig, NodeHome, NodeRole, RelayMode};
use tempfile::TempDir;

/// A wallet home, the core over it and the HTTP service over that.
///
/// The service can be rebuilt on the same home, which is what a test that has
/// to mint an identity before it can write the matching TXT record needs.
struct Wallet {
    _dir: TempDir,
    core: Arc<WalletCore>,
    service: WalletApiService,
    /// The resolver the service holds, so a test can assert that a route
    /// queried nothing.
    resolver: Arc<StubResolver>,
}

impl Wallet {
    /// A fresh home with no DNS answers and an empty crawl table.
    async fn plain() -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let config = NodeConfig {
            role: NodeRole::Wallet,
            relay: RelayMode::Disabled,
            ..NodeConfig::default()
        };
        let home = NodeHome::create(dir.path(), &config, HomeOptions::default())
            .expect("the home is created");
        let core = Arc::new(WalletCore::new(home));
        let resolver = Arc::new(StubResolver::new());
        let service = service(&core, resolver.clone(), StubFetcher::new()).await;
        Self {
            _dir: dir,
            core,
            service,
            resolver,
        }
    }

    /// Replaces the service with one over the same home, answering from these
    /// stubs instead.
    async fn rewire(&mut self, resolver: StubResolver, fetcher: StubFetcher) {
        self.resolver = Arc::new(resolver);
        self.service = service(&self.core, self.resolver.clone(), fetcher).await;
    }

    fn identity(&self, alias: &str) -> IdentityId {
        let created = self
            .core
            .create_identity(alias, DeclaredKind::Person, None)
            .expect("the identity is created");
        created
            .identity
            .identity_id
            .as_str()
            .parse()
            .expect("a rendered identity id parses")
    }
}

/// A service over `core`, on its own endpoint with relays disabled.
///
/// The endpoint is bound and never dialled: the crawl reads the stub fetcher
/// and the hostname check reads the stub resolver.
async fn service(
    core: &Arc<WalletCore>,
    resolver: Arc<StubResolver>,
    fetcher: StubFetcher,
) -> WalletApiService {
    let secret = core.home().node_key().expect("the node key reads");
    let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, &[])
        .await
        .expect("the endpoint binds");
    WalletApiService::new(
        core.clone(),
        WalletSync::new(endpoint),
        "127.0.0.1:9080".parse().expect("a bind address"),
        RelayMode::Disabled,
    )
    .with_resolver(resolver)
    .with_fetcher(Arc::new(fetcher))
}

fn id(identity: IdentityId) -> Id {
    Id::parse(&identity.to_string()).expect("a rendered id")
}

/// `mabel=<identity id>` at `_mabel.<hostname>.`, the record proposal 003
/// section 2 makes normative.
fn resolver_for(hostname: &str, identity: IdentityId) -> StubResolver {
    StubResolver::new().with_text(&format!("_mabel.{hostname}."), &format!("mabel={identity}"))
}

// ---------------------------------------------------------------- profile ----

#[tokio::test]
async fn a_replaced_profile_shows_up_in_the_identity_document() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");

    let before = wallet
        .service
        .identity(id(alice))
        .await
        .expect("a document");
    assert_eq!(before.profile, None);
    assert_eq!(before.verification.status, VerificationStatus::Unclaimed);
    assert_eq!(before.contact, None);

    let replaced = wallet
        .service
        .replace_profile(ReplaceProfile {
            identity_id: id(alice),
            display_name: Some("Alice Ashworth".to_owned()),
            hostname: Some("alice.example".to_owned()),
        })
        .await
        .expect("the profile lands");
    assert_eq!(
        replaced.profile.display_name.as_deref(),
        Some("Alice Ashworth")
    );
    assert_eq!(replaced.previous.display_name, None);
    assert_eq!(replaced.event.payload_kind, "profile_update");

    let after = wallet
        .service
        .identity(id(alice))
        .await
        .expect("a document");
    let profile = after.profile.expect("the fold recorded it");
    assert_eq!(profile.display_name.as_deref(), Some("Alice Ashworth"));
    assert_eq!(profile.hostname.as_deref(), Some("alice.example"));
    assert_eq!(profile.seq, replaced.profile.seq);
    // Any current controller may rename a ledger, so the profile records who
    // signed it (proposal 003 section 1).
    assert_eq!(profile.signing_principal.identity, id(alice));
    // Never checked, so the verdict says so with a null timestamp rather than
    // claiming a lookup that did not happen.
    assert_eq!(after.verification.status, VerificationStatus::Unverified);
    assert_eq!(after.verification.checked_at_ms, None);
    assert!(after.verification.stale);
}

#[tokio::test]
async fn a_replacement_clears_the_field_it_omits() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let set = |display_name: Option<&str>, hostname: Option<&str>| ReplaceProfile {
        identity_id: id(alice),
        display_name: display_name.map(ToOwned::to_owned),
        hostname: hostname.map(ToOwned::to_owned),
    };

    wallet
        .service
        .replace_profile(set(Some("Alice Ashworth"), Some("alice.example")))
        .await
        .expect("the first profile lands");
    let cleared = wallet
        .service
        .replace_profile(set(None, Some("alice.example")))
        .await
        .expect("the second profile lands");

    assert_eq!(cleared.profile.display_name, None);
    assert_eq!(cleared.profile.hostname.as_deref(), Some("alice.example"));
    assert_eq!(
        cleared.previous.display_name.as_deref(),
        Some("Alice Ashworth"),
        "the answer names what it replaced, which is what the CLI diff prints"
    );
}

#[tokio::test]
async fn a_replacement_that_changes_nothing_is_refused_before_signing() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let request = || ReplaceProfile {
        identity_id: id(alice),
        display_name: Some("Alice Ashworth".to_owned()),
        hostname: None,
    };

    let landed = wallet
        .service
        .replace_profile(request())
        .await
        .expect("the profile lands");
    let error = wallet
        .service
        .replace_profile(request())
        .await
        .expect_err("the same profile is refused");

    assert_eq!(error.code(), 20);
    assert_eq!(error.reason(), "no_op_profile_update");
    assert!(error.message().starts_with("Policy error: "), "{error:?}");
    assert_eq!(
        error.details()["profile_seq"],
        serde_json::json!(landed.profile.seq)
    );

    // Nothing was appended: the ledger still ends at the update that changed
    // something.
    let after = wallet
        .service
        .identity(id(alice))
        .await
        .expect("a document");
    assert_eq!(after.head_seq, landed.head_seq);
}

#[tokio::test]
async fn clearing_a_profile_that_was_never_set_is_refused_too() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let error = wallet
        .service
        .replace_profile(ReplaceProfile {
            identity_id: id(alice),
            display_name: None,
            hostname: None,
        })
        .await
        .expect_err("clearing nothing changes nothing");
    assert_eq!(error.reason(), "no_op_profile_update");
}

// ----------------------------------------------------------- verification ----

#[tokio::test]
async fn a_forced_check_writes_the_cache_and_the_document_reads_it_back() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    wallet
        .service
        .replace_profile(ReplaceProfile {
            identity_id: id(alice),
            display_name: None,
            hostname: Some("alice.example".to_owned()),
        })
        .await
        .expect("the hostname claim lands");

    let checked = wallet
        .service
        .check_verification(id(alice))
        .await
        .expect("the check runs");
    // The default stub answers every name with no records, which is a checked
    // verdict, not a failure.
    assert_eq!(checked.verification.status, VerificationStatus::Unverified);
    assert!(checked.verification.checked_at_ms.is_some());
    assert!(!checked.verification.stale);

    let entry = VerificationStore::new(wallet.core.home())
        .read(alice)
        .expect("the cache reads")
        .expect("the check wrote a file");
    assert_eq!(entry.hostname, "alice.example");

    let document = wallet
        .service
        .identity(id(alice))
        .await
        .expect("a document");
    assert_eq!(document.verification.status, VerificationStatus::Unverified);
    assert_eq!(
        document.verification.checked_at_ms,
        checked.verification.checked_at_ms
    );
}

#[tokio::test]
async fn a_matching_txt_record_verifies_and_a_renamed_claim_drops_the_verdict() {
    let mut wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    // The record names the id, so it can only be written once the home has
    // minted the identity.
    wallet
        .rewire(resolver_for("alice.example", alice), StubFetcher::new())
        .await;

    wallet
        .service
        .replace_profile(ReplaceProfile {
            identity_id: id(alice),
            display_name: None,
            hostname: Some("alice.example".to_owned()),
        })
        .await
        .expect("the claim lands");
    let checked = wallet
        .service
        .check_verification(id(alice))
        .await
        .expect("the check runs");
    assert_eq!(checked.verification.status, VerificationStatus::Verified);
    assert!(checked.verification.last_verified_at_ms.is_some());

    // The entry is bound to the hostname it verified, so renaming the claim
    // makes it absent rather than inheriting the verdict.
    wallet
        .service
        .replace_profile(ReplaceProfile {
            identity_id: id(alice),
            display_name: None,
            hostname: Some("carol.example".to_owned()),
        })
        .await
        .expect("the rename lands");
    let document = wallet
        .service
        .identity(id(alice))
        .await
        .expect("a document");
    assert_eq!(
        document.verification.hostname.as_deref(),
        Some("carol.example")
    );
    assert_eq!(document.verification.status, VerificationStatus::Unverified);
    assert_eq!(document.verification.checked_at_ms, None);
}

#[tokio::test]
async fn a_forced_check_on_an_identity_claiming_no_hostname_is_refused() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let error = wallet
        .service
        .check_verification(id(alice))
        .await
        .expect_err("there is nothing to check");
    assert_eq!(error.code(), 20);
    assert_eq!(error.reason(), "no_hostname_claimed");
}

#[tokio::test]
async fn listing_identities_never_queries_the_resolver() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    wallet
        .service
        .replace_profile(ReplaceProfile {
            identity_id: id(alice),
            display_name: None,
            hostname: Some("alice.example".to_owned()),
        })
        .await
        .expect("the claim lands");

    let identities = wallet.service.identities().await.expect("the list answers");
    assert_eq!(identities.len(), 1);
    assert_eq!(
        identities[0].verification.status,
        VerificationStatus::Unverified
    );
    assert_eq!(identities[0].verification.checked_at_ms, None);
    assert!(
        wallet.resolver.queries().is_empty(),
        "the list route is cache-only: no row may trigger a lookup"
    );
}

// --------------------------------------------------------------- contacts ----

#[tokio::test]
async fn a_contact_round_trips_and_covers_a_foreign_identity() {
    let wallet = Wallet::plain().await;
    let stranger = stub_identity(7);

    let empty = wallet
        .service
        .contact(id(stranger))
        .await
        .expect("an answer");
    assert_eq!(empty.contact, None);

    let written = wallet
        .service
        .set_contact(SetContact {
            identity_id: id(stranger),
            nickname: Some("bob at the print shop".to_owned()),
            note: Some("met at the zine fair".to_owned()),
        })
        .await
        .expect("the note is written");
    let contact = written.contact.expect("both fields are set");
    assert_eq!(contact.nickname.as_deref(), Some("bob at the print shop"));

    let read = wallet
        .service
        .contact(id(stranger))
        .await
        .expect("an answer")
        .contact
        .expect("the file is there");
    assert_eq!(read, contact);

    // The note is not part of `IdentityMeta`, which describes identities this
    // home controls: a stranger has no identity directory at all.
    assert!(!wallet.core.home().identity_dir(stranger).is_dir());
    assert!(
        wallet
            .core
            .home()
            .contacts_dir()
            .join(format!("{stranger}.json"))
            .is_file()
    );

    let cleared = wallet
        .service
        .set_contact(SetContact {
            identity_id: id(stranger),
            nickname: None,
            note: None,
        })
        .await
        .expect("the note is cleared");
    assert_eq!(cleared.contact, None);
}

#[tokio::test]
async fn a_contact_note_lands_on_the_identity_document() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    wallet
        .service
        .set_contact(SetContact {
            identity_id: id(alice),
            nickname: Some("me".to_owned()),
            note: None,
        })
        .await
        .expect("the note is written");

    let document = wallet
        .service
        .identity(id(alice))
        .await
        .expect("a document");
    let contact = document.contact.expect("the document carries it");
    assert_eq!(contact.nickname.as_deref(), Some("me"));
    assert_eq!(contact.note, None);
}

// ----------------------------------------------------------- graph, lookup ----

#[tokio::test]
async fn a_lookup_before_any_crawl_answers_null_degrees_rather_than_a_404() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let stranger = stub_identity(4);

    let lookup = wallet
        .service
        .lookup(LookupRequest {
            identity_id: id(stranger),
            from: Some(id(alice)),
        })
        .await
        .expect("not in my crawl is an answer");

    assert_eq!(lookup.degrees, None);
    assert!(lookup.paths.is_empty());
    assert!(lookup.trust.is_empty());
    assert!(lookup.reverse.best_effort);
    assert!(lookup.reverse.entries.is_empty());
    assert!(lookup.graph_stale);
    assert!(!lookup.graph_truncated);
    assert_eq!(lookup.truncated_by, None);
    assert_eq!(lookup.sync_id, None);
    assert_eq!(lookup.identity.provenance, Provenance::None);
    assert_eq!(lookup.from.provenance, Provenance::Alias);
    assert_eq!(lookup.from.alias.as_deref(), Some("alice"));
}

#[tokio::test]
async fn a_crawl_answers_a_two_hop_lookup_with_its_honesty_fields() {
    let bob = stub_identity(2);
    let carol = stub_identity(3);
    let mut wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");

    // Alice trusts Bob, Bob trusts Carol: two degrees over the stub table.
    wallet
        .rewire(
            StubResolver::new(),
            StubFetcher::new()
                .trusting(alice, &[bob])
                .trusting(bob, &[carol])
                .trusting(carol, &[]),
        )
        .await;

    let synced = wallet.service.sync_graph().await.expect("the crawl runs");
    assert_eq!(synced.graph.roots.len(), 1);
    assert_eq!(synced.graph.roots[0].identity_id, id(alice));
    assert_eq!(synced.graph.node_count, 3);
    assert_eq!(synced.graph.edge_count, 2);
    assert!(!synced.graph.stale);

    let graph = wallet.service.graph().await.expect("the pointer resolves");
    let live = graph.graph.expect("a crawl has run");
    assert_eq!(live.sync_id, synced.graph.sync_id);

    let lookup = wallet
        .service
        .lookup(LookupRequest {
            identity_id: id(carol),
            from: Some(id(alice)),
        })
        .await
        .expect("a path");
    assert_eq!(lookup.degrees, Some(2));
    assert_eq!(lookup.paths.len(), 1);
    let hops = &lookup.paths[0].hops;
    assert_eq!(hops.len(), 2);
    assert_eq!(hops[0].from.identity_id, id(alice));
    assert_eq!(hops[0].to.identity_id, id(bob));
    assert_eq!(hops[1].to.identity_id, id(carol));
    // Every hop says when the node it reaches was read, and whether that read
    // has aged out.
    assert!(hops[0].fetched_at_ms.is_some());
    assert!(hops[1].fetched_at_ms.is_some());
    assert_eq!(lookup.sync_id, Some(live.sync_id));
    // Who trusts Carol is who this crawl read, and says so.
    assert!(lookup.reverse.best_effort);
    assert_eq!(lookup.reverse.entries.len(), 1);
    assert_eq!(lookup.reverse.entries[0].identity.identity_id, id(bob));
}

#[tokio::test]
async fn a_lookup_defaults_from_to_the_lowest_local_identity() {
    let wallet = Wallet::plain().await;
    let first = wallet.identity("one");
    let second = wallet.identity("two");
    let lowest = first.min(second);

    let lookup = wallet
        .service
        .lookup(LookupRequest {
            identity_id: id(second),
            from: None,
        })
        .await
        .expect("an answer");
    assert_eq!(lookup.from.identity_id, id(lowest));
}

#[tokio::test]
async fn a_from_that_is_not_in_this_home_is_refused() {
    let wallet = Wallet::plain().await;
    let alice = wallet.identity("alice");
    let error = wallet
        .service
        .lookup(LookupRequest {
            identity_id: id(alice),
            from: Some(id(stub_identity(8))),
        })
        .await
        .expect_err("a root must be local");
    assert_eq!(error.code(), 2);
    assert_eq!(error.reason(), "unknown_from_identity");
}

#[tokio::test]
async fn a_home_with_no_identity_cannot_be_crawled() {
    let wallet = Wallet::plain().await;
    let error = wallet
        .service
        .sync_graph()
        .await
        .expect_err("there is nothing to crawl from");
    assert_eq!(error.code(), 2);
    assert_eq!(error.reason(), "no_local_identity");

    let graph = wallet.service.graph().await.expect("an answer");
    assert_eq!(graph.graph, None);
}

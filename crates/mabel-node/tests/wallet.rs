//! The wallet runtime against real witnesses on loopback (ticket 011).
//!
//! Every test here runs two Iroh endpoints in one process with relays
//! disabled, so nothing touches DNS, a relay or the internet (proposal 001
//! section 11). A wallet dials by `EndpointId` and the witness's loopback
//! address is seeded into the lookup first, which is what a `--peer` ticket
//! does on the command line.

#[macro_use]
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Chain, Served, TIMEOUT, secret, subject, witness_identity};
use iroh_base::EndpointId;
use mabel_core::sign::{Position, build_trust_attestation};
use mabel_core::{EventId, IdentityId};
use mabel_net::store::Provenance;
use mabel_net::{Client, EndpointConfig, RelayChoice, bind_endpoint};
use mabel_node::StorageCaps;
use mabel_node::api::documents::{Appended, DeclaredKind, PushStatus, Pushed, SubjectResolution};
use mabel_node::wallet::{Freshness, Sources, Verifier, WalletCore, WalletSync};
use mabel_node::{HomeOptions, NodeConfig, NodeHome, RelayMode};
use tempfile::TempDir;

/// A wallet home in a temp directory, with the core over it.
struct Wallet {
    _dir: TempDir,
    core: Arc<WalletCore>,
}

impl Wallet {
    /// A fresh wallet home with relays disabled.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let config = NodeConfig {
            relay: RelayMode::Disabled,
            ..NodeConfig::default()
        };
        let home = NodeHome::create(dir.path(), &config, HomeOptions::default())
            .expect("the home is created");
        Self {
            _dir: dir,
            core: Arc::new(WalletCore::new(home)),
        }
    }

    /// Mints an identity and returns its id.
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

    /// Names the witness identity every `Served` home here witnesses for, and
    /// records `endpoints` in `node.json` as where that witness answers.
    ///
    /// Two facts, one call, because a push needs both: the chain says who may
    /// keep the ledger, which is what admits the push (proposal 006 section 4),
    /// and `node.json` says which machine to dial, which is the bootstrap raw
    /// endpoint of section 5.4, which resolution reads under section 5.1.
    async fn witnesses(&self, identity: IdentityId, endpoints: &[EndpointId]) {
        {
            let lock = self.core.append_lock(identity).await;
            self.core
                .set_witnesses(&lock, identity, &[witness_identity()])
                .expect("the witness set is appended");
        }
        let mut config = self.core.home().config().expect("node.json reads");
        config.witnesses = vec![mabel_node::WitnessEntry::new(
            witness_identity(),
            endpoints.to_vec(),
        )];
        self.core
            .home()
            .write_config(&config)
            .expect("node.json is written");
    }

    /// Appends one attestation, holding the ledger's append lock over it.
    async fn add_trust(&self, issuer: IdentityId, subject: IdentityId) -> Appended {
        let lock = self.core.append_lock(issuer).await;
        self.core
            .add_trust(&lock, issuer, subject)
            .expect("the attestation lands")
    }

    /// A sync client on its own endpoint, with every address in `peers`
    /// seeded into the lookup.
    ///
    /// Three seconds is the deadline for a peer that is expected to be
    /// unreachable; a test whose answer depends on every peer replying passes
    /// a longer one to [`Wallet::sync_with`], so a slow loopback dial fails
    /// the test instead of quietly dropping a source.
    async fn sync(&self, peers: &[iroh::EndpointAddr]) -> WalletSync {
        self.sync_with(peers, Duration::from_secs(3)).await
    }

    /// A sync client whose per-request deadline is `timeout`.
    async fn sync_with(&self, peers: &[iroh::EndpointAddr], timeout: Duration) -> WalletSync {
        let secret = self.core.home().node_key().expect("the node key reads");
        let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, peers)
            .await
            .expect("the endpoint binds");
        WalletSync::new(endpoint).with_timeout(timeout)
    }

    /// Signs one attestation at the ledger's next position without storing it.
    fn attestation_at_head(&self, identity: IdentityId, seed: u8) -> (EventId, Vec<u8>) {
        let loaded = self.core.load(identity).expect("the ledger loads");
        let head = loaded.state.head().expect("a head");
        let signer = self.core.signing_key(identity).expect("the key reads");
        let built = build_trust_attestation(
            &signer,
            &Position {
                ledger: identity,
                seq: head.seq + 1,
                prev: head.event_id,
                prev_timestamp_ms: head.timestamp_ms,
            },
            subject(seed),
            head.timestamp_ms + 1_000 + u64::from(seed),
        )
        .expect("the attestation builds");
        (built.event_id, built.signed_event)
    }

    /// Every stored event of one ledger.
    fn events(&self, identity: IdentityId) -> Vec<Vec<u8>> {
        self.core
            .load(identity)
            .map(|loaded| loaded.events)
            .unwrap_or_default()
    }
}

/// Asserts every witness in a push report took the events.
///
/// `push` reports an unreachable or refusing witness as a row rather than an
/// error, so a test that goes on to read those events back says so here.
fn accepted(pushed: Pushed) {
    for result in &pushed.results {
        assert_eq!(
            result.status,
            PushStatus::Accepted,
            "{} did not take the push: {result:?}",
            result.endpoint
        );
    }
}

/// Where one witness says a ledger ends, failing the test if it does not
/// answer or does not hold it.
async fn head_seq(sync: &WalletSync, witness: EndpointId, ledger: IdentityId) -> u64 {
    sync.head(witness, ledger)
        .await
        .expect("the witness answers")
        .expect("the witness holds the ledger")
        .head_seq
}

/// An endpoint id nothing in the test binds, so dialling it always fails.
fn nowhere() -> EndpointId {
    secret(199).public()
}

/// Pushes `events` for `ledger` straight at one witness, bypassing the wallet.
///
/// This is how a test makes a witness hold something the wallet does not: it
/// stands in for the second controller of a shared ledger.
async fn push_directly(witness: &Served, ledger: IdentityId, events: &[Vec<u8>]) {
    let endpoint = bind_endpoint(EndpointConfig::new(RelayChoice::Disabled))
        .await
        .expect("the endpoint binds")
        .endpoint;
    let client = Client::connect(&endpoint, witness.addr.clone())
        .await
        .expect("the client connects");
    client.push(ledger, events).await.expect("the push lands");
    client.close();
    endpoint.close().await;
}

#[tokio::test]
async fn a_push_lands_on_every_configured_witness() {
    bounded!({
        let first = Served::new().await;
        let second = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet
            .witnesses(alice, &[first.endpoint_id, second.endpoint_id])
            .await;
        let sync = wallet
            .sync(&[first.addr.clone(), second.addr.clone()])
            .await;

        let pushed = sync
            .push(
                &wallet.core,
                alice,
                &[first.endpoint_id, second.endpoint_id],
            )
            .await
            .expect("the push reports");

        assert_eq!(pushed.head_seq, 1, "inception plus the witness config");
        assert_eq!(pushed.results.len(), 2);
        for result in &pushed.results {
            assert_eq!(result.status, PushStatus::Accepted, "{result:?}");
            assert_eq!(result.stored, 2);
            assert_eq!(result.head_seq, Some(1));
            assert_eq!(result.message, None);
        }
        for witness in [&first, &second] {
            let head = witness.storage.head(alice).expect("the witness holds it");
            assert_eq!(head.head_seq, 1);
        }

        // A second push of the same events is idempotent: nothing new stored.
        let again = sync
            .push(&wallet.core, alice, &[first.endpoint_id])
            .await
            .expect("the push reports");
        assert_eq!(again.results[0].status, PushStatus::Accepted);
        assert_eq!(again.results[0].stored, 0);

        first.stop().await;
        second.stop().await;
    });
}

#[tokio::test]
async fn a_push_writes_the_witnesses_that_took_it_into_peers_json() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet
            .witnesses(alice, &[witness.endpoint_id, nowhere()])
            .await;
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        assert!(
            wallet
                .core
                .home()
                .peers()
                .expect("peers.json reads")
                .ledgers
                .is_empty(),
            "a fresh home records no hint"
        );

        sync.push(&wallet.core, alice, &[witness.endpoint_id, nowhere()])
            .await
            .expect("a partial failure is still a report");

        // Only the endpoint that accepted the push is a hint: an unreachable
        // one is no evidence that it holds the ledger.
        let peers = wallet.core.home().peers().expect("peers.json reads");
        assert_eq!(peers.hints(alice), [witness.endpoint_id]);

        // A repeat leaves one entry, and no other ledger gains a hint.
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push reports");
        let peers = wallet.core.home().peers().expect("peers.json reads");
        assert_eq!(peers.hints(alice), [witness.endpoint_id]);
        assert_eq!(peers.ledgers.len(), 1);

        witness.stop().await;
    });
}

#[tokio::test]
async fn a_witness_that_cannot_be_reached_is_one_row_of_the_push_report() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet
            .witnesses(alice, &[witness.endpoint_id, nowhere()])
            .await;
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;

        let pushed = sync
            .push(&wallet.core, alice, &[witness.endpoint_id, nowhere()])
            .await
            .expect("a partial failure is still a report");

        assert_eq!(pushed.results[0].status, PushStatus::Accepted);
        assert_eq!(pushed.results[1].status, PushStatus::Unreachable);
        assert_eq!(pushed.results[1].head_seq, None);
        assert_eq!(pushed.results[1].stored, 0);
        let message = pushed.results[1]
            .message
            .as_deref()
            .expect("an unreachable row says why");
        assert!(
            message.starts_with("Network error: no route to"),
            "{message}"
        );
        assert!(
            message.contains(pushed.results[1].endpoint.as_str()),
            "{message}"
        );
        assert_eq!(
            witness
                .storage
                .head(alice)
                .expect("the reachable one holds it")
                .head_seq,
            1,
            "the reachable witness still stored the ledger"
        );

        witness.stop().await;
    });
}

#[tokio::test]
async fn a_fetch_verifies_from_nothing_before_it_stores() {
    bounded!({
        let witness = Served::new().await;
        let publisher = Wallet::new();
        let alice = publisher.identity("alice");
        publisher.witnesses(alice, &[witness.endpoint_id]).await;
        publisher
            .sync(std::slice::from_ref(&witness.addr))
            .await
            .push(&publisher.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push lands");

        let reader = Wallet::new();
        let sync = reader.sync(std::slice::from_ref(&witness.addr)).await;
        let fetched = sync
            .fetch(&reader.core, alice, witness.endpoint_id)
            .await
            .expect("the fetch verifies and stores");

        assert_eq!(fetched.event_count, 2);
        assert_eq!(fetched.stored, 2);
        assert_eq!(fetched.head_seq, 1);
        assert_eq!(fetched.source, witness.endpoint_id);
        assert_eq!(
            reader.events(alice),
            publisher.events(alice),
            "the stored bytes are the bytes that were signed"
        );
        assert_eq!(
            reader
                .core
                .store(alice)
                .meta()
                .expect("provenance reads")
                .expect("provenance was written")
                .source_endpoint,
            Some(witness.endpoint_id),
            "the fetch records where the events came from"
        );

        // A second fetch stores nothing and does not fail.
        let again = sync
            .fetch(&reader.core, alice, witness.endpoint_id)
            .await
            .expect("the fetch is idempotent");
        assert_eq!(again.stored, 0);
        assert_eq!(again.head_seq, 1);

        witness.stop().await;
    });
}

#[tokio::test]
async fn a_witness_ahead_on_an_extending_chain_is_fast_forwarded() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet.witnesses(alice, &[witness.endpoint_id]).await;
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push lands");

        // Another holder of the key appends at seq 2 and pushes it; this home
        // never saw the event.
        let (ahead, bytes) = wallet.attestation_at_head(alice, 7);
        let mut chain = wallet.events(alice);
        chain.push(bytes);
        push_directly(&witness, alice, &chain).await;
        assert_eq!(wallet.core.load(alice).unwrap().head_seq, 1);

        let freshness = sync
            .ensure_fresh(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("an extending chain is not a conflict");

        assert_eq!(freshness, Freshness::FastForwarded { head_seq: 2 });
        let loaded = wallet.core.load(alice).expect("the ledger loads");
        assert_eq!(loaded.head_seq, 2);
        assert_eq!(loaded.head_event, ahead);
        assert!(loaded.violation.is_none(), "the fetched chain verifies");

        // Nothing left to do the second time.
        assert_eq!(
            sync.ensure_fresh(&wallet.core, alice, &[witness.endpoint_id])
                .await
                .expect("still fresh"),
            Freshness::UpToDate
        );

        witness.stop().await;
    });
}

#[tokio::test]
async fn a_local_event_that_lost_a_race_exits_50_and_leaves_no_stale_event() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet.witnesses(alice, &[witness.endpoint_id]).await;
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push lands");

        // The other controller wins seq 2 at the witness.
        let (theirs, bytes) = wallet.attestation_at_head(alice, 7);
        let mut chain = wallet.events(alice);
        chain.push(bytes);
        push_directly(&witness, alice, &chain).await;

        // This home appends its own seq 2, unpushed, naming a different
        // subject: the intent it wants to land.
        let mine = wallet.add_trust(alice, subject(8)).await;
        assert_eq!(mine.head_seq, 2);
        let stale = mine.head_event.clone();

        let error = sync
            .ensure_fresh(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect_err("the local event lost the race");

        assert_eq!(error.code(), 50);
        assert_eq!(error.reason(), "stale_head");
        assert!(
            error.message().starts_with("State error: witness "),
            "{error}"
        );
        let document = error.to_document();
        assert_eq!(document["details"]["local_head_seq"], 2);
        assert_eq!(document["details"]["observed_head_seq"], 2);
        assert_eq!(document["details"]["ledger_id"], alice.to_string());

        // No stale event is left in the home: the witness's chain is what is
        // stored now.
        let loaded = wallet.core.load(alice).expect("the ledger loads");
        assert_eq!(loaded.head_seq, 2);
        assert_eq!(loaded.head_event, theirs);
        assert_ne!(loaded.head_event.to_string(), stale.to_string());
        assert!(loaded.violation.is_none());
        assert_eq!(loaded.events.len(), 3, "nothing past the new head survives");

        // The retry re-signs the same intent on the new head and succeeds.
        assert_eq!(
            sync.ensure_fresh(&wallet.core, alice, &[witness.endpoint_id])
                .await
                .expect("fresh now"),
            Freshness::UpToDate
        );
        let retried = wallet.add_trust(alice, subject(8)).await;
        assert_eq!(retried.head_seq, 3);

        witness.stop().await;
    });
}

#[tokio::test]
async fn two_witnesses_on_divergent_branches_report_equivocation() {
    bounded!({
        let first = Served::new().await;
        let second = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet
            .witnesses(alice, &[first.endpoint_id, second.endpoint_id])
            .await;
        let sync = wallet
            .sync(&[first.addr.clone(), second.addr.clone()])
            .await;
        sync.push(
            &wallet.core,
            alice,
            &[first.endpoint_id, second.endpoint_id],
        )
        .await
        .expect("the push lands");

        // Two events at seq 2, one per witness: a lost race, or a signer that
        // equivocated. The verifier cannot tell and does not guess.
        let shared = wallet.events(alice);
        let (left, left_bytes) = wallet.attestation_at_head(alice, 7);
        let (right, right_bytes) = wallet.attestation_at_head(alice, 8);
        assert_ne!(left, right);
        let mut left_chain = shared.clone();
        left_chain.push(left_bytes);
        let mut right_chain = shared;
        right_chain.push(right_bytes);
        push_directly(&first, alice, &left_chain).await;
        push_directly(&second, alice, &right_chain).await;

        let error = Verifier::new(&wallet.core, Some(&sync))
            .ledger_report(alice, None)
            .await
            .expect_err("two valid branches are equivocation");

        assert_eq!(error.code(), 20);
        assert_eq!(error.reason(), "equivocation");
        assert_eq!(
            error.message(),
            format!("Ledger error: two sources hold divergent events at seq 2 of {alice}")
        );
        let document = error.to_document();
        assert_eq!(document["details"]["at_seq"], 2);
        let candidates = document["details"]["candidates"]
            .as_array()
            .expect("two candidates")
            .clone();
        assert_eq!(candidates.len(), 2);
        let sources: Vec<String> = candidates
            .iter()
            .map(|entry| entry["source"].as_str().expect("a source").to_owned())
            .collect();
        let events: Vec<String> = candidates
            .iter()
            .map(|entry| entry["event_id"].as_str().expect("an event").to_owned())
            .collect();
        for witness in [first.endpoint_id, second.endpoint_id] {
            assert!(
                sources.contains(&rendered(&witness)),
                "{sources:?} names both sources"
            );
        }
        for event in [left, right] {
            assert!(
                events.contains(&rendered_event(event)),
                "{events:?} names both events"
            );
        }

        first.stop().await;
        second.stop().await;
    });
}

#[tokio::test]
async fn a_source_holding_a_strict_prefix_loses_without_an_equivocation_report() {
    bounded!({
        let ahead = Served::new().await;
        let behind = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet
            .witnesses(alice, &[ahead.endpoint_id, behind.endpoint_id])
            .await;
        // Both witnesses must answer for the comparison to mean anything, so
        // the deadline is long enough that a slow loopback dial fails this
        // test rather than dropping a source and leaving the answer to
        // whichever one replied.
        let sync = wallet
            .sync_with(&[ahead.addr.clone(), behind.addr.clone()], TIMEOUT / 2)
            .await;
        accepted(
            sync.push(
                &wallet.core,
                alice,
                &[ahead.endpoint_id, behind.endpoint_id],
            )
            .await
            .expect("both hold the prefix"),
        );

        // Only the first witness gets the third event.
        wallet.add_trust(alice, subject(9)).await;
        accepted(
            sync.push(&wallet.core, alice, &[ahead.endpoint_id])
                .await
                .expect("the longer chain lands on one witness"),
        );

        // The premise, read back from the witnesses themselves: one holds
        // three events and the other two.
        assert_eq!(head_seq(&sync, ahead.endpoint_id, alice).await, 2);
        assert_eq!(head_seq(&sync, behind.endpoint_id, alice).await, 1);

        let report = Verifier::new(&wallet.core, Some(&sync))
            .ledger_report(alice, None)
            .await
            .expect("a prefix is not a divergence");

        assert!(report.valid);
        assert_eq!(report.head_seq, 2, "the longer candidate wins");
        assert_eq!(report.source.as_str(), rendered(&ahead.endpoint_id));
        assert_eq!(report.sources_queried.len(), 2);
        assert!(
            report.statement.contains("valid as of seq 2 of"),
            "{}",
            report.statement
        );

        ahead.stop().await;
        behind.stop().await;
    });
}

#[tokio::test]
async fn a_pinned_source_is_the_only_one_asked() {
    bounded!({
        let pinned = Served::new().await;
        let other = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet
            .witnesses(alice, &[pinned.endpoint_id, other.endpoint_id])
            .await;
        let sync = wallet
            .sync(&[pinned.addr.clone(), other.addr.clone()])
            .await;
        sync.push(&wallet.core, alice, &[pinned.endpoint_id])
            .await
            .expect("only the pinned witness holds it");

        let verified = Verifier::new(&wallet.core, Some(&sync))
            .verify(alice, Sources::Pinned(pinned.endpoint_id))
            .await
            .expect("the pinned source answers");

        assert_eq!(verified.sources_queried, vec![pinned.endpoint_id]);
        assert_eq!(verified.candidate.source, pinned.endpoint_id);

        // The witness that holds nothing cannot answer, pinned or not.
        let error = Verifier::new(&wallet.core, Some(&sync))
            .verify(alice, Sources::Pinned(other.endpoint_id))
            .await
            .expect_err("a source that does not hold it answers nothing");
        assert_eq!(error.code(), 30);
        assert_eq!(error.reason(), "no_source_available");

        pinned.stop().await;
        other.stop().await;
    });
}

#[tokio::test]
async fn a_subject_no_source_holds_is_unresolved_and_still_succeeds() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        wallet.witnesses(alice, &[witness.endpoint_id]).await;
        let stranger = subject(42);
        wallet.add_trust(alice, stranger).await;
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push lands");

        let report = Verifier::new(&wallet.core, Some(&sync))
            .trust_report(alice, stranger, None)
            .await
            .expect("an unresolved subject is not a failure");

        assert!(report.trusted);
        assert_eq!(report.subject_resolution, SubjectResolution::Unresolved);
        assert_eq!(
            report.subject_note.as_deref(),
            Some("subject: unresolved (not held by any queried source)")
        );
        assert_eq!(report.source.as_str(), rendered(&witness.endpoint_id));
        assert!(report.statement.ends_with("; no revocation up to seq 2"));

        // The attestation names who signed it, which is alice's own key here.
        let principal = report
            .signing_principal
            .as_ref()
            .expect("a trusted answer names its signer");
        assert_eq!(principal.identity.as_str(), alice.to_string());
        let expected_key = wallet
            .core
            .signing_key(alice)
            .expect("the key reads")
            .public();
        assert_eq!(principal.key.as_str(), rendered(&expected_key));

        witness.stop().await;
    });
}

/// Holding a directory for the subject is not resolving it: the subject's own
/// ledger must fold to the ledger that was asked for.
#[tokio::test]
async fn a_corrupted_local_subject_ledger_is_unresolved() {
    bounded!({
        let witness = Served::new().await;
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        let bob = wallet.identity("bob");
        wallet.witnesses(alice, &[witness.endpoint_id]).await;
        wallet.add_trust(alice, bob).await;
        let sync = wallet.sync(std::slice::from_ref(&witness.addr)).await;
        sync.push(&wallet.core, alice, &[witness.endpoint_id])
            .await
            .expect("the push lands");
        let verifier = Verifier::new(&wallet.core, Some(&sync));

        let report = verifier
            .trust_report(alice, bob, None)
            .await
            .expect("the issuer verifies");
        assert!(report.trusted);
        assert_eq!(
            report.subject_resolution,
            SubjectResolution::Resolved,
            "bob's own ledger is here and folds"
        );

        // One byte of bob's inception signature, flipped: the home still holds
        // a directory for bob and the fold refuses what is in it. No witness
        // holds bob either, so nothing can resolve him now.
        let path = wallet.core.store(bob).event_path(0);
        let mut bytes = std::fs::read(&path).expect("the inception reads");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        std::fs::write(&path, &bytes).expect("the inception is rewritten");
        assert!(
            wallet
                .core
                .load(bob)
                .expect("the events are still there")
                .violation
                .is_some(),
            "the local copy no longer folds"
        );

        let report = verifier
            .trust_report(alice, bob, None)
            .await
            .expect("an unresolved subject is not a failure");
        assert!(report.trusted, "the attestation itself still stands");
        assert_eq!(report.subject_resolution, SubjectResolution::Unresolved);
        assert!(report.subject_note.is_some());

        witness.stop().await;
    });
}

#[tokio::test]
async fn a_home_serves_the_ledgers_it_signs_for_and_takes_no_stranger_push() {
    bounded!({
        let wallet = Wallet::new();
        let alice = wallet.identity("alice");
        // A stranger's chain this home fetched: stored, and signed for by
        // nobody here.
        let stranger = Chain::new(0x41);
        let store = wallet.core.home().ledger(stranger.ledger);
        let events: Vec<mabel_node::NewEvent<'_>> = stranger
            .events
            .iter()
            .enumerate()
            .map(|(seq, bytes)| mabel_node::NewEvent {
                seq: seq as u64,
                event_id: mabel_net::wire::signed_event_id(bytes).expect("an event has an id"),
                bytes,
            })
            .collect();
        store.append(&events).expect("the fetched chain is written");

        // One store on every node, with this home's own `witness_for`, which is
        // empty: it signs for alice and witnesses for nobody (proposal 006
        // section 8).
        let storage = std::sync::Arc::new(
            mabel_node::LedgerStorage::open_from_config(
                wallet.core.home().clone(),
                wallet
                    .core
                    .home()
                    .node_key()
                    .expect("the node key reads")
                    .public(),
            )
            .expect("the index builds"),
        );
        let store = mabel_node::NodeStore::new(storage);
        let served = mabel_net::store::Store::head(&store, alice)
            .await
            .expect("the store answers")
            .expect("the wallet holds alice");
        assert_eq!(served.head_seq, 0);

        let page = mabel_net::store::Store::read_from(&store, alice, 0, 16)
            .await
            .expect("the store answers")
            .expect("the wallet holds alice");
        assert_eq!(page.events, wallet.events(alice));
        assert!(!page.more);

        // A ledger this home merely fetched is served by `Get` to anyone who
        // can name it, and is never enumerated (proposal 006 section 8).
        let fetched = mabel_net::store::Store::head(&store, stranger.ledger)
            .await
            .expect("the store answers")
            .expect("the fetched ledger is served");
        assert_eq!(fetched.head_seq, stranger.head_seq());

        let listed = mabel_net::store::Store::list(&store, 0, 16)
            .await
            .expect("the store answers");
        assert_eq!(
            listed
                .items
                .iter()
                .map(|row| row.ledger)
                .collect::<Vec<_>>(),
            vec![alice],
            "List names only what this home signs for"
        );

        // A ledger this home neither signs for nor stores: the push is refused
        // in the words of the rule that refused it (proposal 006 section 8).
        let unheld = Chain::new(0x42);
        let refused = mabel_net::store::Store::push(
            &store,
            unheld.ledger,
            unheld.all(),
            Provenance::default(),
        )
        .await
        .expect_err("a home witnessing for nobody stores no stranger's ledger");
        assert!(refused.to_string().contains("NOT_ADMITTED"), "{refused}");
        assert!(
            refused.to_string().contains("witnesses for nobody"),
            "the refusal names the rule that refused it: {refused}"
        );
    });
}

/// A public key as every document spells it.
fn rendered(key: &EndpointId) -> String {
    data_encoding::BASE32_NOPAD
        .encode(key.as_bytes())
        .to_ascii_lowercase()
}

/// An event id as every document spells it.
fn rendered_event(event: EventId) -> String {
    data_encoding::BASE32_NOPAD
        .encode(event.as_bytes())
        .to_ascii_lowercase()
}

/// Keeps the witness caps import honest: the tests run with the section 5
/// caps, which is what a witness enforces in production.
#[test]
fn the_tests_use_the_section_five_caps() {
    assert_eq!(
        StorageCaps::default().storage_capacity,
        mabel_node::DEFAULT_STORAGE_CAPACITY
    );
    assert_eq!(TIMEOUT, Duration::from_secs(10));
}

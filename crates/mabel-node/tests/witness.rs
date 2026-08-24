//! The witness runtime: admission, push semantics, forks and caps (ticket
//! 010, proposal 001 section 5).
//!
//! The push cases run over two in-process Iroh endpoints with relays disabled,
//! so they exercise the same path a wallet takes. The cases that need a shrunk
//! cap or a restart drive [`mabel_node::witness::WitnessStorage`] directly,
//! since neither is about the transport.

#[macro_use]
mod common;

use common::{
    Chain, Home, Served, from_endpoint, home, home_witnessing_for_nobody, secret, subject,
    witness_chain, witness_identity,
};
use mabel_core::proto::RejectCode;
use mabel_net::error::Rejection;
use mabel_net::store::StoreError;
use mabel_net::{Client, EndpointConfig, RelayChoice, bind_endpoint};
use mabel_node::api::UiSource;
use mabel_node::witness::{
    AdmissionPolicy, AdvertisementGap, MAX_FORK_RECORDS, Totals, WitnessCaps, WitnessForEntry,
    WitnessOptions, WitnessRuntime,
};

/// The rejection a storage call answered.
fn rejected(error: StoreError) -> Rejection {
    match error {
        StoreError::Rejected(rejection) => rejection,
        other => panic!("expected a rejection, got {other}"),
    }
}

/// The rejection a client call answered.
fn refused(error: mabel_net::Error) -> Rejection {
    mabel_net::client::rejection_of(&error)
        .cloned()
        .unwrap_or_else(|| panic!("expected a rejection, got {error}"))
}

// ------------------------------------------------------------ admission ----

#[tokio::test]
async fn a_push_of_an_unheld_ledger_naming_this_witness_is_admitted() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(1);
        chain.add_witness();
        let peer = served.dial().await;

        let outcome = peer
            .client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the chain names this witness");
        assert_eq!(outcome.head_seq, 1);
        assert_eq!(outcome.stored, 2);

        // The stored event bytes are the received bytes (section 3.1).
        let page = peer
            .client
            .get(chain.ledger, 0, 0)
            .await
            .expect("the read succeeds")
            .expect("the ledger is stored");
        assert_eq!(page.events, chain.all());
        let store = served.home.home.ledger(chain.ledger);
        assert_eq!(
            std::fs::read(store.event_path(0)).expect("the event file is there"),
            chain.events[0]
        );

        let head = peer
            .client
            .head(chain.ledger)
            .await
            .expect("the read succeeds")
            .expect("the ledger is stored");
        assert_eq!(head.head_seq, 1);
        served.stop().await;
    });
}

#[tokio::test]
async fn a_third_party_may_relay_to_an_already_stored_ledger() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(2);
        chain.add_witness();
        let attestation = chain.attestation(9);
        chain.add(attestation);

        let first = served.dial().await;
        first
            .client
            .push(chain.ledger, &chain.slice(0..2))
            .await
            .expect("the chain names this witness");

        // A second endpoint, which the chain never names, may still relay.
        let relay = served.dial().await;
        assert_ne!(relay.endpoint.id(), first.endpoint.id());
        let outcome = relay
            .client
            .push(chain.ledger, &chain.from(2))
            .await
            .expect("the ledger is already stored");
        assert_eq!(outcome.head_seq, 2);
        assert_eq!(outcome.stored, 1);
        served.stop().await;
    });
}

#[tokio::test]
async fn a_push_for_an_unknown_ledger_not_naming_the_witness_is_not_admitted() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(3);
        // The set names a witness identity this home does not witness for.
        chain.add_witness_set(&[subject(99)]);
        let peer = served.dial().await;

        let error = peer
            .client
            .push(chain.ledger, &chain.all())
            .await
            .expect_err("the chain does not name this witness");
        assert_eq!(refused(error).code, RejectCode::NotAdmitted);
        assert!(
            peer.client
                .head(chain.ledger)
                .await
                .expect("the read succeeds")
                .is_none(),
            "a refused push stores nothing"
        );
        served.stop().await;
    });
}

/// A home whose `witness_for` is empty witnesses for nobody, so it refuses a
/// push for a ledger it holds no key for, whatever that ledger's witness set
/// says (proposal 006 section 4).
#[tokio::test]
async fn a_home_that_witnesses_for_nobody_answers_not_admitted() {
    bounded!({
        let served = Served::over(home_witnessing_for_nobody(), WitnessCaps::default()).await;
        assert!(served.storage.witness_for().is_empty());
        let mut chain = Chain::new(4);
        chain.add_witness();
        let peer = served.dial().await;

        let error = peer
            .client
            .push(chain.ledger, &chain.all())
            .await
            .expect_err("this home witnesses for nobody");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::NotAdmitted);
        assert!(
            rejection.msg.contains("witnesses for nobody"),
            "the refusal names the rule: {}",
            rejection.msg
        );
        assert!(
            peer.client
                .head(chain.ledger)
                .await
                .expect("the read succeeds")
                .is_none(),
            "a refused push stores nothing"
        );
        served.stop().await;
    });
}

/// The same chain lands on a home whose `witness_for` names the witness the
/// chain's `WitnessSet` names, and a later extension lands too, because the
/// stored state still names it (clause 2 of proposal 006 section 4).
#[tokio::test]
async fn a_push_naming_a_witness_this_home_witnesses_for_is_admitted_and_keeps_growing() {
    bounded!({
        let served = Served::new().await;
        assert_eq!(served.storage.witness_for(), [witness_identity()]);
        let mut chain = Chain::new(5);
        chain.add_witness();
        let peer = served.dial().await;

        let outcome = peer
            .client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the witness set names an identity this home witnesses for");
        assert_eq!(outcome.stored, 2);

        chain.add_attestation(9);
        let outcome = peer
            .client
            .push(chain.ledger, &chain.from(2))
            .await
            .expect("the stored witness set still names this home's witness");
        assert_eq!(outcome.head_seq, 2);
        assert_eq!(outcome.stored, 1);
        served.stop().await;
    });
}

/// Clause 2 of proposal 006 section 4 admits the very event that drops this
/// witness, and nothing after it: the prefix stays, and reads stay open.
#[test]
fn the_event_that_drops_this_witness_lands_and_the_next_extension_does_not() {
    let home = home();
    let storage = home.storage(WitnessCaps::default());
    let mut chain = Chain::new(51);
    chain.add_witness();
    storage
        .push(chain.ledger, &chain.all(), from_endpoint(1))
        .expect("the witness set names an identity this home witnesses for");

    // The removal itself: a witness set naming nobody. The stored state still
    // names this home's witness, which is what admits it.
    chain.add_witness_set(&[]);
    let outcome = storage
        .push(chain.ledger, &chain.from(2), from_endpoint(1))
        .expect("clause 2 admits the event that drops this witness");
    assert_eq!(outcome.head_seq, 2);

    // Neither state names an identity this home witnesses for now.
    chain.add_attestation(9);
    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.from(3), from_endpoint(1))
            .expect_err("the witness set names nobody"),
    );
    assert_eq!(rejection.code, RejectCode::NotAdmitted);

    // The prefix is kept and still served: reads stay open to all.
    let page = storage
        .page(chain.ledger, 0, 16)
        .expect("the read succeeds")
        .expect("the prefix is stored");
    assert_eq!(page.events, chain.slice(0..3));
    assert_eq!(page.report.summary.head_seq, 2);
}

/// Clause 3 admits the first push and nothing else does: a chain whose first
/// witness set names nobody is refused outright, and the reason names the rule.
#[test]
fn a_first_push_naming_nobody_is_refused_and_the_reason_names_the_rule() {
    let home = home();
    let storage = home.storage(WitnessCaps::default());
    let mut chain = Chain::new(52);
    chain.add_witness_set(&[]);

    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("no state names an identity this home witnesses for"),
    );
    assert_eq!(rejection.code, RejectCode::NotAdmitted);
    assert!(
        rejection.msg.contains("names none of the 1 identities"),
        "{}",
        rejection.msg
    );
    assert!(storage.head(chain.ledger).is_none());
}

/// Proposal 006 section 4.1: a `witness_for` entry admits a ledger this home
/// does not store only while the latest local copy of that identity advertises
/// this home. A failing entry stops that and nothing else.
#[test]
fn an_entry_whose_identity_stops_advertising_this_home_takes_no_new_ledger() {
    let home = home();
    let storage = home.storage(WitnessCaps::default());
    assert_eq!(
        storage.witness_for_entries(),
        [WitnessForEntry {
            identity: witness_identity(),
            gap: None
        }]
    );

    let mut stored = Chain::new(53);
    stored.add_witness();
    storage
        .push(stored.ledger, &stored.all(), from_endpoint(1))
        .expect("the advertisement holds, so a new ledger is admitted");

    // A longer copy of the witness identity's own ledger moves the
    // advertisement to another machine. Clause 2 admits it: the copy this home
    // stores names the witness identity itself.
    let mut witness = witness_chain(&[home.endpoint_id()]);
    let moved = witness.advertisement(&[secret(60).public()]);
    witness.add(moved);
    storage
        .push(witness.ledger, &witness.from(3), from_endpoint(1))
        .expect("clause 2 admits an extension of a stored ledger");
    assert_eq!(
        storage.witness_for_entries(),
        [WitnessForEntry {
            identity: witness_identity(),
            gap: Some(AdvertisementGap::AdvertisesOtherEndpoints)
        }],
        "storing a longer copy rechecks the invariant"
    );

    // The ledger this home already stores keeps growing.
    stored.add_attestation(9);
    let outcome = storage
        .push(stored.ledger, &stored.from(2), from_endpoint(1))
        .expect("clause 2 keeps firing for a stored ledger");
    assert_eq!(outcome.stored, 1);

    // A ledger it does not store is refused, and the refusal names the reason.
    let mut fresh = Chain::new(54);
    fresh.add_witness();
    let rejection = rejected(
        storage
            .push(fresh.ledger, &fresh.all(), from_endpoint(1))
            .expect_err("the entry advertises another machine"),
    );
    assert_eq!(rejection.code, RejectCode::NotAdmitted);
    assert!(
        rejection
            .msg
            .contains("advertises other endpoints and not this one"),
        "{}",
        rejection.msg
    );
    // Reads of what it holds are untouched.
    assert!(storage.report(stored.ledger).is_some());
}

/// The three reasons of section 4.1, and the recheck when this home's own
/// endpoint id changes. Startup never fails on any of them.
#[test]
fn the_three_advertisement_reasons_are_reported_and_rechecked() {
    let no_copy = Home::witnessing_for(
        mabel_node::DEFAULT_STORAGE_CAPACITY,
        vec![witness_identity()],
    );
    let storage = no_copy.storage(WitnessCaps::default());
    assert_eq!(
        storage.witness_for_entries()[0].gap,
        Some(AdvertisementGap::NoLocalCopy)
    );
    let mut chain = Chain::new(55);
    chain.add_witness();
    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("no copy of the witness identity is stored here"),
    );
    assert!(rejection.msg.contains("holds no copy"), "{}", rejection.msg);

    // An advertisement that names nobody is the second reason.
    let cleared = Home::witnessing_for(
        mabel_node::DEFAULT_STORAGE_CAPACITY,
        vec![witness_identity()],
    );
    cleared.advertise(&[]);
    assert_eq!(
        cleared
            .storage(WitnessCaps::default())
            .witness_for_entries()[0]
            .gap,
        Some(AdvertisementGap::AdvertisesNothing)
    );

    // A regenerated `node.key` is a new endpoint id, and the invariant is
    // checked against it.
    let advertised = home();
    let storage = advertised.storage(WitnessCaps::default());
    assert!(storage.witness_for_entries()[0].advertised());
    storage.note_endpoint(secret(61).public());
    assert_eq!(
        storage.witness_for_entries()[0].gap,
        Some(AdvertisementGap::AdvertisesOtherEndpoints)
    );
    storage.note_endpoint(advertised.endpoint_id());
    assert!(
        storage.witness_for_entries()[0].advertised(),
        "the old key advertises this home again"
    );
}

/// Clause 4 of proposal 006 section 4: the retired tag-11 list admits a push
/// only with the switch on, a non-empty `witness_for`, and this home's own
/// endpoint id in the list. Each leg alone refuses.
#[test]
fn the_legacy_tag_eleven_clause_needs_all_three_of_its_gates() {
    let legacy = |seed: u8, endpoint| {
        let mut chain = Chain::new(seed);
        chain.add_witness_config(&[endpoint]);
        chain
    };
    let migrating = |witness_for: Vec<mabel_core::IdentityId>| AdmissionPolicy {
        witness_for,
        accept_legacy_witness_config: true,
    };

    // Leg one: the switch is off, which is the default.
    let strict = home();
    let chain = legacy(56, strict.endpoint_id());
    let storage = strict.storage(WitnessCaps::default());
    assert!(!storage.accepts_legacy_witness_config());
    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("the switch is off"),
    );
    assert_eq!(rejection.code, RejectCode::NotAdmitted);

    // Every gate open: the same chain lands.
    let storage = strict.storage_with(WitnessCaps::default(), migrating(vec![witness_identity()]));
    let outcome = storage
        .push(chain.ledger, &chain.all(), from_endpoint(1))
        .expect("the legacy clause admits it");
    assert_eq!(outcome.stored, 2);

    // Leg two: the switch is on and `witness_for` is empty, so the home
    // witnesses for nobody and the clause cannot fire.
    let nobody = home_witnessing_for_nobody();
    let chain = legacy(57, nobody.endpoint_id());
    let storage = nobody.storage_with(WitnessCaps::default(), migrating(Vec::new()));
    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("this home witnesses for nobody"),
    );
    assert!(
        rejection.msg.contains("witnesses for nobody"),
        "{}",
        rejection.msg
    );

    // Leg three: the switch is on but the list names another machine.
    let elsewhere = home();
    let chain = legacy(58, secret(62).public());
    let storage =
        elsewhere.storage_with(WitnessCaps::default(), migrating(vec![witness_identity()]));
    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("the tag-11 list names another endpoint"),
    );
    assert_eq!(rejection.code, RejectCode::NotAdmitted);
}

// ------------------------------------------------------- push semantics ----

#[tokio::test]
async fn a_gapped_push_is_malformed() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(4);
        chain.add_witness();
        chain.add_attestation(9);
        let peer = served.dial().await;

        // An unheld ledger must be pushed from seq 0.
        let error = peer
            .client
            .push(chain.ledger, &chain.from(1))
            .await
            .expect_err("seq 1 is not seq 0");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Malformed);
        assert_eq!(rejection.at_seq, 1);

        peer.client
            .push(chain.ledger, &chain.slice(0..2))
            .await
            .expect("the chain names this witness");

        // The ledger ends at seq 1, so seq 3 would leave seq 2 missing.
        chain.add_attestation(10);
        let error = peer
            .client
            .push(chain.ledger, &chain.from(3))
            .await
            .expect_err("seq 3 leaves a gap");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Malformed);
        assert_eq!(rejection.at_seq, 3);
        assert_eq!(
            peer.client
                .head(chain.ledger)
                .await
                .unwrap()
                .expect("the ledger is stored")
                .head_seq,
            1
        );
        served.stop().await;
    });
}

#[tokio::test]
async fn an_overlapping_re_push_is_idempotent() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(5);
        chain.add_witness();
        chain.add_attestation(9);
        let peer = served.dial().await;

        let first = peer
            .client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the chain names this witness");
        assert_eq!(first.stored, 3);

        for events in [chain.all(), chain.from(1), chain.from(2)] {
            let again = peer
                .client
                .push(chain.ledger, &events)
                .await
                .expect("a byte-identical overlap is accepted");
            assert_eq!(again.head_seq, 2);
            assert_eq!(again.stored, 0, "nothing is stored twice");
        }
        assert_eq!(
            served
                .home
                .home
                .ledger(chain.ledger)
                .sequences()
                .expect("the directory lists")
                .len(),
            3
        );
        served.stop().await;
    });
}

#[tokio::test]
async fn a_partially_invalid_push_stores_the_valid_prefix_and_names_the_failing_seq() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(6);
        chain.add_witness();
        // Seq 2 is signed by a key this ledger never authorized, and seq 3
        // would be valid if seq 2 were.
        let forged = chain.forged(9);
        let mut events = chain.all();
        events.push(forged.signed_event);
        let peer = served.dial().await;

        let error = peer
            .client
            .push(chain.ledger, &events)
            .await
            .expect_err("seq 2 does not verify");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Invalid);
        assert_eq!(rejection.at_seq, 2);

        // The valid prefix landed, whole.
        let page = peer
            .client
            .get(chain.ledger, 0, 0)
            .await
            .unwrap()
            .expect("the prefix is stored");
        assert_eq!(page.events, chain.all());
        assert_eq!(page.head_seq, 1);
        assert_eq!(
            served
                .home
                .home
                .ledger(chain.ledger)
                .sequences()
                .expect("the directory lists"),
            vec![0, 1]
        );
        served.stop().await;
    });
}

#[tokio::test]
async fn the_first_ingest_verifies_the_whole_pushed_chain() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(7);
        chain.add_witness();
        chain.add_attestation(9);
        // A broken link at seq 3: the event names an event that is not its
        // predecessor, which only a fold over the whole chain catches.
        let mut broken = chain.at();
        broken.prev = mabel_core::EventId::from_bytes([0xaa; 32]);
        let orphan = mabel_core::sign::build_trust_attestation(
            &secret(7),
            &broken,
            common::subject(10),
            chain.now(),
        )
        .expect("the attestation builds");
        let mut events = chain.all();
        events.push(orphan.signed_event);

        let peer = served.dial().await;
        let error = peer
            .client
            .push(chain.ledger, &events)
            .await
            .expect_err("seq 3 does not link");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Invalid);
        assert_eq!(rejection.at_seq, 3);
        assert_eq!(
            peer.client
                .head(chain.ledger)
                .await
                .unwrap()
                .expect("the prefix is stored")
                .head_seq,
            2
        );
        served.stop().await;
    });
}

#[tokio::test]
async fn a_later_push_verifies_the_spliced_suffix_against_the_kept_state() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(8);
        chain.add_witness();
        let attestation = chain.add_attestation(9);
        let peer = served.dial().await;
        peer.client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the chain names this witness");

        // A revocation is valid only against the trust map the earlier push
        // built, so accepting it proves the folded state was kept.
        chain.add_revocation(attestation);
        let outcome = peer
            .client
            .push(chain.ledger, &chain.from(3))
            .await
            .expect("the suffix verifies against the kept state");
        assert_eq!(outcome.head_seq, 3);
        assert_eq!(outcome.stored, 1);

        // And a suffix that contradicts that state is refused at its own
        // sequence.
        let orphan = chain.revocation(attestation);
        let error = peer
            .client
            .push(chain.ledger, &[orphan.signed_event])
            .await
            .expect_err("the attestation is already revoked");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Invalid);
        assert_eq!(rejection.at_seq, 4);
        served.stop().await;
    });
}

// ----------------------------------------------------------------- forks ---

#[tokio::test]
async fn a_fork_push_records_both_events_while_the_first_survives() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(9);
        chain.add_witness();
        // Two valid events for seq 2; the witness sees this one first.
        let kept = chain.attestation(9);
        let conflicting = chain.attestation(10);
        chain.add(kept.clone());

        let peer = served.dial().await;
        peer.client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the chain names this witness");

        let error = peer
            .client
            .push(
                chain.ledger,
                std::slice::from_ref(&conflicting.signed_event),
            )
            .await
            .expect_err("seq 2 already holds another valid event");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Fork);
        assert_eq!(rejection.at_seq, 2);

        // The record carries both events and the endpoint that offered the
        // conflicting one.
        let forks = peer
            .client
            .forks(None, 0, 0)
            .await
            .expect("the read succeeds");
        assert_eq!(forks.rejected, 0, "a real fork verifies against the ledger");
        assert_eq!(forks.verified.len(), 1);
        let record = &forks.verified[0];
        assert_eq!(record.ledger, chain.ledger);
        assert_eq!(record.seq, 2);
        assert_eq!(record.kept, kept.signed_event);
        assert_eq!(record.conflicting, conflicting.signed_event);
        assert_eq!(record.source_endpoint, Some(peer.endpoint.id()));

        // The event seen first is still the ledger's.
        let page = peer
            .client
            .get(chain.ledger, 0, 0)
            .await
            .unwrap()
            .expect("the ledger is stored");
        assert_eq!(page.events, chain.all());

        let filtered = peer
            .client
            .forks(Some(chain.ledger), 0, 0)
            .await
            .expect("the read succeeds");
        assert_eq!(filtered.verified.len(), 1);
        served.stop().await;
    });
}

#[tokio::test]
async fn an_invalid_conflicting_event_is_rejected_and_not_stored() {
    bounded!({
        let served = Served::new().await;
        let mut chain = Chain::new(10);
        chain.add_witness();
        let forged = chain.forged(9);
        chain.add_attestation(9);

        let peer = served.dial().await;
        peer.client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the chain names this witness");

        // A conflicting event that does not verify is not evidence of
        // anything, so it is INVALID and no record is written.
        let error = peer
            .client
            .push(chain.ledger, &[forged.signed_event])
            .await
            .expect_err("the conflicting event does not verify");
        let rejection = refused(error);
        assert_eq!(rejection.code, RejectCode::Invalid);
        assert_eq!(rejection.at_seq, 2);
        assert!(
            peer.client
                .forks(None, 0, 0)
                .await
                .expect("the read succeeds")
                .verified
                .is_empty()
        );
        assert!(
            served
                .home
                .home
                .ledger(chain.ledger)
                .forks()
                .expect("the directory lists")
                .is_empty(),
            "nothing was written to forks/"
        );
        served.stop().await;
    });
}

#[test]
fn the_ninth_fork_on_one_ledger_is_not_recorded_and_forks_truncated_is_set() {
    let home = home();
    let storage = home.storage(WitnessCaps::default());
    let mut chain = Chain::new(11);
    chain.add_witness();
    // Nine more valid events for seq 2, on top of the one that is stored.
    let candidates: Vec<Vec<u8>> = (0..10)
        .map(|seed| chain.attestation(20 + seed).signed_event)
        .collect();
    chain.add_attestation(9);
    storage
        .push(chain.ledger, &chain.all(), from_endpoint(1))
        .expect("the chain names this witness");

    for (offset, event) in candidates.iter().enumerate() {
        let rejection = rejected(
            storage
                .push(chain.ledger, std::slice::from_ref(event), from_endpoint(2))
                .expect_err("seq 2 is taken"),
        );
        assert_eq!(rejection.code, RejectCode::Fork, "candidate {offset}");
        let summary = &storage
            .report(chain.ledger)
            .expect("the ledger is stored")
            .summary;
        let recorded = u32::try_from(offset + 1).unwrap().min(MAX_FORK_RECORDS);
        assert_eq!(summary.fork_count, recorded, "candidate {offset}");
        assert_eq!(
            summary.forks_truncated,
            recorded >= MAX_FORK_RECORDS,
            "candidate {offset}"
        );
    }

    let summary = &storage
        .report(chain.ledger)
        .expect("the ledger is stored")
        .summary;
    assert_eq!(summary.fork_count, MAX_FORK_RECORDS);
    assert!(summary.forks_truncated);
    assert_eq!(
        home.home
            .ledger(chain.ledger)
            .forks()
            .expect("the directory lists")
            .len(),
        MAX_FORK_RECORDS as usize,
        "the ninth record was never written"
    );

    // Fork paging is stable: the records come back in one order, four at a
    // time, and a restart does not reorder them.
    let whole: Vec<Vec<u8>> = storage
        .forks(None, 0, 64)
        .items
        .into_iter()
        .map(|record| record.conflicting)
        .collect();
    assert_eq!(whole.len(), MAX_FORK_RECORDS as usize);
    let mut paged = Vec::new();
    for offset in [0, 4] {
        let page = storage.forks(Some(chain.ledger), offset, 4);
        assert_eq!(page.items.len(), 4);
        assert_eq!(page.more, offset == 0);
        paged.extend(page.items.into_iter().map(|record| record.conflicting));
    }
    assert_eq!(paged, whole);
    let reopened = home.storage(WitnessCaps::default());
    assert_eq!(
        reopened
            .forks(None, 0, 64)
            .items
            .into_iter()
            .map(|record| record.conflicting)
            .collect::<Vec<_>>(),
        whole,
        "the records read back from disk in the same order"
    );
}

// ------------------------------------------------------------------ caps ---

#[test]
fn a_push_over_the_per_ledger_event_cap_is_rejected() {
    let home = home();
    let storage = home.storage(WitnessCaps {
        events_per_ledger: 2,
        ..WitnessCaps::default()
    });
    let mut chain = Chain::new(12);
    chain.add_witness();
    chain.add_attestation(9);

    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("three events pass the two-event cap"),
    );
    assert_eq!(rejection.code, RejectCode::TooLarge);
    assert!(rejection.msg.contains("event cap"), "{}", rejection.msg);
    assert!(storage.head(chain.ledger).is_none(), "nothing was stored");

    // The push that fits is accepted, and the next event is not.
    storage
        .push(chain.ledger, &chain.slice(0..2), from_endpoint(1))
        .expect("two events fit");
    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.from(2), from_endpoint(1))
            .expect_err("a third event passes the cap"),
    );
    assert_eq!(rejection.code, RejectCode::TooLarge);
}

#[test]
fn a_push_over_the_per_ledger_byte_cap_is_rejected() {
    let home = home();
    let storage = home.storage(WitnessCaps {
        bytes_per_ledger: 200,
        ..WitnessCaps::default()
    });
    let mut chain = Chain::new(13);
    chain.add_witness();
    chain.add_attestation(9);

    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("the chain passes the byte cap"),
    );
    assert_eq!(rejection.code, RejectCode::TooLarge);
    assert!(rejection.msg.contains("byte cap"), "{}", rejection.msg);
    assert!(storage.head(chain.ledger).is_none(), "nothing was stored");
}

#[test]
fn a_push_over_the_ledger_count_cap_is_rejected() {
    let home = home();
    // Two ledgers: the witness identity's own chain, which the home holds so it
    // may take a new one at all, and the first pushed chain.
    let storage = home.storage(WitnessCaps {
        ledgers: 2,
        ..WitnessCaps::default()
    });
    let mut first = Chain::new(14);
    first.add_witness();
    let mut second = Chain::new(15);
    second.add_witness();

    storage
        .push(first.ledger, &first.all(), from_endpoint(1))
        .expect("the first ledger fits");
    let rejection = rejected(
        storage
            .push(second.ledger, &second.all(), from_endpoint(1))
            .expect_err("the second ledger passes the cap"),
    );
    assert_eq!(rejection.code, RejectCode::TooLarge);
    assert!(rejection.msg.contains("ledgers"), "{}", rejection.msg);
    assert_eq!(storage.totals().ledger_count, 2);
}

#[test]
fn a_push_over_the_storage_capacity_from_node_json_is_rejected() {
    // `node.json` is where the capacity comes from (proposal 001 section 8).
    // The capacity leaves 300 bytes above the witness identity's own chain.
    let home = home();
    let held = home.stored_bytes();
    home.set_storage_capacity(held + 300);
    let storage = home.storage(WitnessCaps::from_config(
        &home.home.config().expect("node.json reads"),
    ));
    assert_eq!(storage.caps().storage_capacity, held + 300);
    let mut chain = Chain::new(16);
    chain.add_witness();
    chain.add_attestation(9);

    let rejection = rejected(
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect_err("the chain passes the capacity"),
    );
    assert_eq!(rejection.code, RejectCode::TooLarge);
    assert!(rejection.msg.contains("capacity"), "{}", rejection.msg);
    assert_eq!(
        storage.totals().storage_used,
        held,
        "the refused push stored nothing"
    );
}

// ------------------------------------------------------- reads and state ---

#[test]
fn list_paging_is_stable_in_ascending_ledger_id_order() {
    let home = home();
    let storage = home.storage(WitnessCaps::default());
    // The witness identity's own chain is on disk before anything is pushed, so
    // it is one of the rows the paging walks.
    let mut stored = vec![witness_identity()];
    for seed in [21u8, 22, 23, 24] {
        let mut chain = Chain::new(seed);
        chain.add_witness();
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(1))
            .expect("the chain names this witness");
        stored.push(chain.ledger);
    }
    stored.sort_unstable();

    let mut paged = Vec::new();
    for offset in 0..stored.len() {
        let page = storage.list(offset, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.more, offset < stored.len() - 1, "offset {offset}");
        paged.push(page.items[0].ledger);
    }
    assert_eq!(paged, stored, "paging follows ascending ledger id");

    let whole = storage.list(0, 256);
    assert_eq!(
        whole.items.iter().map(|row| row.ledger).collect::<Vec<_>>(),
        stored
    );
    assert!(!whole.more);
    assert!(storage.list(stored.len(), 10).items.is_empty());
}

#[test]
fn a_restart_rebuilds_the_folded_state_from_disk() {
    let home = home();
    let mut chain = Chain::new(25);
    chain.add_witness();
    let attestation = chain.add_attestation(9);

    let held = home.stored_bytes();
    let before = {
        let storage = home.storage(WitnessCaps::default());
        storage
            .push(chain.ledger, &chain.all(), from_endpoint(3))
            .expect("the chain names this witness");
        storage
            .report(chain.ledger)
            .expect("the ledger is stored")
            .summary
    };

    // A second storage over the same home rebuilds everything from the event
    // files.
    let storage = home.storage(WitnessCaps::default());
    let after = storage
        .report(chain.ledger)
        .expect("the ledger is stored")
        .summary;
    assert_eq!(after.ledger, before.ledger);
    assert_eq!(after.head_seq, before.head_seq);
    assert_eq!(after.head_event, before.head_event);
    assert_eq!(after.event_count, before.event_count);
    assert_eq!(after.first_seen_ms, before.first_seen_ms);
    assert_eq!(
        storage.totals(),
        Totals {
            // The pushed chain beside the witness identity's own.
            ledger_count: 2,
            fork_count: 0,
            storage_used: held
                + chain
                    .all()
                    .iter()
                    .map(|event| event.len() as u64)
                    .sum::<u64>(),
        }
    );

    // The rebuilt state is a real fold: a revocation naming the stored
    // attestation is accepted, which needs the trust map.
    chain.add_revocation(attestation);
    let outcome = storage
        .push(chain.ledger, &chain.from(3), from_endpoint(3))
        .expect("the suffix verifies against the rebuilt state");
    assert_eq!(outcome.head_seq, 3);
}

#[test]
fn the_first_seen_record_and_its_endpoint_survive_later_pushes() {
    let home = home();
    let storage = home.storage(WitnessCaps::default());
    let mut chain = Chain::new(26);
    chain.add_witness();
    storage
        .push(chain.ledger, &chain.all(), from_endpoint(4))
        .expect("the chain names this witness");
    let first = storage.report(chain.ledger).expect("the ledger is stored");
    assert_eq!(first.source_endpoint, Some(secret(4).public()));

    chain.add_attestation(9);
    storage
        .push(chain.ledger, &chain.from(2), from_endpoint(5))
        .expect("a third party may relay");
    let later = storage.report(chain.ledger).expect("the ledger is stored");
    assert_eq!(
        later.source_endpoint,
        Some(secret(4).public()),
        "provenance records who got there first"
    );
    assert_eq!(later.summary.first_seen_ms, first.summary.first_seen_ms);

    // And across a restart, since both come from meta.json.
    let reopened = home.storage(WitnessCaps::default());
    let rebuilt = reopened.report(chain.ledger).expect("the ledger is stored");
    assert_eq!(rebuilt.source_endpoint, Some(secret(4).public()));
    assert_eq!(rebuilt.summary.first_seen_ms, first.summary.first_seen_ms);
}

// --------------------------------------------------------------- runtime ---

/// `mabel witness run`: both listeners, one push, one HTTP read and a clean
/// stop.
#[tokio::test]
async fn the_runtime_serves_both_surfaces_and_shuts_down() {
    bounded!({
        let home = home();
        let endpoint_id = home.endpoint_id();
        let witness = WitnessRuntime::start(
            home.home.clone(),
            WitnessOptions {
                http_bind: Some("127.0.0.1:0".parse().expect("a loopback address")),
                ui: UiSource::Disabled,
                ..WitnessOptions::default()
            },
        )
        .await
        .expect("the witness starts");

        assert_eq!(witness.endpoint_id(), endpoint_id, "the node key is reused");
        let http = witness.http_address();
        assert_ne!(http.port(), 0, "the bound port is reported");
        assert!(witness.warning().is_none(), "a loopback bind is quiet");
        let addr = common::addr_of(endpoint_id, witness.iroh_addresses());
        assert!(!addr.addrs.is_empty(), "the Iroh endpoint bound a socket");

        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(witness.serve_until(async move {
            let _ = stopped.await;
        }));

        // The sync surface accepts a push.
        let endpoint = bind_endpoint(EndpointConfig::new(RelayChoice::Disabled))
            .await
            .expect("the endpoint binds")
            .endpoint;
        let client = Client::connect(&endpoint, addr)
            .await
            .expect("the client connects");
        let mut chain = Chain::new(41);
        chain.add_witness();
        let outcome = client
            .push(chain.ledger, &chain.all())
            .await
            .expect("the chain names this witness");
        assert_eq!(outcome.head_seq, 1);

        // The HTTP surface answers from the same storage.
        let body = tokio::task::spawn_blocking(move || get(http, "/api/node"))
            .await
            .expect("the request finishes");
        assert!(body.contains("\"role\":\"witness\""), "{body}");
        // The pushed chain beside the witness identity's own.
        assert!(body.contains("\"ledger_count\":2"), "{body}");

        stop.send(()).expect("the serve loop is listening");
        serving
            .await
            .expect("the serve task finishes")
            .expect("the witness stops cleanly");
    });
}

/// One HTTP/1.1 GET over a blocking socket, so the test needs no HTTP client.
fn get(address: std::net::SocketAddr, path: &str) -> String {
    use std::io::{Read as _, Write as _};

    let mut stream = std::net::TcpStream::connect(address).expect("the API is listening");
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n",
        address = address
    );
    stream
        .write_all(request.as_bytes())
        .expect("the request is written");
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the response is read");
    answer
}

#[test]
fn the_default_caps_are_the_numbers_of_section_five() {
    let caps = WitnessCaps::default();
    assert_eq!(caps.events_per_ledger, 4096);
    assert_eq!(caps.bytes_per_ledger, 4 * 1024 * 1024);
    assert_eq!(caps.ledgers, 10_000);
    assert_eq!(caps.fork_records, 8);
    assert_eq!(caps.storage_capacity, 2 * 1024 * 1024 * 1024);
}

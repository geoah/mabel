//! Golden vectors for the canonical encoding, the event ids and the
//! signatures (proposal 001 section 11, proposal 002 section 7).
//!
//! The files under `test-vectors/` are literals: the tests here read them and
//! compare, and never write them. The only writer is `gen_vectors`, which is
//! `#[ignore]`d and gated behind the `gen-vectors` feature, so a byte change
//! shows up as a failing test first and as a reviewed diff second:
//!
//! ```text
//! cargo test -p mabel-core --features gen-vectors -- --ignored gen_vectors
//! ```

use std::path::{Path, PathBuf};

use data_encoding::HEXLOWER;
use iroh_base::{PublicKey, SecretKey, Signature};
use mabel_core::proto::{
    Acceptance, DeclaredKind, EventBody, Role, SignedEvent, event_body::Payload, inception,
};
use mabel_core::{
    BuiltEvent, IdentityId, LedgerId, Position, Root, build_acceptance, build_inception,
    build_membership_acceptance, build_membership_invitation, build_membership_removal,
    build_trust_attestation, build_trust_revocation, build_witness_config, event_id, sign_input,
};
use mabel_proto::prost::Message;
use serde_json::{Value, json};

const T0: u64 = 1_700_000_000_000;
const STEP_MS: u64 = 60_000;

struct Vector {
    file: &'static str,
    description: &'static str,
    inputs: Value,
    built: BuiltEvent,
}

impl Vector {
    /// The document a vector file holds. `body_hex` and `signed_event_hex`
    /// are the authoritative bytes; every other field is derived from them.
    fn document(&self) -> Value {
        let signed = SignedEvent::decode(&self.built.signed_event[..]).expect("decodes");
        json!({
            "file": self.file,
            "description": self.description,
            "inputs": self.inputs,
            "body_hex": hex(&self.built.body),
            "signed_event_hex": hex(&self.built.signed_event),
            "event_id": self.built.event_id.to_string(),
            "event_id_hex": hex(self.built.event_id.as_bytes()),
            "signature_hex": hex(&signed.signature),
            "body": render_body(&self.built.body),
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    HEXLOWER.encode(bytes)
}

fn secret(seed: u8) -> SecretKey {
    SecretKey::from_bytes(&[seed; 32])
}

fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-vectors")
}

/// Every vector in one scenario: Alice's raw-rooted ledger, the delegation she
/// grants Bob on it, and the identity-rooted organization she founds, covering
/// every payload variant and both roots with fixed keys, nonces and
/// timestamps.
fn vectors() -> Vec<Vector> {
    let alice = secret(0x11);
    let alice_reserve = secret(0x1a).public();
    let bob = secret(0x22);
    let bob_reserve = secret(0x2a).public();
    let witnesses = [secret(0x77).public(), secret(0x78).public()];
    let alice_nonce = [0xa1u8; 16];
    let bob_nonce = [0xb1u8; 16];
    let organization_nonce = [0xc1u8; 16];

    let raw_root_inception = build_inception(
        &alice,
        DeclaredKind::Person,
        Root::Raw {
            reserve_key: &alice_reserve,
        },
        alice_nonce,
        T0,
    )
    .expect("builds");
    let alice_id: IdentityId = raw_root_inception.event_id.into();

    let bob_inception = build_inception(
        &bob,
        DeclaredKind::Person,
        Root::Raw {
            reserve_key: &bob_reserve,
        },
        bob_nonce,
        T0,
    )
    .expect("builds");
    let bob_id: IdentityId = bob_inception.event_id.into();

    let at = |ledger: LedgerId, seq: u64, prev: &BuiltEvent, prev_timestamp_ms: u64| Position {
        ledger,
        seq,
        prev: prev.event_id,
        prev_timestamp_ms,
    };

    let witness_config = build_witness_config(
        &alice,
        &at(alice_id, 1, &raw_root_inception, T0),
        &witnesses,
        T0 + STEP_MS,
    )
    .expect("builds");

    let trust_attestation = build_trust_attestation(
        &alice,
        &at(alice_id, 2, &witness_config, T0 + STEP_MS),
        bob_id,
        T0 + 2 * STEP_MS,
    )
    .expect("builds");

    let trust_revocation = build_trust_revocation(
        &alice,
        &at(alice_id, 3, &trust_attestation, T0 + 2 * STEP_MS),
        trust_attestation.event_id,
        T0 + 3 * STEP_MS,
    )
    .expect("builds");

    // The organization: an identity root founded by Alice, holding no key of
    // its own.
    let identity_root_inception = build_inception(
        &alice,
        DeclaredKind::Organization,
        Root::Identity {
            founder: alice_id,
            founder_inception: &raw_root_inception.signed_event,
        },
        organization_nonce,
        T0 + 4 * STEP_MS,
    )
    .expect("builds");
    let organization_id: LedgerId = identity_root_inception.event_id.into();

    let membership_invitation = build_membership_invitation(
        &alice,
        &at(
            organization_id,
            1,
            &identity_root_inception,
            T0 + 4 * STEP_MS,
        ),
        bob_id,
        &bob.public(),
        Role::Controller,
        &bob_inception.signed_event,
        T0 + 5 * STEP_MS,
    )
    .expect("builds");

    let accepted = build_acceptance(
        &bob,
        organization_id,
        membership_invitation.event_id,
        bob_id,
    );
    let membership_acceptance = build_membership_acceptance(
        &alice,
        &at(organization_id, 2, &membership_invitation, T0 + 5 * STEP_MS),
        &accepted,
        T0 + 6 * STEP_MS,
    )
    .expect("builds");

    let membership_removal = build_membership_removal(
        &alice,
        &at(organization_id, 3, &membership_acceptance, T0 + 6 * STEP_MS),
        bob_id,
        T0 + 7 * STEP_MS,
    )
    .expect("builds");

    // Delegation on the raw-rooted ledger: Alice admits Bob as a second
    // controller of her own ledger (proposal 002 section 4).
    let delegation_invitation = build_membership_invitation(
        &alice,
        &at(alice_id, 4, &trust_revocation, T0 + 3 * STEP_MS),
        bob_id,
        &bob.public(),
        Role::Controller,
        &bob_inception.signed_event,
        T0 + 8 * STEP_MS,
    )
    .expect("builds");

    let delegation_accepted =
        build_acceptance(&bob, alice_id, delegation_invitation.event_id, bob_id);
    let delegation_acceptance = build_membership_acceptance(
        &alice,
        &at(alice_id, 5, &delegation_invitation, T0 + 8 * STEP_MS),
        &delegation_accepted,
        T0 + 9 * STEP_MS,
    )
    .expect("builds");

    let key_inputs = json!({
        "alice_secret_key_hex": hex(&alice.to_bytes()),
        "alice_identity_id": alice_id.to_string(),
        "bob_secret_key_hex": hex(&bob.to_bytes()),
        "bob_identity_id": bob_id.to_string(),
    });

    vec![
        Vector {
            file: "01-raw-root-inception.json",
            description: "Alice's seq-0 event: a raw root, self-signed by its active key.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "declared_kind": "PERSON",
                "root": "raw_root",
                "reserve_public_key_hex": hex(alice_reserve.as_bytes()),
                "nonce_hex": hex(&alice_nonce),
                "now_ms": T0,
            }),
            built: raw_root_inception.clone(),
        },
        Vector {
            file: "02-witness-config.json",
            description: "Alice replaces her witness set with two endpoints.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": alice_id.to_string(),
                "seq": 1,
                "prev": raw_root_inception.event_id.to_string(),
                "prev_timestamp_ms": T0,
                "now_ms": T0 + STEP_MS,
                "witnesses_hex": witnesses.iter().map(|w| hex(w.as_bytes())).collect::<Vec<_>>(),
            }),
            built: witness_config.clone(),
        },
        Vector {
            file: "03-trust-attestation.json",
            description: "Alice attests that she trusts Bob.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": alice_id.to_string(),
                "seq": 2,
                "prev": witness_config.event_id.to_string(),
                "prev_timestamp_ms": T0 + STEP_MS,
                "now_ms": T0 + 2 * STEP_MS,
                "subject": bob_id.to_string(),
            }),
            built: trust_attestation.clone(),
        },
        Vector {
            file: "04-trust-revocation.json",
            description: "Alice revokes the attestation at seq 2.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": alice_id.to_string(),
                "seq": 3,
                "prev": trust_attestation.event_id.to_string(),
                "prev_timestamp_ms": T0 + 2 * STEP_MS,
                "now_ms": T0 + 3 * STEP_MS,
                "target": trust_attestation.event_id.to_string(),
            }),
            built: trust_revocation.clone(),
        },
        Vector {
            file: "05-identity-root-inception.json",
            description: "Alice founds an organization: an identity root embedding her own \
                          inception.",
            inputs: json!({
                "founder_secret_key_hex": hex(&alice.to_bytes()),
                "declared_kind": "ORGANIZATION",
                "root": "identity_root",
                "founder": alice_id.to_string(),
                "founder_inception_hex": hex(&raw_root_inception.signed_event),
                "nonce_hex": hex(&organization_nonce),
                "now_ms": T0 + 4 * STEP_MS,
            }),
            built: identity_root_inception.clone(),
        },
        Vector {
            file: "06-membership-invitation.json",
            description: "The organization invites Bob as a controller, embedding his inception.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": organization_id.to_string(),
                "seq": 1,
                "prev": identity_root_inception.event_id.to_string(),
                "prev_timestamp_ms": T0 + 4 * STEP_MS,
                "now_ms": T0 + 5 * STEP_MS,
                "invitee": bob_id.to_string(),
                "invitee_key_hex": hex(bob.public().as_bytes()),
                "role": "CONTROLLER",
                "invitee_inception_hex": hex(&bob_inception.signed_event),
            }),
            built: membership_invitation.clone(),
        },
        Vector {
            file: "07-membership-acceptance.json",
            description: "The organization admits Bob, embedding the acceptance Bob signed.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": organization_id.to_string(),
                "seq": 2,
                "prev": membership_invitation.event_id.to_string(),
                "prev_timestamp_ms": T0 + 5 * STEP_MS,
                "now_ms": T0 + 6 * STEP_MS,
                "acceptance_hex": hex(&accepted.acceptance),
                "acceptance_signature_hex": hex(&accepted.signature),
                "acceptance": render_acceptance(&accepted.acceptance),
            }),
            built: membership_acceptance.clone(),
        },
        Vector {
            file: "08-membership-removal.json",
            description: "The organization removes Bob.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": organization_id.to_string(),
                "seq": 3,
                "prev": membership_acceptance.event_id.to_string(),
                "prev_timestamp_ms": T0 + 6 * STEP_MS,
                "now_ms": T0 + 7 * STEP_MS,
                "target": bob_id.to_string(),
            }),
            built: membership_removal,
        },
        Vector {
            file: "09-embedded-raw-root-inception.json",
            description: "Bob's seq-0 event, embedded by vectors 06, 07, 10 and 11.",
            inputs: json!({
                "secret_key_hex": hex(&bob.to_bytes()),
                "declared_kind": "PERSON",
                "root": "raw_root",
                "reserve_public_key_hex": hex(bob_reserve.as_bytes()),
                "nonce_hex": hex(&bob_nonce),
                "now_ms": T0,
                "scenario": key_inputs,
            }),
            built: bob_inception.clone(),
        },
        Vector {
            file: "10-raw-root-delegation-invitation.json",
            description: "Alice invites Bob as a second controller of her own raw-rooted ledger.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": alice_id.to_string(),
                "seq": 4,
                "prev": trust_revocation.event_id.to_string(),
                "prev_timestamp_ms": T0 + 3 * STEP_MS,
                "now_ms": T0 + 8 * STEP_MS,
                "invitee": bob_id.to_string(),
                "invitee_key_hex": hex(bob.public().as_bytes()),
                "role": "CONTROLLER",
                "invitee_inception_hex": hex(&bob_inception.signed_event),
            }),
            built: delegation_invitation.clone(),
        },
        Vector {
            file: "11-raw-root-delegation-acceptance.json",
            description: "Alice admits Bob, who may then sign for her ledger beside its root.",
            inputs: json!({
                "secret_key_hex": hex(&alice.to_bytes()),
                "ledger": alice_id.to_string(),
                "seq": 5,
                "prev": delegation_invitation.event_id.to_string(),
                "prev_timestamp_ms": T0 + 8 * STEP_MS,
                "now_ms": T0 + 9 * STEP_MS,
                "acceptance_hex": hex(&delegation_accepted.acceptance),
                "acceptance_signature_hex": hex(&delegation_accepted.signature),
                "acceptance": render_acceptance(&delegation_accepted.acceptance),
            }),
            built: delegation_acceptance,
        },
    ]
}

/// A human-readable rendering of an encoded `EventBody`. Byte fields render
/// as hex; the event id and the ledger ids render as base32 in `inputs`.
fn render_body(body_bytes: &[u8]) -> Value {
    let body = EventBody::decode(body_bytes).expect("body decodes");
    json!({
        "version": body.version,
        "ledger_hex": hex(&body.ledger),
        "seq": body.seq,
        "prev_hex": hex(&body.prev),
        "timestamp_ms": body.timestamp_ms,
        "author_key_hex": hex(&body.author_key),
        "payload": render_payload(body.payload.expect("payload present")),
    })
}

fn render_payload(payload: Payload) -> Value {
    match payload {
        Payload::Inception(p) => json!({"inception": {
            "declared_kind": DeclaredKind::try_from(p.kind).expect("known kind").as_str_name(),
            "nonce_hex": hex(&p.nonce),
            "root": render_root(p.root.expect("root present")),
        }}),
        Payload::WitnessConfig(p) => json!({"witness_config": {
            "witnesses_hex": p.witnesses.iter().map(|w| hex(w.as_slice())).collect::<Vec<_>>(),
        }}),
        Payload::TrustAttestation(p) => json!({"trust_attestation": {
            "subject_hex": hex(&p.subject),
        }}),
        Payload::TrustRevocation(p) => json!({"trust_revocation": {
            "target_hex": hex(&p.target),
        }}),
        Payload::MembershipInvitation(p) => json!({"membership_invitation": {
            "invitee_hex": hex(&p.invitee),
            "invitee_key_hex": hex(&p.invitee_key),
            "role": Role::try_from(p.role).expect("known role").as_str_name(),
            "invitee_inception_hex": hex(&p.invitee_inception),
        }}),
        Payload::MembershipAcceptance(p) => json!({"membership_acceptance": {
            "acceptance_hex": hex(&p.acceptance),
            "signature_hex": hex(&p.signature),
        }}),
        Payload::MembershipRemoval(p) => json!({"membership_removal": {
            "target_hex": hex(&p.target),
        }}),
    }
}

fn render_root(root: inception::Root) -> Value {
    match root {
        inception::Root::RawRoot(root) => json!({"raw_root": {
            "active_key_hex": hex(&root.active_key),
            "reserve_commit_hex": hex(&root.reserve_commit),
        }}),
        inception::Root::IdentityRoot(root) => json!({"identity_root": {
            "founder_hex": hex(&root.founder),
            "founder_key_hex": hex(&root.founder_key),
            "founder_inception_hex": hex(&root.founder_inception),
        }}),
    }
}

fn render_acceptance(acceptance_bytes: &[u8]) -> Value {
    let acceptance = Acceptance::decode(acceptance_bytes).expect("acceptance decodes");
    json!({
        "version": acceptance.version,
        "ledger_hex": hex(&acceptance.ledger),
        "invitation_event_hex": hex(&acceptance.invitation_event),
        "invitee_hex": hex(&acceptance.invitee),
        "invitee_key_hex": hex(&acceptance.invitee_key),
    })
}

fn read_vector(file: &str) -> Value {
    let path = vectors_dir().join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "missing vector {}: {err}. Regenerate with `cargo test -p mabel-core \
             --features gen-vectors -- --ignored gen_vectors` and review the diff.",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("vector is valid JSON")
}

fn field(document: &Value, key: &str) -> String {
    document[key]
        .as_str()
        .unwrap_or_else(|| panic!("vector field {key} is a string"))
        .to_string()
}

#[test]
fn built_events_match_the_checked_in_vectors() {
    for vector in vectors() {
        assert_eq!(
            read_vector(vector.file),
            vector.document(),
            "vector {} no longer matches what the signing path builds",
            vector.file
        );
    }
}

#[test]
fn vector_digests_and_signatures_hold_on_their_own_bytes() {
    for vector in vectors() {
        let document = read_vector(vector.file);
        let body = HEXLOWER
            .decode(field(&document, "body_hex").as_bytes())
            .expect("body_hex is hex");
        let signed_bytes = HEXLOWER
            .decode(field(&document, "signed_event_hex").as_bytes())
            .expect("signed_event_hex is hex");
        let signature = HEXLOWER
            .decode(field(&document, "signature_hex").as_bytes())
            .expect("signature_hex is hex");

        let signed = SignedEvent::decode(&signed_bytes[..]).expect("decodes");
        assert_eq!(signed.body, body, "{}", vector.file);
        assert_eq!(signed.signature, signature, "{}", vector.file);

        let id = event_id(&body);
        assert_eq!(
            id.to_string(),
            field(&document, "event_id"),
            "{}",
            vector.file
        );
        assert_eq!(hex(id.as_bytes()), field(&document, "event_id_hex"));

        author_key(&body)
            .verify(&sign_input(&body), &to_signature(&signature))
            .unwrap_or_else(|err| panic!("{} signature does not verify: {err}", vector.file));
    }
}

#[test]
fn one_byte_mutation_breaks_every_vector() {
    for vector in vectors() {
        let document = read_vector(vector.file);
        let body = HEXLOWER
            .decode(field(&document, "body_hex").as_bytes())
            .expect("body_hex is hex");
        let signature = to_signature(
            &HEXLOWER
                .decode(field(&document, "signature_hex").as_bytes())
                .expect("signature_hex is hex"),
        );
        let key = author_key(&body);

        for index in [0, body.len() / 2, body.len() - 1] {
            let mut mutated = body.clone();
            mutated[index] ^= 0x01;
            assert_ne!(
                event_id(&mutated).to_string(),
                field(&document, "event_id"),
                "{} keeps its event id after flipping byte {index}",
                vector.file
            );
            assert!(
                key.verify(&sign_input(&mutated), &signature).is_err(),
                "{} still verifies after flipping byte {index}",
                vector.file
            );
        }
    }
}

#[test]
fn every_payload_variant_has_a_vector_and_no_file_is_stale() {
    let built: Vec<&str> = vectors().iter().map(|v| v.file).collect();

    let mut variants: Vec<String> = vectors()
        .iter()
        .map(|v| {
            let body = EventBody::decode(&v.built.body[..]).expect("decodes");
            render_payload(body.payload.expect("payload present"))
                .as_object()
                .expect("one payload key")
                .keys()
                .next()
                .expect("one payload key")
                .clone()
        })
        .collect();
    variants.sort();
    variants.dedup();
    assert_eq!(
        variants,
        vec![
            "inception",
            "membership_acceptance",
            "membership_invitation",
            "membership_removal",
            "trust_attestation",
            "trust_revocation",
            "witness_config",
        ]
    );

    // Both roots are pinned, since the root is the one thing that differs
    // between ledgers (proposal 002 section 2).
    let mut roots: Vec<String> = vectors()
        .iter()
        .filter_map(|v| {
            let body = EventBody::decode(&v.built.body[..]).expect("decodes");
            match body.payload.expect("payload present") {
                Payload::Inception(inception) => Some(
                    render_root(inception.root.expect("root present"))
                        .as_object()
                        .expect("one root key")
                        .keys()
                        .next()
                        .expect("one root key")
                        .clone(),
                ),
                _ => None,
            }
        })
        .collect();
    roots.sort();
    roots.dedup();
    assert_eq!(roots, vec!["identity_root", "raw_root"]);

    let mut on_disk: Vec<String> = std::fs::read_dir(vectors_dir())
        .expect("test-vectors/ exists")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json"))
        .collect();
    on_disk.sort();
    let mut expected: Vec<String> = built.iter().map(|f| f.to_string()).collect();
    expected.sort();
    assert_eq!(on_disk, expected);
}

/// The whole scenario folds: the two ledgers of the vectors are valid chains,
/// and the delegation on the raw-rooted one admits a second controller.
#[test]
fn the_vector_scenario_folds_into_two_valid_ledgers() {
    let files = |names: &[&str]| -> Vec<Vec<u8>> {
        names
            .iter()
            .map(|name| {
                HEXLOWER
                    .decode(field(&read_vector(name), "signed_event_hex").as_bytes())
                    .expect("signed_event_hex is hex")
            })
            .collect()
    };

    let (alice, violation) = mabel_core::fold(files(&[
        "01-raw-root-inception.json",
        "02-witness-config.json",
        "03-trust-attestation.json",
        "04-trust-revocation.json",
        "10-raw-root-delegation-invitation.json",
        "11-raw-root-delegation-acceptance.json",
    ]));
    assert_eq!(violation, None);
    assert_eq!(alice.principals().len(), 2, "Alice delegated to Bob");
    assert_eq!(alice.controller_keys().len(), 2);

    let (organization, violation) = mabel_core::fold(files(&[
        "05-identity-root-inception.json",
        "06-membership-invitation.json",
        "07-membership-acceptance.json",
        "08-membership-removal.json",
    ]));
    assert_eq!(violation, None);
    assert_eq!(
        organization.principals().len(),
        1,
        "Bob was admitted and removed"
    );
}

fn author_key(body_bytes: &[u8]) -> PublicKey {
    let body = EventBody::decode(body_bytes).expect("body decodes");
    let key: [u8; 32] = body.author_key.try_into().expect("32-byte author_key");
    PublicKey::from_bytes(&key).expect("valid ed25519 public key")
}

fn to_signature(bytes: &[u8]) -> Signature {
    let bytes: [u8; 64] = bytes.try_into().expect("64-byte signature");
    Signature::from_bytes(&bytes)
}

/// Rewrites `test-vectors/`. The only writer; run it deliberately and commit
/// the diff for review.
#[cfg(feature = "gen-vectors")]
#[test]
#[ignore = "writes test-vectors/; run explicitly and review the diff"]
fn gen_vectors() {
    let dir = vectors_dir();
    std::fs::create_dir_all(&dir).expect("create test-vectors/");
    for stale in std::fs::read_dir(&dir).expect("test-vectors/ exists") {
        let path = stale.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            std::fs::remove_file(&path).expect("remove a stale vector");
        }
    }
    for vector in vectors() {
        let mut text = serde_json::to_string_pretty(&vector.document()).expect("serializes");
        text.push('\n');
        std::fs::write(dir.join(vector.file), text).expect("write vector");
    }
}

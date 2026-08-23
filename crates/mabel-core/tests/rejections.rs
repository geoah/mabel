//! Rejection vectors: one byte string per validator class and per stateless
//! field-table rule, each with the reason the validator must give
//! (proposal 001 sections 3.1, 3.4 and 11).
//!
//! The files under `test-vectors/rejections/` are literals, exactly like the
//! golden vectors: the tests here read them and compare, and the only writer
//! is `gen_rejections`, which is `#[ignore]`d and gated behind the
//! `gen-vectors` feature:
//!
//! ```text
//! cargo test -p mabel-core --features gen-vectors -- --ignored gen_rejections
//! ```

use std::path::{Path, PathBuf};

use data_encoding::HEXLOWER;
use iroh_base::{PublicKey, SecretKey};
use mabel_core::validate::{self, WireError};
use mabel_core::{
    BuiltEvent, ID_BYTES, IdentityId, LedgerId, MAX_ACCEPTANCE_BYTES, MAX_EMBEDDED_INCEPTION_BYTES,
    MAX_EVENT_BYTES, MAX_TIMESTAMP_MS, MAX_WITNESSES, NONCE_BYTES, Position, SIG_BYTES,
    build_acceptance, build_org_acceptance, build_org_inception, build_org_invite,
    build_org_removal, build_person_inception, build_trust_attestation, build_witness_config,
    proto::Role, reserve_commit, sign_input,
};
use serde_json::{Value, json};

const T0: u64 = 1_700_000_000_000;
const STEP_MS: u64 = 60_000;

/// One rejection vector: the bytes, the entry point that must reject them and
/// the error it must return.
struct Rejection {
    file: String,
    class: &'static str,
    rule: &'static str,
    description: &'static str,
    entry: Entry,
    input: Vec<u8>,
    error: WireError,
}

/// Which validator entry point a vector feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    SignedEvent,
    Acceptance,
}

impl Entry {
    fn name(self) -> &'static str {
        match self {
            Self::SignedEvent => "signed_event",
            Self::Acceptance => "acceptance",
        }
    }

    fn run(self, bytes: &[u8]) -> Result<(), WireError> {
        match self {
            Self::SignedEvent => validate::signed_event(bytes),
            Self::Acceptance => validate::acceptance(bytes),
        }
    }
}

impl Rejection {
    fn document(&self) -> Value {
        json!({
            "file": self.file,
            "class": self.class,
            "rule": self.rule,
            "description": self.description,
            "entry": self.entry.name(),
            "code": self.error.code(),
            "reason": self.error.to_string(),
            "input_hex": hex(&self.input),
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

fn rejections_dir() -> PathBuf {
    vectors_dir().join("rejections")
}

// A record of an encoded message, kept as parts so a case can drop, reorder
// or corrupt exactly one of them.
#[derive(Debug, Clone)]
enum Part {
    /// A varint field.
    V(u32, u64),
    /// A length-delimited field.
    L(u32, Vec<u8>),
}

fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
    out
}

fn key(number: u32, wire: u8) -> Vec<u8> {
    varint(u64::from(number) << 3 | u64::from(wire))
}

fn len_field(number: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = key(number, 2);
    out.extend_from_slice(&varint(payload.len() as u64));
    out.extend_from_slice(payload);
    out
}

fn varint_field(number: u32, value: u64) -> Vec<u8> {
    let mut out = key(number, 0);
    out.extend_from_slice(&varint(value));
    out
}

fn encode(parts: &[Part]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            Part::V(number, value) => out.extend_from_slice(&varint_field(*number, *value)),
            Part::L(number, bytes) => out.extend_from_slice(&len_field(*number, bytes)),
        }
    }
    out
}

fn number_of(part: &Part) -> u32 {
    match part {
        Part::V(number, _) | Part::L(number, _) => *number,
    }
}

/// Replaces the part with the same field number, keeping its position.
fn replace(parts: &mut [Part], part: Part) {
    let number = number_of(&part);
    let slot = parts
        .iter_mut()
        .find(|existing| number_of(existing) == number)
        .expect("the part to replace exists");
    *slot = part;
}

fn drop_part(parts: &mut Vec<Part>, number: u32) {
    parts.retain(|part| number_of(part) != number);
}

/// Wraps body bytes in a `SignedEvent`, signed by `signer`.
///
/// The validator does not check this signature, which the fold verifies once
/// it knows the authorized keys; signing anyway keeps the vectors realistic.
fn sign(body: &[u8], signer: &SecretKey) -> Vec<u8> {
    let sig = signer.sign(&sign_input(body)).to_bytes();
    let mut out = len_field(1, body);
    out.extend_from_slice(&len_field(2, &sig));
    out
}

fn person_inception_parts(
    active: &PublicKey,
    reserve: &PublicKey,
    nonce: [u8; NONCE_BYTES],
) -> Vec<Part> {
    vec![
        Part::V(1, 1),
        Part::L(2, active.as_bytes().to_vec()),
        Part::L(3, reserve_commit(reserve).to_vec()),
        Part::L(4, nonce.to_vec()),
    ]
}

fn org_inception_parts(
    founder: IdentityId,
    founder_key: &PublicKey,
    founder_inception: &[u8],
    nonce: [u8; NONCE_BYTES],
) -> Vec<Part> {
    vec![
        Part::V(1, 2),
        Part::L(2, founder.to_vec()),
        Part::L(3, founder_key.as_bytes().to_vec()),
        Part::L(4, founder_inception.to_vec()),
        Part::L(5, nonce.to_vec()),
    ]
}

fn org_invite_parts(
    invitee: IdentityId,
    invitee_key: &PublicKey,
    role: u64,
    invitee_inception: &[u8],
) -> Vec<Part> {
    vec![
        Part::L(1, invitee.to_vec()),
        Part::L(2, invitee_key.as_bytes().to_vec()),
        Part::V(3, role),
        Part::L(4, invitee_inception.to_vec()),
    ]
}

/// A seq-0 envelope: no `ledger`, no `prev`, no `seq`.
fn inception_body(author: &PublicKey, tag: u32, payload: &[Part]) -> Vec<Part> {
    vec![
        Part::V(5, T0),
        Part::L(6, author.as_bytes().to_vec()),
        Part::L(tag, encode(payload)),
    ]
}

/// An envelope past seq 0, on `ledger` after `prev`.
fn append_body(
    ledger: LedgerId,
    seq: u64,
    prev: &[u8],
    author: &PublicKey,
    tag: u32,
    payload: &[Part],
) -> Vec<Part> {
    vec![
        Part::L(2, ledger.to_vec()),
        Part::V(3, seq),
        Part::L(4, prev.to_vec()),
        Part::V(5, T0 + seq * STEP_MS),
        Part::L(6, author.as_bytes().to_vec()),
        Part::L(tag, encode(payload)),
    ]
}

/// A `SignedEvent` carrying an `OrgInception` whose `founder_inception` is
/// `inner`: well formed enough to reach the embedded-inception check, which
/// is what recurses.
fn org_inception_around(inner: &[u8]) -> Vec<u8> {
    let author = secret(0x33).public();
    let payload = vec![
        Part::V(1, 2),
        Part::L(2, vec![8u8; ID_BYTES]),
        Part::L(3, author.as_bytes().to_vec()),
        Part::L(4, inner.to_vec()),
        Part::L(5, vec![9u8; NONCE_BYTES]),
    ];
    let body = encode(&inception_body(&author, 11, &payload));
    let mut signed = len_field(1, &body);
    signed.extend_from_slice(&len_field(2, &[0u8; SIG_BYTES]));
    signed
}

/// The scenario the vectors mutate: the golden-vector cast, so a reader can
/// diff a rejection against the valid event it came from.
struct Scenario {
    alice: SecretKey,
    bob: SecretKey,
    alice_id: IdentityId,
    bob_id: IdentityId,
    alice_inception: BuiltEvent,
    bob_inception: BuiltEvent,
    attestation: BuiltEvent,
    org_inception: BuiltEvent,
    org_id: LedgerId,
    org_invite: BuiltEvent,
    org_acceptance: BuiltEvent,
    org_removal: BuiltEvent,
    acceptance_blob: Vec<u8>,
    acceptance_sig: [u8; SIG_BYTES],
}

fn scenario() -> Scenario {
    let alice = secret(0x11);
    let bob = secret(0x22);
    let alice_inception =
        build_person_inception(&alice, &secret(0x1a).public(), [0xa1; NONCE_BYTES], T0)
            .expect("builds");
    let bob_inception =
        build_person_inception(&bob, &secret(0x2a).public(), [0xb1; NONCE_BYTES], T0)
            .expect("builds");
    let alice_id: IdentityId = alice_inception.event_id.into();
    let bob_id: IdentityId = bob_inception.event_id.into();

    let attestation = build_trust_attestation(
        &alice,
        &Position {
            ledger: alice_id,
            seq: 1,
            prev: alice_inception.event_id,
            prev_timestamp_ms: T0,
        },
        bob_id,
        T0 + STEP_MS,
    )
    .expect("builds");

    let org_inception = build_org_inception(
        &alice,
        alice_id,
        &alice_inception.signed_event,
        [0xc1; NONCE_BYTES],
        T0 + 4 * STEP_MS,
    )
    .expect("builds");
    let org_id: LedgerId = org_inception.event_id.into();

    let org_invite = build_org_invite(
        &alice,
        &Position {
            ledger: org_id,
            seq: 1,
            prev: org_inception.event_id,
            prev_timestamp_ms: T0 + 4 * STEP_MS,
        },
        bob_id,
        &bob.public(),
        Role::Controller,
        &bob_inception.signed_event,
        T0 + 5 * STEP_MS,
    )
    .expect("builds");

    let accepted = build_acceptance(&bob, org_id, org_invite.event_id, bob_id);
    let org_acceptance = build_org_acceptance(
        &alice,
        &Position {
            ledger: org_id,
            seq: 2,
            prev: org_invite.event_id,
            prev_timestamp_ms: T0 + 5 * STEP_MS,
        },
        &accepted,
        T0 + 6 * STEP_MS,
    )
    .expect("builds");

    let org_removal = build_org_removal(
        &alice,
        &Position {
            ledger: org_id,
            seq: 3,
            prev: org_acceptance.event_id,
            prev_timestamp_ms: T0 + 6 * STEP_MS,
        },
        bob_id,
        T0 + 7 * STEP_MS,
    )
    .expect("builds");

    Scenario {
        alice,
        bob,
        alice_id,
        bob_id,
        alice_inception,
        bob_inception,
        attestation,
        org_inception,
        org_id,
        org_invite,
        org_acceptance,
        org_removal,
        acceptance_blob: accepted.acceptance,
        acceptance_sig: accepted.sig,
    }
}

/// Every rejection vector, in file order.
fn rejections() -> Vec<Rejection> {
    let s = scenario();
    let mut cases: Vec<Rejection> = Vec::new();

    let mut push = |name: &str,
                    class: &'static str,
                    rule: &'static str,
                    description: &'static str,
                    entry: Entry,
                    input: Vec<u8>,
                    error: WireError| {
        let file = format!("{:02}-{name}.json", cases.len() + 1);
        cases.push(Rejection {
            file,
            class,
            rule,
            description,
            entry,
            input,
            error,
        });
    };

    // The seven wire-format classes of section 3.1, plus truncation, the
    // caps and the nesting guard.

    let attestation_parts = || {
        append_body(
            s.alice_id,
            1,
            s.alice_inception.event_id.as_bytes(),
            &s.alice.public(),
            13,
            &[Part::L(1, s.bob_id.to_vec())],
        )
    };

    let mut parts = attestation_parts();
    parts.insert(5, Part::V(7, 1));
    push(
        "unknown-field-number",
        "wire-format",
        "3.1 unknown field numbers",
        "An EventBody carrying field 7, which the schema does not declare.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::UnknownField {
            message: "EventBody",
            number: 7,
        },
    );

    let mut parts = attestation_parts();
    parts.insert(5, Part::L(6, s.alice.public().as_bytes().to_vec()));
    push(
        "duplicate-field",
        "wire-format",
        "3.1 duplicate non-repeated fields",
        "An EventBody carrying author_key twice.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::DuplicateField {
            message: "EventBody",
            field: "author_key",
        },
    );

    let mut parts = attestation_parts();
    parts.swap(3, 4);
    push(
        "field-out-of-order",
        "wire-format",
        "3.1 out-of-order fields",
        "An EventBody whose timestamp_ms follows author_key.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::FieldOutOfOrder {
            message: "EventBody",
            number: 5,
        },
    );

    let parts = attestation_parts();
    let mut body = Vec::new();
    for part in &parts {
        match part {
            // seq 1 padded to two bytes.
            Part::V(3, value) => {
                body.extend_from_slice(&key(3, 0));
                body.extend_from_slice(&[(*value as u8) | 0x80, 0x00]);
            }
            other => body.extend_from_slice(&encode(std::slice::from_ref(other))),
        }
    }
    push(
        "non-minimal-varint",
        "wire-format",
        "3.1 non-minimal varints",
        "An EventBody whose seq is written as a padded two-byte varint.",
        Entry::SignedEvent,
        sign(&body, &s.alice),
        WireError::NonMinimalVarint {
            message: "EventBody",
        },
    );

    let mut body = encode(&attestation_parts()[..3]);
    body.extend_from_slice(&key(5, 0));
    body.extend_from_slice(&[0xff; 10]);
    body.push(0x01);
    push(
        "varint-overflow",
        "wire-format",
        "3.1 non-minimal varints",
        "An EventBody whose timestamp_ms does not fit in 64 bits.",
        Entry::SignedEvent,
        sign(&body, &s.alice),
        WireError::VarintOverflow {
            message: "EventBody",
        },
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::V(6, 1));
    push(
        "wrong-wire-type",
        "wire-format",
        "3.1 wrong wire types",
        "An EventBody whose author_key arrives as a varint.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongWireType {
            message: "EventBody",
            field: "author_key",
            expected: 2,
            actual: 0,
        },
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 13);
    parts.push(Part::L(18, encode(&[Part::L(1, s.bob_id.to_vec())])));
    push(
        "unknown-oneof-variant",
        "wire-format",
        "3.1 unrecognised oneof variants",
        "An EventBody whose payload uses tag 18, which v0 does not define.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::UnknownOneofVariant {
            message: "EventBody",
            oneof: "payload",
            number: 18,
        },
    );

    let mut payload = person_inception_parts(
        &s.alice.public(),
        &secret(0x1a).public(),
        [0xa1; NONCE_BYTES],
    );
    replace(&mut payload, Part::V(1, 0));
    let parts = inception_body(&s.alice.public(), 10, &payload);
    push(
        "unspecified-enum",
        "wire-format",
        "3.1 *_UNSPECIFIED enum values",
        "A PersonInception whose kind is IDENTITY_KIND_UNSPECIFIED.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::UnspecifiedEnum {
            message: "PersonInception",
            field: "kind",
        },
    );

    let mut truncated = key(1, 2);
    truncated.extend_from_slice(&varint(4000));
    truncated.extend_from_slice(&s.attestation.body[..8]);
    push(
        "truncated",
        "wire-format",
        "3.1 truncated input",
        "A SignedEvent whose body claims 4000 bytes and carries 8.",
        Entry::SignedEvent,
        truncated,
        WireError::Truncated {
            message: "SignedEvent",
        },
    );

    let mut oversize = s.attestation.signed_event.clone();
    oversize.resize(MAX_EVENT_BYTES + 1, 0);
    push(
        "event-over-the-cap",
        "field-table",
        "3.4 SignedEvent <= 4096 bytes",
        "A SignedEvent one byte over the 4096-byte cap.",
        Entry::SignedEvent,
        oversize,
        WireError::MessageTooLarge {
            message: "SignedEvent",
            len: MAX_EVENT_BYTES + 1,
            cap: MAX_EVENT_BYTES,
        },
    );

    push(
        "nesting-too-deep",
        "wire-format",
        "3.1 bounded work per message",
        "Three embedded inceptions nested inside one another.",
        Entry::SignedEvent,
        org_inception_around(&org_inception_around(&org_inception_around(&[0xff; 8]))),
        WireError::TooDeeplyNested,
    );

    // The field table of section 3.4, row by row.

    let mut signed = len_field(1, &s.attestation.body);
    let sig = s.alice.sign(&sign_input(&s.attestation.body)).to_bytes();
    signed.extend_from_slice(&len_field(2, &sig[..SIG_BYTES - 1]));
    push(
        "signed-event-sig-length",
        "field-table",
        "3.4 SignedEvent.sig is 64 bytes",
        "A SignedEvent whose signature is 63 bytes.",
        Entry::SignedEvent,
        signed,
        WireError::WrongLength {
            message: "SignedEvent",
            field: "sig",
            expected: SIG_BYTES,
            actual: SIG_BYTES - 1,
        },
    );

    push(
        "signed-event-body-missing",
        "field-table",
        "3.4 SignedEvent.body is required",
        "A SignedEvent carrying only a signature.",
        Entry::SignedEvent,
        len_field(2, &sig),
        WireError::MissingField {
            message: "SignedEvent",
            field: "body",
        },
    );

    let mut parts = attestation_parts();
    parts.insert(0, Part::V(1, 1));
    push(
        "event-body-version-present",
        "field-table",
        "3.4 EventBody.version is absent",
        "An EventBody declaring version 1, which v0 rejects.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::FieldForbidden {
            message: "EventBody",
            field: "version",
        },
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::L(2, vec![0x5a; ID_BYTES - 1]));
    push(
        "event-body-ledger-length",
        "field-table",
        "3.4 EventBody.ledger is 32 bytes",
        "An EventBody whose ledger is 31 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "EventBody",
            field: "ledger",
            expected: ID_BYTES,
            actual: ID_BYTES - 1,
        },
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 4);
    push(
        "event-body-prev-missing",
        "field-table",
        "3.4 EventBody.prev is present past seq 0",
        "An event at seq 1 with no prev.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::MissingChainField { field: "prev" },
    );

    let payload = person_inception_parts(
        &s.alice.public(),
        &secret(0x1a).public(),
        [0xa1; NONCE_BYTES],
    );
    let mut parts = inception_body(&s.alice.public(), 10, &payload);
    parts.insert(0, Part::L(2, s.alice_id.to_vec()));
    push(
        "event-body-ledger-at-seq-zero",
        "field-table",
        "3.4 EventBody.ledger is absent at seq 0",
        "An inception that also names a ledger.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::SetAtSeqZero { field: "ledger" },
    );

    let mut parts = inception_body(&s.alice.public(), 10, &payload);
    parts.insert(0, Part::V(3, 0));
    push(
        "event-body-seq-zero-encoded",
        "field-table",
        "3.1 no proto3 default is serialized",
        "An inception that writes seq 0 instead of omitting it.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::DefaultValueEncoded {
            message: "EventBody",
            field: "seq",
        },
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 5);
    push(
        "event-body-timestamp-missing",
        "field-table",
        "3.4 EventBody.timestamp_ms is required",
        "An event with no timestamp_ms.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::MissingField {
            message: "EventBody",
            field: "timestamp_ms",
        },
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::V(5, 0));
    push(
        "event-body-timestamp-zero",
        "field-table",
        "3.4 timestamp_ms in 1..=4102444800000",
        "An event whose timestamp_ms is 0, the proto3 default.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::DefaultValueEncoded {
            message: "EventBody",
            field: "timestamp_ms",
        },
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::V(5, MAX_TIMESTAMP_MS + 1));
    push(
        "event-body-timestamp-past-2100",
        "field-table",
        "3.4 timestamp_ms in 1..=4102444800000",
        "An event one millisecond past the year-2100 bound.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::ValueOutOfRange {
            message: "EventBody",
            field: "timestamp_ms",
            value: MAX_TIMESTAMP_MS + 1,
            min: 1,
            max: MAX_TIMESTAMP_MS,
        },
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::L(6, vec![0x7c; ID_BYTES - 1]));
    push(
        "event-body-author-key-length",
        "field-table",
        "3.4 EventBody.author_key is 32 bytes",
        "An event whose author_key is 31 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "EventBody",
            field: "author_key",
            expected: ID_BYTES,
            actual: ID_BYTES - 1,
        },
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 13);
    push(
        "event-body-payload-missing",
        "field-table",
        "3.4 EventBody.payload is exactly one recognised variant",
        "An event with no payload.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::MissingOneof {
            message: "EventBody",
            oneof: "payload",
        },
    );

    let mut parts = attestation_parts();
    parts.push(Part::L(
        14,
        encode(&[Part::L(1, s.attestation.event_id.to_vec())]),
    ));
    push(
        "event-body-two-payloads",
        "field-table",
        "3.4 EventBody.payload is exactly one recognised variant",
        "An event carrying both a trust_attestation and a trust_revocation.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::MultipleOneofVariants {
            message: "EventBody",
            oneof: "payload",
        },
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 13);
    parts.push(Part::L(
        10,
        encode(&person_inception_parts(
            &s.alice.public(),
            &secret(0x1a).public(),
            [0xa1; NONCE_BYTES],
        )),
    ));
    push(
        "inception-past-seq-zero",
        "field-table",
        "3.4 an inception sits at seq 0",
        "A PersonInception payload at seq 1.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::InceptionPastSeqZero,
    );

    let parts = inception_body(&s.alice.public(), 13, &[Part::L(1, s.bob_id.to_vec())]);
    push(
        "non-inception-at-seq-zero",
        "field-table",
        "3.4 an inception sits at seq 0",
        "A TrustAttestation payload at seq 0.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::NonInceptionAtSeqZero,
    );

    let mut payload = person_inception_parts(
        &s.alice.public(),
        &secret(0x1a).public(),
        [0xa1; NONCE_BYTES],
    );
    replace(&mut payload, Part::V(1, 2));
    let parts = inception_body(&s.alice.public(), 10, &payload);
    push(
        "person-inception-kind-org",
        "field-table",
        "3.4 *Inception.kind matches the variant",
        "A PersonInception whose kind is ORG.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::EnumValue {
            message: "PersonInception",
            field: "kind",
            value: 2,
        },
    );

    let mut payload = person_inception_parts(
        &s.alice.public(),
        &secret(0x1a).public(),
        [0xa1; NONCE_BYTES],
    );
    replace(&mut payload, Part::L(4, vec![0xa1; NONCE_BYTES - 1]));
    let parts = inception_body(&s.alice.public(), 10, &payload);
    push(
        "person-inception-nonce-length",
        "field-table",
        "3.4 *Inception.nonce is 16 bytes",
        "A PersonInception whose nonce is 15 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "PersonInception",
            field: "nonce",
            expected: NONCE_BYTES,
            actual: NONCE_BYTES - 1,
        },
    );

    let mut payload = person_inception_parts(
        &s.alice.public(),
        &secret(0x1a).public(),
        [0xa1; NONCE_BYTES],
    );
    replace(
        &mut payload,
        Part::L(3, s.alice.public().as_bytes().to_vec()),
    );
    let parts = inception_body(&s.alice.public(), 10, &payload);
    push(
        "person-inception-commit-equals-key",
        "field-table",
        "3.4 active_key and reserve_commit differ",
        "A PersonInception committing to its own active key.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::FieldsMustDiffer {
            first: "PersonInception.active_key",
            second: "PersonInception.reserve_commit",
        },
    );

    let payload = person_inception_parts(
        &s.alice.public(),
        &secret(0x1a).public(),
        [0xa1; NONCE_BYTES],
    );
    let parts = inception_body(&s.bob.public(), 10, &payload);
    push(
        "person-inception-author-key-mismatch",
        "field-table",
        "3.6 a person's seq-0 event is self-signed",
        "A PersonInception whose author_key is not the active_key it records.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.bob),
        WireError::FieldsMustMatch {
            first: "EventBody.author_key",
            second: "PersonInception.active_key",
        },
    );

    let mismatched = build_org_inception(
        &s.alice,
        s.bob_id,
        &s.alice_inception.signed_event,
        [0xc1; NONCE_BYTES],
        T0 + 4 * STEP_MS,
    )
    .expect("builds");
    push(
        "org-inception-founder-mismatch",
        "field-table",
        "3.4 the embedded inception hashes to the recorded id",
        "An OrgInception naming Bob as founder while embedding Alice's inception.",
        Entry::SignedEvent,
        mismatched.signed_event,
        WireError::InceptionIdMismatch {
            field: "OrgInception.founder",
        },
    );

    let payload = org_inception_parts(
        s.alice_id,
        &s.bob.public(),
        &s.alice_inception.signed_event,
        [0xc1; NONCE_BYTES],
    );
    let parts = inception_body(&s.bob.public(), 11, &payload);
    push(
        "org-inception-founder-key-mismatch",
        "field-table",
        "3.4 the embedded inception records the recorded key",
        "An OrgInception whose founder_key is not the embedded inception's active_key.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.bob),
        WireError::InceptionKeyMismatch {
            field: "OrgInception.founder_key",
        },
    );

    let payload = org_inception_parts(
        s.org_id,
        &s.alice.public(),
        &s.org_inception.signed_event,
        [0xc1; NONCE_BYTES],
    );
    let parts = inception_body(&s.alice.public(), 11, &payload);
    push(
        "org-inception-embedded-not-person",
        "field-table",
        "3.4 the embedded inception is a PERSON seq-0 event",
        "An OrgInception embedding another org's inception.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::NotPersonInception,
    );

    let mut broken = s.alice_inception.signed_event.clone();
    let last = broken.len() - 1;
    broken[last] ^= 0x01;
    let payload = org_inception_parts(s.alice_id, &s.alice.public(), &broken, [0xc1; NONCE_BYTES]);
    let parts = inception_body(&s.alice.public(), 11, &payload);
    push(
        "org-inception-embedded-bad-signature",
        "field-table",
        "3.4 the embedded inception verifies standalone",
        "An OrgInception whose embedded inception has one signature bit flipped.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::BadSignature {
            message: "SignedEvent",
            field: "sig",
        },
    );

    let oversize_inception = [0x11u8; MAX_EMBEDDED_INCEPTION_BYTES + 1];
    let payload = org_inception_parts(
        s.alice_id,
        &s.alice.public(),
        &oversize_inception,
        [0xc1; NONCE_BYTES],
    );
    let parts = inception_body(&s.alice.public(), 11, &payload);
    push(
        "org-inception-embedded-too-long",
        "field-table",
        "3.4 founder_inception <= 1024 bytes",
        "An OrgInception whose founder_inception is one byte over the cap.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::FieldTooLong {
            message: "OrgInception",
            field: "founder_inception",
            len: MAX_EMBEDDED_INCEPTION_BYTES + 1,
            cap: MAX_EMBEDDED_INCEPTION_BYTES,
        },
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        12,
        &[],
    );
    push(
        "witness-config-empty",
        "field-table",
        "3.4 WitnessConfig holds 1 to 16 witnesses",
        "A WitnessConfig naming no witnesses.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::RepeatedCount {
            message: "WitnessConfig",
            field: "witnesses",
            count: 0,
            min: 1,
            max: MAX_WITNESSES,
        },
    );

    let many: Vec<Part> = (0..=MAX_WITNESSES as u8)
        .map(|seed| Part::L(1, secret(0x80 + seed).public().as_bytes().to_vec()))
        .collect();
    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        12,
        &many,
    );
    push(
        "witness-config-seventeen",
        "field-table",
        "3.4 WitnessConfig holds 1 to 16 witnesses",
        "A WitnessConfig naming 17 witnesses.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::RepeatedCount {
            message: "WitnessConfig",
            field: "witnesses",
            count: MAX_WITNESSES + 1,
            min: 1,
            max: MAX_WITNESSES,
        },
    );

    let witness = secret(0x77).public().as_bytes().to_vec();
    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        12,
        &[Part::L(1, witness.clone()), Part::L(1, witness)],
    );
    push(
        "witness-config-duplicate",
        "field-table",
        "3.4 witnesses are distinct",
        "A WitnessConfig naming the same endpoint twice.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::RepeatedDuplicate {
            message: "WitnessConfig",
            field: "witnesses",
        },
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        12,
        &[Part::L(1, vec![0x77; ID_BYTES - 1])],
    );
    push(
        "witness-config-witness-length",
        "field-table",
        "3.4 each witness is 32 bytes",
        "A WitnessConfig whose witness is 31 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "WitnessConfig",
            field: "witnesses",
            expected: ID_BYTES,
            actual: ID_BYTES - 1,
        },
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        13,
        &[Part::L(1, s.alice_id.to_vec())],
    );
    push(
        "trust-attestation-subject-is-issuer",
        "field-table",
        "3.4 TrustAttestation.subject differs from ledger_id",
        "An attestation whose subject is the issuing ledger.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::FieldsMustDiffer {
            first: "TrustAttestation.subject",
            second: "EventBody.ledger",
        },
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        13,
        &[Part::L(1, vec![0x4d; ID_BYTES + 1])],
    );
    push(
        "trust-attestation-subject-length",
        "field-table",
        "3.4 TrustAttestation.subject is 32 bytes",
        "An attestation whose subject is 33 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "TrustAttestation",
            field: "subject",
            expected: ID_BYTES,
            actual: ID_BYTES + 1,
        },
    );

    let parts = append_body(
        s.alice_id,
        2,
        s.attestation.event_id.as_bytes(),
        &s.alice.public(),
        14,
        &[Part::L(1, vec![0xf7; ID_BYTES - 1])],
    );
    push(
        "trust-revocation-target-length",
        "field-table",
        "3.4 TrustRevocation.target is 32 bytes",
        "A revocation whose target is 31 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "TrustRevocation",
            field: "target",
            expected: ID_BYTES,
            actual: ID_BYTES - 1,
        },
    );

    let mut payload = org_invite_parts(s.bob_id, &s.bob.public(), 2, &s.bob_inception.signed_event);
    drop_part(&mut payload, 3);
    let parts = append_body(
        s.org_id,
        1,
        s.org_inception.event_id.as_bytes(),
        &s.alice.public(),
        15,
        &payload,
    );
    push(
        "org-invite-role-unspecified",
        "field-table",
        "3.4 OrgInvite.role is MEMBER or CONTROLLER",
        "An invite with no role, which reads as ROLE_UNSPECIFIED.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::UnspecifiedEnum {
            message: "OrgInvite",
            field: "role",
        },
    );

    let payload = org_invite_parts(s.bob_id, &s.bob.public(), 3, &s.bob_inception.signed_event);
    let parts = append_body(
        s.org_id,
        1,
        s.org_inception.event_id.as_bytes(),
        &s.alice.public(),
        15,
        &payload,
    );
    push(
        "org-invite-role-unknown",
        "field-table",
        "3.4 OrgInvite.role is MEMBER or CONTROLLER",
        "An invite whose role is 3, a value v0 does not define.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::EnumValue {
            message: "OrgInvite",
            field: "role",
            value: 3,
        },
    );

    let payload = org_invite_parts(
        s.alice_id,
        &s.bob.public(),
        2,
        &s.bob_inception.signed_event,
    );
    let parts = append_body(
        s.org_id,
        1,
        s.org_inception.event_id.as_bytes(),
        &s.alice.public(),
        15,
        &payload,
    );
    push(
        "org-invite-invitee-mismatch",
        "field-table",
        "3.4 the embedded inception hashes to the recorded id",
        "An invite naming Alice while embedding Bob's inception.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::InceptionIdMismatch {
            field: "OrgInvite.invitee",
        },
    );

    let payload = org_invite_parts(
        s.bob_id,
        &s.alice.public(),
        2,
        &s.bob_inception.signed_event,
    );
    let parts = append_body(
        s.org_id,
        1,
        s.org_inception.event_id.as_bytes(),
        &s.alice.public(),
        15,
        &payload,
    );
    push(
        "org-invite-invitee-key-mismatch",
        "field-table",
        "3.4 the embedded inception records the recorded key",
        "An invite whose invitee_key is not the embedded inception's active_key.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::InceptionKeyMismatch {
            field: "OrgInvite.invitee_key",
        },
    );

    let parts = append_body(
        s.org_id,
        2,
        s.org_invite.event_id.as_bytes(),
        &s.alice.public(),
        16,
        &[
            Part::L(1, s.acceptance_blob.clone()),
            Part::L(2, s.acceptance_sig[..SIG_BYTES - 1].to_vec()),
        ],
    );
    push(
        "org-acceptance-sig-length",
        "field-table",
        "3.4 OrgAcceptance.sig is 64 bytes",
        "An acceptance whose invitee signature is 63 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "OrgAcceptance",
            field: "sig",
            expected: SIG_BYTES,
            actual: SIG_BYTES - 1,
        },
    );

    let mut sig = s.acceptance_sig;
    sig[0] ^= 0x01;
    let parts = append_body(
        s.org_id,
        2,
        s.org_invite.event_id.as_bytes(),
        &s.alice.public(),
        16,
        &[
            Part::L(1, s.acceptance_blob.clone()),
            Part::L(2, sig.to_vec()),
        ],
    );
    push(
        "org-acceptance-bad-signature",
        "field-table",
        "3.5 sig verifies over accept_input under invitee_key",
        "An acceptance whose invitee signature has one bit flipped.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::BadSignature {
            message: "OrgAcceptance",
            field: "sig",
        },
    );

    // 0x02 repeated is 32 bytes that do not decompress to a curve point.
    let blob = encode(&[
        Part::L(2, s.org_id.to_vec()),
        Part::L(3, s.org_invite.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
        Part::L(5, vec![0x02; ID_BYTES]),
    ]);
    let parts = append_body(
        s.org_id,
        2,
        s.org_invite.event_id.as_bytes(),
        &s.alice.public(),
        16,
        &[Part::L(1, blob), Part::L(2, s.acceptance_sig.to_vec())],
    );
    push(
        "acceptance-invitee-key-not-a-point",
        "field-table",
        "3.5 invitee_key is an ed25519 public key",
        "An Acceptance blob whose invitee_key is 32 bytes but not a curve point.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::InvalidPublicKey {
            message: "Acceptance",
            field: "invitee_key",
        },
    );

    let parts = append_body(
        s.org_id,
        2,
        s.org_invite.event_id.as_bytes(),
        &s.alice.public(),
        16,
        &[
            Part::L(1, vec![0x2b; MAX_ACCEPTANCE_BYTES + 1]),
            Part::L(2, s.acceptance_sig.to_vec()),
        ],
    );
    push(
        "org-acceptance-blob-too-long",
        "field-table",
        "3.4 OrgAcceptance.acceptance <= 1024 bytes",
        "An acceptance blob one byte over the cap.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::FieldTooLong {
            message: "OrgAcceptance",
            field: "acceptance",
            len: MAX_ACCEPTANCE_BYTES + 1,
            cap: MAX_ACCEPTANCE_BYTES,
        },
    );

    let mut blob = vec![Part::V(1, 1)];
    blob.extend_from_slice(&[
        Part::L(2, s.org_id.to_vec()),
        Part::L(3, s.org_invite.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
        Part::L(5, s.bob.public().as_bytes().to_vec()),
    ]);
    push(
        "acceptance-version-present",
        "field-table",
        "3.4 Acceptance.version is absent",
        "An Acceptance blob declaring version 1.",
        Entry::Acceptance,
        encode(&blob),
        WireError::FieldForbidden {
            message: "Acceptance",
            field: "version",
        },
    );

    let blob = vec![
        Part::L(2, vec![0x0c; ID_BYTES - 1]),
        Part::L(3, s.org_invite.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
        Part::L(5, s.bob.public().as_bytes().to_vec()),
    ];
    push(
        "acceptance-org-length",
        "field-table",
        "3.4 Acceptance fields are 32 bytes each",
        "An Acceptance blob whose org is 31 bytes.",
        Entry::Acceptance,
        encode(&blob),
        WireError::WrongLength {
            message: "Acceptance",
            field: "org",
            expected: ID_BYTES,
            actual: ID_BYTES - 1,
        },
    );

    let blob = vec![
        Part::L(2, s.org_id.to_vec()),
        Part::L(3, s.org_invite.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
    ];
    push(
        "acceptance-invitee-key-missing",
        "field-table",
        "3.4 Acceptance fields are required",
        "An Acceptance blob with no invitee_key.",
        Entry::Acceptance,
        encode(&blob),
        WireError::MissingField {
            message: "Acceptance",
            field: "invitee_key",
        },
    );

    let parts = append_body(
        s.org_id,
        3,
        s.org_acceptance.event_id.as_bytes(),
        &s.alice.public(),
        17,
        &[Part::L(1, vec![0x22; ID_BYTES + 1])],
    );
    push(
        "org-removal-target-length",
        "field-table",
        "3.4 OrgRemoval.target is 32 bytes",
        "A removal whose target is 33 bytes.",
        Entry::SignedEvent,
        sign(&encode(&parts), &s.alice),
        WireError::WrongLength {
            message: "OrgRemoval",
            field: "target",
            expected: ID_BYTES,
            actual: ID_BYTES + 1,
        },
    );

    cases
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "missing vector {}: {err}. Regenerate with `cargo test -p mabel-core \
             --features gen-vectors -- --ignored gen_rejections` and review the diff.",
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
fn rejection_vectors_match_the_checked_in_files() {
    for rejection in rejections() {
        let path = rejections_dir().join(&rejection.file);
        assert_eq!(
            read_json(&path),
            rejection.document(),
            "vector {} no longer matches what the test builds",
            rejection.file
        );
    }
}

#[test]
fn every_rejection_vector_is_rejected_with_its_reason() {
    for rejection in rejections() {
        let document = read_json(&rejections_dir().join(&rejection.file));
        let input = HEXLOWER
            .decode(field(&document, "input_hex").as_bytes())
            .expect("input_hex is hex");
        let entry = match field(&document, "entry").as_str() {
            "signed_event" => Entry::SignedEvent,
            "acceptance" => Entry::Acceptance,
            other => panic!("unknown entry point {other}"),
        };
        let err = entry
            .run(&input)
            .expect_err(&format!("{} must be rejected", rejection.file));
        assert_eq!(err.code(), field(&document, "code"), "{}", rejection.file);
        assert_eq!(
            err.to_string(),
            field(&document, "reason"),
            "{}",
            rejection.file
        );
    }
}

#[test]
fn no_rejection_file_is_stale() {
    let mut on_disk: Vec<String> = std::fs::read_dir(rejections_dir())
        .expect("test-vectors/rejections/ exists")
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
    let mut expected: Vec<String> = rejections().into_iter().map(|case| case.file).collect();
    expected.sort();
    assert_eq!(on_disk, expected);
}

/// The other half of the contract: the golden vectors of ticket 002 must all
/// pass, bytes and body alike.
#[test]
fn every_golden_vector_passes_the_validator() {
    let mut seen = 0;
    for entry in std::fs::read_dir(vectors_dir()).expect("test-vectors/ exists") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let document = read_json(&path);
        let signed = HEXLOWER
            .decode(field(&document, "signed_event_hex").as_bytes())
            .expect("signed_event_hex is hex");
        let body = HEXLOWER
            .decode(field(&document, "body_hex").as_bytes())
            .expect("body_hex is hex");
        validate::signed_event(&signed)
            .unwrap_or_else(|err| panic!("{} is rejected: {err}", path.display()));
        validate::event_body(&body)
            .unwrap_or_else(|err| panic!("{} body is rejected: {err}", path.display()));
        seen += 1;
    }
    assert!(seen >= 9, "expected the nine golden vectors, saw {seen}");
}

/// The events the signing path builds pass, including the acceptance blob it
/// signs.
#[test]
fn the_signing_path_produces_valid_events() {
    let s = scenario();
    for event in [
        &s.alice_inception,
        &s.bob_inception,
        &s.attestation,
        &s.org_inception,
        &s.org_invite,
        &s.org_acceptance,
        &s.org_removal,
    ] {
        validate::signed_event(&event.signed_event).expect("built events pass");
    }
    validate::acceptance(&s.acceptance_blob).expect("the acceptance blob passes");

    let witnesses = build_witness_config(
        &s.alice,
        &Position {
            ledger: s.alice_id,
            seq: 1,
            prev: s.alice_inception.event_id,
            prev_timestamp_ms: T0,
        },
        &[secret(0x77).public(), secret(0x78).public()],
        T0 + STEP_MS,
    )
    .expect("builds");
    validate::signed_event(&witnesses.signed_event).expect("a witness config passes");
}

/// Rewrites `test-vectors/rejections/`. The only writer; run it deliberately
/// and commit the diff for review.
#[cfg(feature = "gen-vectors")]
#[test]
#[ignore = "writes test-vectors/rejections/; run explicitly and review the diff"]
fn gen_rejections() {
    let dir = rejections_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("clear test-vectors/rejections/");
    }
    std::fs::create_dir_all(&dir).expect("create test-vectors/rejections/");
    for rejection in rejections() {
        let mut text = serde_json::to_string_pretty(&rejection.document()).expect("serializes");
        text.push('\n');
        std::fs::write(dir.join(&rejection.file), text).expect("write vector");
    }
}

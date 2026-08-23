//! Rejection vectors: one byte string per validator class, per stateless
//! field-table rule and per membership rule of the fold (proposal 001
//! sections 3.1, 3.4 and 11, proposal 002 sections 4 and 8).
//!
//! The files under `test-vectors/rejections/` are literals, exactly like the
//! golden vectors: the tests here read them and compare, and the only writer
//! is `gen_rejections`, which is `#[ignore]`d and gated behind the
//! `gen-vectors` feature:
//!
//! ```text
//! cargo test -p mabel-core --features gen-vectors -- --ignored gen_rejections
//! ```
//!
//! A vector whose `entry` is `signed_event` or `acceptance` carries one
//! `input_hex` and pins a stateless rejection. A vector whose `entry` is
//! `fold` carries `events_hex`, the whole chain, and pins the position and
//! reason of the first violation, because the rule it tests needs the folded
//! state.

use std::path::{Path, PathBuf};

use data_encoding::HEXLOWER;
use iroh_base::{PublicKey, SecretKey};
use mabel_core::fold::{Reason, Violation};
use mabel_core::validate::{self, WireError};
use mabel_core::{
    BuiltEvent, ID_BYTES, IdentityId, LedgerId, MAX_ACCEPTANCE_BYTES, MAX_EMBEDDED_INCEPTION_BYTES,
    MAX_EVENT_BYTES, MAX_TIMESTAMP_MS, MAX_WITNESSES, NONCE_BYTES, Position, Root, SIG_BYTES,
    build_acceptance, build_inception, build_membership_acceptance, build_membership_invitation,
    build_membership_removal, build_trust_attestation, build_witness_config, fold,
    proto::{DeclaredKind, Role},
    reserve_commit, sign_input,
};
use serde_json::{Value, json};

const T0: u64 = 1_700_000_000_000;
const STEP_MS: u64 = 60_000;

/// The `EventBody.payload` tags of proposal 002 section 7.
const INCEPTION: u32 = 10;
const WITNESS_CONFIG: u32 = 11;
const TRUST_ATTESTATION: u32 = 12;
const TRUST_REVOCATION: u32 = 13;
const MEMBERSHIP_INVITATION: u32 = 14;
const MEMBERSHIP_ACCEPTANCE: u32 = 15;
const MEMBERSHIP_REMOVAL: u32 = 16;

/// The `Inception.root` tags.
const RAW_ROOT: u32 = 10;
const IDENTITY_ROOT: u32 = 11;

/// One rejection vector: what to feed, where to feed it and what must come
/// back.
struct Rejection {
    file: String,
    class: &'static str,
    rule: &'static str,
    description: &'static str,
    expected: Expected,
}

/// What a vector asserts.
enum Expected {
    /// One byte string a validator entry point must reject.
    Wire {
        entry: Entry,
        input: Vec<u8>,
        error: WireError,
    },
    /// A chain the fold must reject at `at_seq`, because the rule needs the
    /// state folded from the events before it.
    Fold {
        events: Vec<Vec<u8>>,
        at_seq: u64,
        reason: Reason,
    },
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
        match &self.expected {
            Expected::Wire {
                entry,
                input,
                error,
            } => json!({
                "file": self.file,
                "class": self.class,
                "rule": self.rule,
                "description": self.description,
                "entry": entry.name(),
                "code": error.code(),
                "reason": error.to_string(),
                "input_hex": hex(input),
            }),
            Expected::Fold {
                events,
                at_seq,
                reason,
            } => json!({
                "file": self.file,
                "class": self.class,
                "rule": self.rule,
                "description": self.description,
                "entry": "fold",
                "at_seq": at_seq,
                "code": reason.code(),
                "reason": reason.to_string(),
                "events_hex": events.iter().map(|event| hex(event)).collect::<Vec<_>>(),
            }),
        }
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
    let signature = signer.sign(&sign_input(body)).to_bytes();
    let mut out = len_field(1, body);
    out.extend_from_slice(&len_field(2, &signature));
    out
}

fn raw_root_parts(active: &PublicKey, reserve: &PublicKey) -> Vec<Part> {
    vec![
        Part::L(1, active.as_bytes().to_vec()),
        Part::L(2, reserve_commit(reserve).to_vec()),
    ]
}

fn identity_root_parts(
    founder: IdentityId,
    founder_key: &PublicKey,
    founder_inception: &[u8],
) -> Vec<Part> {
    vec![
        Part::L(1, founder.to_vec()),
        Part::L(2, founder_key.as_bytes().to_vec()),
        Part::L(3, founder_inception.to_vec()),
    ]
}

/// An `Inception` around one root.
fn inception_parts(kind: u64, nonce: [u8; NONCE_BYTES], root_tag: u32, root: &[Part]) -> Vec<Part> {
    vec![
        Part::V(1, kind),
        Part::L(2, nonce.to_vec()),
        Part::L(root_tag, encode(root)),
    ]
}

fn membership_invitation_parts(
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

/// A `SignedEvent` carrying an identity-rooted `Inception` whose
/// `founder_inception` is `inner`: well formed enough to reach the
/// embedded-inception check, which is what recurses.
fn identity_root_around(inner: &[u8]) -> Vec<u8> {
    let author = secret(0x33).public();
    let root = identity_root_parts(IdentityId::from_bytes([8u8; ID_BYTES]), &author, inner);
    let payload = inception_parts(2, [9u8; NONCE_BYTES], IDENTITY_ROOT, &root);
    let body = encode(&inception_body(&author, INCEPTION, &payload));
    let mut signed = len_field(1, &body);
    signed.extend_from_slice(&len_field(2, &[0u8; SIG_BYTES]));
    signed
}

/// The scenario the vectors mutate: the golden-vector cast, so a reader can
/// diff a rejection against the valid event it came from.
struct Scenario {
    alice: SecretKey,
    bob: SecretKey,
    carol: SecretKey,
    alice_id: IdentityId,
    bob_id: IdentityId,
    carol_id: IdentityId,
    alice_inception: BuiltEvent,
    bob_inception: BuiltEvent,
    carol_inception: BuiltEvent,
    attestation: BuiltEvent,
    organization: BuiltEvent,
    organization_id: LedgerId,
    invitation: BuiltEvent,
    acceptance: BuiltEvent,
    acceptance_blob: Vec<u8>,
    acceptance_signature: [u8; SIG_BYTES],
}

fn raw_rooted(signer: &SecretKey, reserve: u8, nonce: u8) -> BuiltEvent {
    build_inception(
        signer,
        DeclaredKind::Person,
        Root::Raw {
            reserve_key: &secret(reserve).public(),
        },
        [nonce; NONCE_BYTES],
        T0,
    )
    .expect("builds")
}

fn scenario() -> Scenario {
    let alice = secret(0x11);
    let bob = secret(0x22);
    let carol = secret(0x33);
    let alice_inception = raw_rooted(&alice, 0x1a, 0xa1);
    let bob_inception = raw_rooted(&bob, 0x2a, 0xb1);
    let carol_inception = raw_rooted(&carol, 0x3a, 0xd1);
    let alice_id: IdentityId = alice_inception.event_id.into();
    let bob_id: IdentityId = bob_inception.event_id.into();
    let carol_id: IdentityId = carol_inception.event_id.into();

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

    let organization = build_inception(
        &alice,
        DeclaredKind::Organization,
        Root::Identity {
            founder: alice_id,
            founder_inception: &alice_inception.signed_event,
        },
        [0xc1; NONCE_BYTES],
        T0 + 4 * STEP_MS,
    )
    .expect("builds");
    let organization_id: LedgerId = organization.event_id.into();

    let invitation = build_membership_invitation(
        &alice,
        &Position {
            ledger: organization_id,
            seq: 1,
            prev: organization.event_id,
            prev_timestamp_ms: T0 + 4 * STEP_MS,
        },
        bob_id,
        &bob.public(),
        Role::Controller,
        &bob_inception.signed_event,
        T0 + 5 * STEP_MS,
    )
    .expect("builds");

    let accepted = build_acceptance(&bob, organization_id, invitation.event_id, bob_id);
    let acceptance = build_membership_acceptance(
        &alice,
        &Position {
            ledger: organization_id,
            seq: 2,
            prev: invitation.event_id,
            prev_timestamp_ms: T0 + 5 * STEP_MS,
        },
        &accepted,
        T0 + 6 * STEP_MS,
    )
    .expect("builds");

    Scenario {
        alice,
        bob,
        carol,
        alice_id,
        bob_id,
        carol_id,
        alice_inception,
        bob_inception,
        carol_inception,
        attestation,
        organization,
        organization_id,
        invitation,
        acceptance,
        acceptance_blob: accepted.acceptance,
        acceptance_signature: accepted.signature,
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
                    expected: Expected| {
        let file = format!("{:02}-{name}.json", cases.len() + 1);
        cases.push(Rejection {
            file,
            class,
            rule,
            description,
            expected,
        });
    };
    let wire = |entry: Entry, input: Vec<u8>, error: WireError| Expected::Wire {
        entry,
        input,
        error,
    };

    // The seven wire-format classes of proposal 001 section 3.1, plus
    // truncation, the caps and the nesting guard.

    let attestation_parts = || {
        append_body(
            s.alice_id,
            1,
            s.alice_inception.event_id.as_bytes(),
            &s.alice.public(),
            TRUST_ATTESTATION,
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
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::UnknownField {
                message: "EventBody",
                number: 7,
            },
        ),
    );

    let mut parts = attestation_parts();
    parts.insert(5, Part::L(6, s.alice.public().as_bytes().to_vec()));
    push(
        "duplicate-field",
        "wire-format",
        "3.1 duplicate non-repeated fields",
        "An EventBody carrying author_key twice.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::DuplicateField {
                message: "EventBody",
                field: "author_key",
            },
        ),
    );

    let mut parts = attestation_parts();
    parts.swap(3, 4);
    push(
        "field-out-of-order",
        "wire-format",
        "3.1 out-of-order fields",
        "An EventBody whose timestamp_ms follows author_key.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::FieldOutOfOrder {
                message: "EventBody",
                number: 5,
            },
        ),
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
        wire(
            Entry::SignedEvent,
            sign(&body, &s.alice),
            WireError::NonMinimalVarint {
                message: "EventBody",
            },
        ),
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
        wire(
            Entry::SignedEvent,
            sign(&body, &s.alice),
            WireError::VarintOverflow {
                message: "EventBody",
            },
        ),
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::V(6, 1));
    push(
        "wrong-wire-type",
        "wire-format",
        "3.1 wrong wire types",
        "An EventBody whose author_key arrives as a varint.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongWireType {
                message: "EventBody",
                field: "author_key",
                expected: 2,
                actual: 0,
            },
        ),
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, TRUST_ATTESTATION);
    parts.push(Part::L(17, encode(&[Part::L(1, s.bob_id.to_vec())])));
    push(
        "unknown-oneof-variant",
        "wire-format",
        "3.1 unrecognised oneof variants",
        "An EventBody whose payload uses tag 17, which v0 does not define.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::UnknownOneofVariant {
                message: "EventBody",
                oneof: "payload",
                number: 17,
            },
        ),
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, TRUST_ATTESTATION);
    parts.push(Part::L(20, encode(&[Part::L(1, s.bob_id.to_vec())])));
    push(
        "reserved-payload-tag",
        "wire-format",
        "002 section 9 reserved 20 to 29",
        "An EventBody whose payload uses tag 20, held for a deferred payload.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::UnknownOneofVariant {
                message: "EventBody",
                oneof: "payload",
                number: 20,
            },
        ),
    );

    let root = raw_root_parts(&s.alice.public(), &secret(0x1a).public());
    let mut payload = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    replace(&mut payload, Part::V(1, 0));
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "unspecified-enum",
        "wire-format",
        "3.1 *_UNSPECIFIED enum values",
        "An Inception whose kind is KIND_UNSPECIFIED.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::UnspecifiedEnum {
                message: "Inception",
                field: "kind",
            },
        ),
    );

    let mut truncated = key(1, 2);
    truncated.extend_from_slice(&varint(4000));
    truncated.extend_from_slice(&s.attestation.body[..8]);
    push(
        "truncated",
        "wire-format",
        "3.1 truncated input",
        "A SignedEvent whose body claims 4000 bytes and carries 8.",
        wire(
            Entry::SignedEvent,
            truncated,
            WireError::Truncated {
                message: "SignedEvent",
            },
        ),
    );

    let mut oversize = s.attestation.signed_event.clone();
    oversize.resize(MAX_EVENT_BYTES + 1, 0);
    push(
        "event-over-the-cap",
        "field-table",
        "3.4 SignedEvent <= 4096 bytes",
        "A SignedEvent one byte over the 4096-byte cap.",
        wire(
            Entry::SignedEvent,
            oversize,
            WireError::MessageTooLarge {
                message: "SignedEvent",
                len: MAX_EVENT_BYTES + 1,
                cap: MAX_EVENT_BYTES,
            },
        ),
    );

    push(
        "nesting-too-deep",
        "wire-format",
        "3.1 bounded work per message",
        "Three embedded inceptions nested inside one another.",
        wire(
            Entry::SignedEvent,
            identity_root_around(&identity_root_around(&identity_root_around(&[0xff; 8]))),
            WireError::TooDeeplyNested,
        ),
    );

    // The field table of proposal 001 section 3.4 and proposal 002 section 8,
    // row by row.

    let mut signed = len_field(1, &s.attestation.body);
    let signature = s.alice.sign(&sign_input(&s.attestation.body)).to_bytes();
    signed.extend_from_slice(&len_field(2, &signature[..SIG_BYTES - 1]));
    push(
        "signed-event-signature-length",
        "field-table",
        "3.4 SignedEvent.signature is 64 bytes",
        "A SignedEvent whose signature is 63 bytes.",
        wire(
            Entry::SignedEvent,
            signed,
            WireError::WrongLength {
                message: "SignedEvent",
                field: "signature",
                expected: SIG_BYTES,
                actual: SIG_BYTES - 1,
            },
        ),
    );

    push(
        "signed-event-body-missing",
        "field-table",
        "3.4 SignedEvent.body is required",
        "A SignedEvent carrying only a signature.",
        wire(
            Entry::SignedEvent,
            len_field(2, &signature),
            WireError::MissingField {
                message: "SignedEvent",
                field: "body",
            },
        ),
    );

    let mut parts = attestation_parts();
    parts.insert(0, Part::V(1, 1));
    push(
        "event-body-version-present",
        "field-table",
        "3.4 EventBody.version is absent",
        "An EventBody declaring version 1, which v0 rejects.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::FieldForbidden {
                message: "EventBody",
                field: "version",
            },
        ),
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::L(2, vec![0x5a; ID_BYTES - 1]));
    push(
        "event-body-ledger-length",
        "field-table",
        "3.4 EventBody.ledger is 32 bytes",
        "An EventBody whose ledger is 31 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "EventBody",
                field: "ledger",
                expected: ID_BYTES,
                actual: ID_BYTES - 1,
            },
        ),
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 4);
    push(
        "event-body-prev-missing",
        "field-table",
        "3.4 EventBody.prev is present past seq 0",
        "An event at seq 1 with no prev.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::MissingChainField { field: "prev" },
        ),
    );

    let payload = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    let mut parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    parts.insert(0, Part::L(2, s.alice_id.to_vec()));
    push(
        "event-body-ledger-at-seq-zero",
        "field-table",
        "3.4 EventBody.ledger is absent at seq 0",
        "An inception that also names a ledger.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::SetAtSeqZero { field: "ledger" },
        ),
    );

    let mut parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    parts.insert(0, Part::V(3, 0));
    push(
        "event-body-seq-zero-encoded",
        "field-table",
        "3.1 no proto3 default is serialized",
        "An inception that writes seq 0 instead of omitting it.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::DefaultValueEncoded {
                message: "EventBody",
                field: "seq",
            },
        ),
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, 5);
    push(
        "event-body-timestamp-missing",
        "field-table",
        "3.4 EventBody.timestamp_ms is required",
        "An event with no timestamp_ms.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::MissingField {
                message: "EventBody",
                field: "timestamp_ms",
            },
        ),
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::V(5, 0));
    push(
        "event-body-timestamp-zero",
        "field-table",
        "3.4 timestamp_ms in 1..=4102444800000",
        "An event whose timestamp_ms is 0, the proto3 default.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::DefaultValueEncoded {
                message: "EventBody",
                field: "timestamp_ms",
            },
        ),
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::V(5, MAX_TIMESTAMP_MS + 1));
    push(
        "event-body-timestamp-past-2100",
        "field-table",
        "3.4 timestamp_ms in 1..=4102444800000",
        "An event one millisecond past the year-2100 bound.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::ValueOutOfRange {
                message: "EventBody",
                field: "timestamp_ms",
                value: MAX_TIMESTAMP_MS + 1,
                min: 1,
                max: MAX_TIMESTAMP_MS,
            },
        ),
    );

    let mut parts = attestation_parts();
    replace(&mut parts, Part::L(6, vec![0x7c; ID_BYTES - 1]));
    push(
        "event-body-author-key-length",
        "field-table",
        "3.4 EventBody.author_key is 32 bytes",
        "An event whose author_key is 31 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "EventBody",
                field: "author_key",
                expected: ID_BYTES,
                actual: ID_BYTES - 1,
            },
        ),
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, TRUST_ATTESTATION);
    push(
        "event-body-payload-missing",
        "field-table",
        "3.4 EventBody.payload is exactly one recognised variant",
        "An event with no payload.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::MissingOneof {
                message: "EventBody",
                oneof: "payload",
            },
        ),
    );

    let mut parts = attestation_parts();
    parts.push(Part::L(
        TRUST_REVOCATION,
        encode(&[Part::L(1, s.attestation.event_id.to_vec())]),
    ));
    push(
        "event-body-two-payloads",
        "field-table",
        "3.4 EventBody.payload is exactly one recognised variant",
        "An event carrying both a trust_attestation and a trust_revocation.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::MultipleOneofVariants {
                message: "EventBody",
                oneof: "payload",
            },
        ),
    );

    let mut parts = attestation_parts();
    drop_part(&mut parts, TRUST_ATTESTATION);
    parts.push(Part::L(INCEPTION, encode(&payload)));
    push(
        "inception-past-seq-zero",
        "field-table",
        "3.4 an inception sits at seq 0",
        "An Inception payload at seq 1.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::InceptionPastSeqZero,
        ),
    );

    let parts = inception_body(
        &s.alice.public(),
        TRUST_ATTESTATION,
        &[Part::L(1, s.bob_id.to_vec())],
    );
    push(
        "non-inception-at-seq-zero",
        "field-table",
        "3.4 an inception sits at seq 0",
        "A TrustAttestation payload at seq 0.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::NonInceptionAtSeqZero,
        ),
    );

    // The inception rows of proposal 002 section 8.

    let mut payload_without_kind = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    drop_part(&mut payload_without_kind, 1);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload_without_kind);
    push(
        "inception-kind-absent",
        "field-table",
        "002 section 8 Inception.kind is a defined kind",
        "An Inception with no kind, which reads as KIND_UNSPECIFIED.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::UnspecifiedEnum {
                message: "Inception",
                field: "kind",
            },
        ),
    );

    let payload = inception_parts(5, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "inception-kind-unknown",
        "field-table",
        "002 section 8 Inception.kind is a defined kind",
        "An Inception whose kind is 5, past SERVICE, which v0 does not define.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::EnumValue {
                message: "Inception",
                field: "kind",
                value: 5,
            },
        ),
    );

    let mut payload_without_root = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    drop_part(&mut payload_without_root, RAW_ROOT);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload_without_root);
    push(
        "inception-no-root",
        "field-table",
        "002 section 8 Inception.root names exactly one variant",
        "An Inception carrying a kind and a nonce but no root.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::MissingOneof {
                message: "Inception",
                oneof: "root",
            },
        ),
    );

    let mut payload = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    replace(&mut payload, Part::L(2, vec![0xa1; NONCE_BYTES - 1]));
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "inception-nonce-length",
        "field-table",
        "002 section 8 Inception.nonce is 16 bytes",
        "An Inception whose nonce is 15 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "Inception",
                field: "nonce",
                expected: NONCE_BYTES,
                actual: NONCE_BYTES - 1,
            },
        ),
    );

    let mut commit_equals_key = root.clone();
    replace(
        &mut commit_equals_key,
        Part::L(2, s.alice.public().as_bytes().to_vec()),
    );
    let payload = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &commit_equals_key);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "raw-root-commit-equals-key",
        "field-table",
        "002 section 8 active_key and reserve_commit differ",
        "A RawRoot committing to its own active key.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::FieldsMustDiffer {
                first: "RawRoot.active_key",
                second: "RawRoot.reserve_commit",
            },
        ),
    );

    let payload = inception_parts(1, [0xa1; NONCE_BYTES], RAW_ROOT, &root);
    let parts = inception_body(&s.bob.public(), INCEPTION, &payload);
    push(
        "raw-root-author-key-mismatch",
        "field-table",
        "002 section 8 author_key at seq 0 equals the root key",
        "A raw-rooted inception whose author_key is not the active_key it records.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.bob),
            WireError::FieldsMustMatch {
                first: "EventBody.author_key",
                second: "RawRoot.active_key",
            },
        ),
    );

    let identity_root = identity_root_parts(
        s.alice_id,
        &s.alice.public(),
        &s.alice_inception.signed_event,
    );
    let payload = inception_parts(2, [0xc1; NONCE_BYTES], IDENTITY_ROOT, &identity_root);
    let parts = inception_body(&s.bob.public(), INCEPTION, &payload);
    push(
        "identity-root-author-key-mismatch",
        "field-table",
        "002 section 8 author_key at seq 0 equals the root key",
        "An identity-rooted inception signed by someone other than the founder.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.bob),
            WireError::FieldsMustMatch {
                first: "EventBody.author_key",
                second: "IdentityRoot.founder_key",
            },
        ),
    );

    let mismatched = build_inception(
        &s.alice,
        DeclaredKind::Organization,
        Root::Identity {
            founder: s.bob_id,
            founder_inception: &s.alice_inception.signed_event,
        },
        [0xc1; NONCE_BYTES],
        T0 + 4 * STEP_MS,
    )
    .expect("builds");
    push(
        "identity-root-founder-mismatch",
        "field-table",
        "002 section 8 the embedded inception hashes to the recorded id",
        "An IdentityRoot naming Bob as founder while embedding Alice's inception.",
        wire(
            Entry::SignedEvent,
            mismatched.signed_event,
            WireError::InceptionIdMismatch {
                field: "IdentityRoot.founder",
            },
        ),
    );

    let wrong_key =
        identity_root_parts(s.alice_id, &s.bob.public(), &s.alice_inception.signed_event);
    let payload = inception_parts(2, [0xc1; NONCE_BYTES], IDENTITY_ROOT, &wrong_key);
    let parts = inception_body(&s.bob.public(), INCEPTION, &payload);
    push(
        "identity-root-founder-key-mismatch",
        "field-table",
        "002 section 8 the embedded inception records the recorded key",
        "An IdentityRoot whose founder_key is not the embedded inception's active_key.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.bob),
            WireError::InceptionKeyMismatch {
                field: "IdentityRoot.founder_key",
            },
        ),
    );

    let not_an_inception =
        identity_root_parts(s.alice_id, &s.alice.public(), &s.attestation.signed_event);
    let payload = inception_parts(2, [0xc1; NONCE_BYTES], IDENTITY_ROOT, &not_an_inception);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "identity-root-embedded-not-raw-rooted",
        "field-table",
        "002 section 8 the embedded inception carries a raw root",
        "An IdentityRoot embedding an attestation instead of a raw-rooted inception.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::EmbeddedInceptionNotRawRooted,
        ),
    );

    // An organization controlling an organization is deferred (proposal 002
    // section 9). Two levels of embedding fit; a third does not, so the depth
    // guard is what closes the door.
    let nested = identity_root_parts(
        s.organization_id,
        &s.alice.public(),
        &s.organization.signed_event,
    );
    let payload = inception_parts(2, [0xc1; NONCE_BYTES], IDENTITY_ROOT, &nested);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "identity-root-embedded-identity-rooted",
        "field-table",
        "002 section 9 identity principals nest no deeper than one level",
        "An IdentityRoot embedding an identity-rooted inception, which nests past the \
         scanner's depth guard.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::TooDeeplyNested,
        ),
    );

    let mut broken = s.alice_inception.signed_event.clone();
    let last = broken.len() - 1;
    broken[last] ^= 0x01;
    let tampered = identity_root_parts(s.alice_id, &s.alice.public(), &broken);
    let payload = inception_parts(2, [0xc1; NONCE_BYTES], IDENTITY_ROOT, &tampered);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "identity-root-embedded-bad-signature",
        "field-table",
        "002 section 8 the embedded inception verifies standalone",
        "An IdentityRoot whose embedded inception has one signature bit flipped.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::BadSignature {
                message: "SignedEvent",
                field: "signature",
            },
        ),
    );

    let oversize_inception = [0x11u8; MAX_EMBEDDED_INCEPTION_BYTES + 1];
    let too_long = identity_root_parts(s.alice_id, &s.alice.public(), &oversize_inception);
    let payload = inception_parts(2, [0xc1; NONCE_BYTES], IDENTITY_ROOT, &too_long);
    let parts = inception_body(&s.alice.public(), INCEPTION, &payload);
    push(
        "identity-root-embedded-too-long",
        "field-table",
        "002 section 8 founder_inception <= 1024 bytes",
        "An IdentityRoot whose founder_inception is one byte over the cap.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::FieldTooLong {
                message: "IdentityRoot",
                field: "founder_inception",
                len: MAX_EMBEDDED_INCEPTION_BYTES + 1,
                cap: MAX_EMBEDDED_INCEPTION_BYTES,
            },
        ),
    );

    // WitnessConfig, TrustAttestation and TrustRevocation, unchanged rows.

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        WITNESS_CONFIG,
        &[],
    );
    push(
        "witness-config-empty",
        "field-table",
        "3.4 WitnessConfig holds 1 to 16 witnesses",
        "A WitnessConfig naming no witnesses.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::RepeatedCount {
                message: "WitnessConfig",
                field: "witnesses",
                count: 0,
                min: 1,
                max: MAX_WITNESSES,
            },
        ),
    );

    let many: Vec<Part> = (0..=MAX_WITNESSES as u8)
        .map(|seed| Part::L(1, secret(0x80 + seed).public().as_bytes().to_vec()))
        .collect();
    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        WITNESS_CONFIG,
        &many,
    );
    push(
        "witness-config-seventeen",
        "field-table",
        "3.4 WitnessConfig holds 1 to 16 witnesses",
        "A WitnessConfig naming 17 witnesses.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::RepeatedCount {
                message: "WitnessConfig",
                field: "witnesses",
                count: MAX_WITNESSES + 1,
                min: 1,
                max: MAX_WITNESSES,
            },
        ),
    );

    let witness = secret(0x77).public().as_bytes().to_vec();
    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        WITNESS_CONFIG,
        &[Part::L(1, witness.clone()), Part::L(1, witness)],
    );
    push(
        "witness-config-duplicate",
        "field-table",
        "3.4 witnesses are distinct",
        "A WitnessConfig naming the same endpoint twice.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::RepeatedDuplicate {
                message: "WitnessConfig",
                field: "witnesses",
            },
        ),
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        WITNESS_CONFIG,
        &[Part::L(1, vec![0x77; ID_BYTES - 1])],
    );
    push(
        "witness-config-witness-length",
        "field-table",
        "3.4 each witness is 32 bytes",
        "A WitnessConfig whose witness is 31 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "WitnessConfig",
                field: "witnesses",
                expected: ID_BYTES,
                actual: ID_BYTES - 1,
            },
        ),
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        TRUST_ATTESTATION,
        &[Part::L(1, s.alice_id.to_vec())],
    );
    push(
        "trust-attestation-subject-is-issuer",
        "field-table",
        "3.4 TrustAttestation.subject differs from the ledger id",
        "An attestation whose subject is the issuing ledger.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::FieldsMustDiffer {
                first: "TrustAttestation.subject",
                second: "EventBody.ledger",
            },
        ),
    );

    let parts = append_body(
        s.alice_id,
        1,
        s.alice_inception.event_id.as_bytes(),
        &s.alice.public(),
        TRUST_ATTESTATION,
        &[Part::L(1, vec![0x4d; ID_BYTES + 1])],
    );
    push(
        "trust-attestation-subject-length",
        "field-table",
        "3.4 TrustAttestation.subject is 32 bytes",
        "An attestation whose subject is 33 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "TrustAttestation",
                field: "subject",
                expected: ID_BYTES,
                actual: ID_BYTES + 1,
            },
        ),
    );

    let parts = append_body(
        s.alice_id,
        2,
        s.attestation.event_id.as_bytes(),
        &s.alice.public(),
        TRUST_REVOCATION,
        &[Part::L(1, vec![0xf7; ID_BYTES - 1])],
    );
    push(
        "trust-revocation-target-length",
        "field-table",
        "3.4 TrustRevocation.target is 32 bytes",
        "A revocation whose target is 31 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "TrustRevocation",
                field: "target",
                expected: ID_BYTES,
                actual: ID_BYTES - 1,
            },
        ),
    );

    // The membership rows of proposal 002 section 8.

    let mut payload =
        membership_invitation_parts(s.bob_id, &s.bob.public(), 2, &s.bob_inception.signed_event);
    drop_part(&mut payload, 3);
    let parts = append_body(
        s.organization_id,
        1,
        s.organization.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_INVITATION,
        &payload,
    );
    push(
        "membership-invitation-role-unspecified",
        "field-table",
        "002 section 8 MembershipInvitation.role is MEMBER or CONTROLLER",
        "An invitation with no role, which reads as ROLE_UNSPECIFIED.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::UnspecifiedEnum {
                message: "MembershipInvitation",
                field: "role",
            },
        ),
    );

    let payload =
        membership_invitation_parts(s.bob_id, &s.bob.public(), 3, &s.bob_inception.signed_event);
    let parts = append_body(
        s.organization_id,
        1,
        s.organization.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_INVITATION,
        &payload,
    );
    push(
        "membership-invitation-role-unknown",
        "field-table",
        "002 section 8 MembershipInvitation.role is MEMBER or CONTROLLER",
        "An invitation whose role is 3, the slot proposal 002 section 9 holds for narrower \
         capabilities.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::EnumValue {
                message: "MembershipInvitation",
                field: "role",
                value: 3,
            },
        ),
    );

    let payload = membership_invitation_parts(
        s.alice_id,
        &s.bob.public(),
        2,
        &s.bob_inception.signed_event,
    );
    let parts = append_body(
        s.organization_id,
        1,
        s.organization.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_INVITATION,
        &payload,
    );
    push(
        "membership-invitation-invitee-mismatch",
        "field-table",
        "002 section 8 the embedded inception hashes to the recorded id",
        "An invitation naming Alice while embedding Bob's inception.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::InceptionIdMismatch {
                field: "MembershipInvitation.invitee",
            },
        ),
    );

    let payload = membership_invitation_parts(
        s.bob_id,
        &s.alice.public(),
        2,
        &s.bob_inception.signed_event,
    );
    let parts = append_body(
        s.organization_id,
        1,
        s.organization.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_INVITATION,
        &payload,
    );
    push(
        "membership-invitation-invitee-key-mismatch",
        "field-table",
        "002 section 8 the embedded inception records the recorded key",
        "An invitation whose invitee_key is not the embedded inception's active_key.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::InceptionKeyMismatch {
                field: "MembershipInvitation.invitee_key",
            },
        ),
    );

    // The invitee is the ledger itself: on a raw-rooted ledger that would
    // shadow the root principal, so it is refused on every ledger.
    let self_invitation = build_membership_invitation(
        &s.alice,
        &Position {
            ledger: s.alice_id,
            seq: 1,
            prev: s.alice_inception.event_id,
            prev_timestamp_ms: T0,
        },
        s.alice_id,
        &s.alice.public(),
        Role::Controller,
        &s.alice_inception.signed_event,
        T0 + STEP_MS,
    )
    .expect("builds");
    push(
        "membership-invitation-invitee-is-the-ledger",
        "field-table",
        "002 section 4 invitee differs from the ledger id",
        "A raw-rooted ledger inviting itself, which would shadow its root principal.",
        wire(
            Entry::SignedEvent,
            self_invitation.signed_event,
            WireError::FieldsMustDiffer {
                first: "MembershipInvitation.invitee",
                second: "EventBody.ledger",
            },
        ),
    );

    let parts = append_body(
        s.organization_id,
        2,
        s.invitation.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_ACCEPTANCE,
        &[
            Part::L(1, s.acceptance_blob.clone()),
            Part::L(2, s.acceptance_signature[..SIG_BYTES - 1].to_vec()),
        ],
    );
    push(
        "membership-acceptance-signature-length",
        "field-table",
        "002 section 8 MembershipAcceptance.signature is 64 bytes",
        "An acceptance whose invitee signature is 63 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "MembershipAcceptance",
                field: "signature",
                expected: SIG_BYTES,
                actual: SIG_BYTES - 1,
            },
        ),
    );

    let mut signature = s.acceptance_signature;
    signature[0] ^= 0x01;
    let parts = append_body(
        s.organization_id,
        2,
        s.invitation.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_ACCEPTANCE,
        &[
            Part::L(1, s.acceptance_blob.clone()),
            Part::L(2, signature.to_vec()),
        ],
    );
    push(
        "membership-acceptance-bad-signature",
        "field-table",
        "3.5 the signature verifies over accept_input under invitee_key",
        "An acceptance whose invitee signature has one bit flipped.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::BadSignature {
                message: "MembershipAcceptance",
                field: "signature",
            },
        ),
    );

    // 0x02 repeated is 32 bytes that do not decompress to a curve point.
    let blob = encode(&[
        Part::L(2, s.organization_id.to_vec()),
        Part::L(3, s.invitation.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
        Part::L(5, vec![0x02; ID_BYTES]),
    ]);
    let parts = append_body(
        s.organization_id,
        2,
        s.invitation.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_ACCEPTANCE,
        &[
            Part::L(1, blob),
            Part::L(2, s.acceptance_signature.to_vec()),
        ],
    );
    push(
        "acceptance-invitee-key-not-a-point",
        "field-table",
        "3.5 invitee_key is an ed25519 public key",
        "An Acceptance blob whose invitee_key is 32 bytes but not a curve point.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::InvalidPublicKey {
                message: "Acceptance",
                field: "invitee_key",
            },
        ),
    );

    let parts = append_body(
        s.organization_id,
        2,
        s.invitation.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_ACCEPTANCE,
        &[
            Part::L(1, vec![0x2b; MAX_ACCEPTANCE_BYTES + 1]),
            Part::L(2, s.acceptance_signature.to_vec()),
        ],
    );
    push(
        "membership-acceptance-blob-too-long",
        "field-table",
        "002 section 8 MembershipAcceptance.acceptance <= 1024 bytes",
        "An acceptance blob one byte over the cap.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::FieldTooLong {
                message: "MembershipAcceptance",
                field: "acceptance",
                len: MAX_ACCEPTANCE_BYTES + 1,
                cap: MAX_ACCEPTANCE_BYTES,
            },
        ),
    );

    let mut blob = vec![Part::V(1, 1)];
    blob.extend_from_slice(&[
        Part::L(2, s.organization_id.to_vec()),
        Part::L(3, s.invitation.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
        Part::L(5, s.bob.public().as_bytes().to_vec()),
    ]);
    push(
        "acceptance-version-present",
        "field-table",
        "002 section 8 Acceptance.version is absent",
        "An Acceptance blob declaring version 1.",
        wire(
            Entry::Acceptance,
            encode(&blob),
            WireError::FieldForbidden {
                message: "Acceptance",
                field: "version",
            },
        ),
    );

    let blob = vec![
        Part::L(2, vec![0x0c; ID_BYTES - 1]),
        Part::L(3, s.invitation.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
        Part::L(5, s.bob.public().as_bytes().to_vec()),
    ];
    push(
        "acceptance-ledger-length",
        "field-table",
        "002 section 8 Acceptance fields are 32 bytes each",
        "An Acceptance blob whose ledger is 31 bytes.",
        wire(
            Entry::Acceptance,
            encode(&blob),
            WireError::WrongLength {
                message: "Acceptance",
                field: "ledger",
                expected: ID_BYTES,
                actual: ID_BYTES - 1,
            },
        ),
    );

    let blob = vec![
        Part::L(2, s.organization_id.to_vec()),
        Part::L(3, s.invitation.event_id.to_vec()),
        Part::L(4, s.bob_id.to_vec()),
    ];
    push(
        "acceptance-invitee-key-missing",
        "field-table",
        "002 section 8 Acceptance fields are required",
        "An Acceptance blob with no invitee_key.",
        wire(
            Entry::Acceptance,
            encode(&blob),
            WireError::MissingField {
                message: "Acceptance",
                field: "invitee_key",
            },
        ),
    );

    let parts = append_body(
        s.organization_id,
        3,
        s.acceptance.event_id.as_bytes(),
        &s.alice.public(),
        MEMBERSHIP_REMOVAL,
        &[Part::L(1, vec![0x22; ID_BYTES + 1])],
    );
    push(
        "membership-removal-target-length",
        "field-table",
        "002 section 8 MembershipRemoval.target is 32 bytes",
        "A removal whose target is 33 bytes.",
        wire(
            Entry::SignedEvent,
            sign(&encode(&parts), &s.alice),
            WireError::WrongLength {
                message: "MembershipRemoval",
                field: "target",
                expected: ID_BYTES,
                actual: ID_BYTES + 1,
            },
        ),
    );

    // The membership rules of proposal 002 section 4, which need the folded
    // state. Each vector carries the whole chain.

    // Alice's raw-rooted ledger, with Bob invited as a controller at seq 1.
    let alice_invitation = build_membership_invitation(
        &s.alice,
        &Position {
            ledger: s.alice_id,
            seq: 1,
            prev: s.alice_inception.event_id,
            prev_timestamp_ms: T0,
        },
        s.bob_id,
        &s.bob.public(),
        Role::Controller,
        &s.bob_inception.signed_event,
        T0 + STEP_MS,
    )
    .expect("builds");
    let admit = |accepted: &mabel_core::DetachedAcceptance| {
        build_membership_acceptance(
            &s.alice,
            &Position {
                ledger: s.alice_id,
                seq: 2,
                prev: alice_invitation.event_id,
                prev_timestamp_ms: T0 + STEP_MS,
            },
            accepted,
            T0 + 2 * STEP_MS,
        )
        .expect("builds")
    };
    let invited_chain = |acceptance: &BuiltEvent| {
        vec![
            s.alice_inception.signed_event.clone(),
            alice_invitation.signed_event.clone(),
            acceptance.signed_event.clone(),
        ]
    };

    let elsewhere = admit(&build_acceptance(
        &s.bob,
        s.organization_id,
        alice_invitation.event_id,
        s.bob_id,
    ));
    push(
        "acceptance-transplanted-from-another-ledger",
        "fold",
        "002 section 4 Acceptance.ledger equals this ledger id",
        "An acceptance Bob signed for the organization, replayed on Alice's ledger.",
        Expected::Fold {
            events: invited_chain(&elsewhere),
            at_seq: 2,
            reason: Reason::AcceptanceForAnotherLedger {
                named: s.organization_id,
                expected: s.alice_id,
            },
        },
    );

    let unknown = mabel_core::EventId::from_bytes([0xee; ID_BYTES]);
    let other_invitation = admit(&build_acceptance(&s.bob, s.alice_id, unknown, s.bob_id));
    push(
        "acceptance-transplanted-from-another-invitation",
        "fold",
        "002 section 4 invitation_event names an open invitation",
        "An acceptance naming an invitation event this ledger does not hold.",
        Expected::Fold {
            events: invited_chain(&other_invitation),
            at_seq: 2,
            reason: Reason::UnknownInvitation(unknown),
        },
    );

    let other_identity = admit(&build_acceptance(
        &s.carol,
        s.alice_id,
        alice_invitation.event_id,
        s.carol_id,
    ));
    push(
        "acceptance-names-another-identity",
        "fold",
        "002 section 4 the acceptance matches the invitation it names",
        "Carol signing an acceptance for the invitation that named Bob.",
        Expected::Fold {
            events: invited_chain(&other_identity),
            at_seq: 2,
            reason: Reason::AcceptanceInviteeMismatch {
                named: s.carol_id,
                invited: s.bob_id,
            },
        },
    );

    let other_key = admit(&build_acceptance(
        &s.carol,
        s.alice_id,
        alice_invitation.event_id,
        s.bob_id,
    ));
    push(
        "acceptance-names-another-key",
        "fold",
        "002 section 4 the acceptance matches the invitation it names",
        "An acceptance naming Bob as invitee but signed by a key the invitation does not \
         record.",
        Expected::Fold {
            events: invited_chain(&other_key),
            at_seq: 2,
            reason: Reason::AcceptanceInviteeKeyMismatch {
                named: s.carol.public(),
                invited: s.bob.public(),
            },
        },
    );

    // A second inception under the same key: a different identity id holding
    // a key the root principal already holds.
    let twin = raw_rooted(&s.alice, 0x1a, 0xa2);
    let twin_id: IdentityId = twin.event_id.into();
    let twin_invitation = build_membership_invitation(
        &s.alice,
        &Position {
            ledger: s.alice_id,
            seq: 1,
            prev: s.alice_inception.event_id,
            prev_timestamp_ms: T0,
        },
        twin_id,
        &s.alice.public(),
        Role::Member,
        &twin.signed_event,
        T0 + STEP_MS,
    )
    .expect("builds");
    let twin_acceptance = build_membership_acceptance(
        &s.alice,
        &Position {
            ledger: s.alice_id,
            seq: 2,
            prev: twin_invitation.event_id,
            prev_timestamp_ms: T0 + STEP_MS,
        },
        &build_acceptance(&s.alice, s.alice_id, twin_invitation.event_id, twin_id),
        T0 + 2 * STEP_MS,
    )
    .expect("builds");
    push(
        "duplicate-principal-key",
        "fold",
        "002 section 4 duplicate keys are rejected at admission",
        "Admitting a second identity whose active key the root principal already holds.",
        Expected::Fold {
            events: vec![
                s.alice_inception.signed_event.clone(),
                twin_invitation.signed_event,
                twin_acceptance.signed_event,
            ],
            at_seq: 2,
            reason: Reason::DuplicatePrincipalKey {
                key: s.alice.public(),
                held_by: s.alice_id,
            },
        },
    );

    let remove_root = build_membership_removal(
        &s.alice,
        &Position {
            ledger: s.alice_id,
            seq: 1,
            prev: s.alice_inception.event_id,
            prev_timestamp_ms: T0,
        },
        s.alice_id,
        T0 + STEP_MS,
    )
    .expect("builds");
    push(
        "raw-root-removal",
        "fold",
        "002 section 4 the raw root is never removable",
        "A raw-rooted ledger removing its own root principal.",
        Expected::Fold {
            events: vec![
                s.alice_inception.signed_event.clone(),
                remove_root.signed_event,
            ],
            at_seq: 1,
            reason: Reason::RootNotRemovable(s.alice_id),
        },
    );

    let remove_founder = build_membership_removal(
        &s.alice,
        &Position {
            ledger: s.organization_id,
            seq: 1,
            prev: s.organization.event_id,
            prev_timestamp_ms: T0 + 4 * STEP_MS,
        },
        s.alice_id,
        T0 + 5 * STEP_MS,
    )
    .expect("builds");
    push(
        "removal-leaving-no-controller",
        "fold",
        "002 section 4 a removal leaves at least one controller",
        "An identity-rooted ledger removing its only controller, the founder.",
        Expected::Fold {
            events: vec![
                s.organization.signed_event.clone(),
                remove_founder.signed_event,
            ],
            at_seq: 1,
            reason: Reason::LastController(s.alice_id),
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

fn decode_hex(text: &str) -> Vec<u8> {
    HEXLOWER.decode(text.as_bytes()).expect("the field is hex")
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
        let (code, reason) = match field(&document, "entry").as_str() {
            entry @ ("signed_event" | "acceptance") => {
                let entry = if entry == "signed_event" {
                    Entry::SignedEvent
                } else {
                    Entry::Acceptance
                };
                let input = decode_hex(&field(&document, "input_hex"));
                let error = entry
                    .run(&input)
                    .expect_err(&format!("{} must be rejected", rejection.file));
                (error.code(), error.to_string())
            }
            "fold" => {
                let events: Vec<Vec<u8>> = document["events_hex"]
                    .as_array()
                    .expect("events_hex is an array")
                    .iter()
                    .map(|event| decode_hex(event.as_str().expect("an event is hex")))
                    .collect();
                let violation = fold(&events)
                    .1
                    .unwrap_or_else(|| panic!("{} must be rejected", rejection.file));
                let at_seq = document["at_seq"].as_u64().expect("at_seq is a number");
                assert_eq!(violation.seq, at_seq, "{}", rejection.file);
                let Violation { seq: _, reason } = violation;
                (reason.code(), reason.to_string())
            }
            other => panic!("unknown entry point {other}"),
        };
        assert_eq!(code, field(&document, "code"), "{}", rejection.file);
        assert_eq!(reason, field(&document, "reason"), "{}", rejection.file);
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
        let signed = decode_hex(&field(&document, "signed_event_hex"));
        let body = decode_hex(&field(&document, "body_hex"));
        validate::signed_event(&signed)
            .unwrap_or_else(|err| panic!("{} is rejected: {err}", path.display()));
        validate::event_body(&body)
            .unwrap_or_else(|err| panic!("{} body is rejected: {err}", path.display()));
        seen += 1;
    }
    assert!(seen >= 11, "expected the eleven golden vectors, saw {seen}");
}

/// The events the signing path builds pass, including the acceptance blob it
/// signs.
#[test]
fn the_signing_path_produces_valid_events() {
    let s = scenario();
    for event in [
        &s.alice_inception,
        &s.bob_inception,
        &s.carol_inception,
        &s.attestation,
        &s.organization,
        &s.invitation,
        &s.acceptance,
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

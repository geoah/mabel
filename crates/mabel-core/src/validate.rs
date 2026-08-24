//! The stateless gate every received byte string passes before any semantic
//! rule runs (proposal 001 sections 3.1 and 3.4).
//!
//! Two layers, one pass:
//!
//! 1. The wire-format validator scans the received bytes directly, before and
//!    independently of prost decoding, and rejects unknown field numbers,
//!    duplicate non-repeated fields, out-of-order fields, non-minimal
//!    varints, wrong wire types, unrecognised `oneof` variants,
//!    `*_UNSPECIFIED` enum values and truncated input.
//! 2. The stateless rows of the field table: presence, exact byte lengths,
//!    value ranges, uniqueness, enum agreement, the intra-message cross-field
//!    rules and the standalone check of an embedded inception.
//!
//! Both layers are driven by [`MessageDescriptor`]s, so the sync frames of
//! section 5 and the file artifacts of section 3.8 register their own
//! descriptors and call [`message`].
//!
//! The scanner never allocates in proportion to a claimed length: every
//! length-delimited field is bounds-checked against the remaining input and
//! then borrowed as a slice.
//!
//! What is *not* checked here, because it needs the folded state: the
//! `author_key` authorization and the outer event signature, the
//! `ledger`/`prev`/`seq` chain equalities, `TrustRevocation.target` liveness,
//! the invitation an acceptance names, and `MembershipRemoval.target`
//! validity.

use iroh_base::{PublicKey, Signature};

use crate::digest::{accept_input, event_id, sign_input};
use crate::id::{EventId, IdentityId};
use crate::{
    ID_BYTES, MAX_ACCEPTANCE_BYTES, MAX_DISPLAY_NAME_BYTES, MAX_EMAIL_BYTES,
    MAX_EMBEDDED_INCEPTION_BYTES, MAX_ENDPOINTS, MAX_EVENT_BYTES, MAX_HOSTNAME_BYTES,
    MAX_HOSTNAME_LABEL_BYTES, MAX_TIMESTAMP_MS, MAX_WITNESSES, NONCE_BYTES, SIG_BYTES,
};

/// How deep messages may nest before the scanner gives up.
///
/// A legitimate event reaches depth 7: `SignedEvent`, `EventBody`,
/// `Inception`, `IdentityRoot`, then the embedded `SignedEvent`, its
/// `EventBody`, its `Inception` and that inception's `RawRoot`.
pub const MAX_NESTING: u32 = 8;

/// The `oneof payload` tag of `Inception`.
const INCEPTION_TAG: u32 = 10;
/// The `oneof payload` tag of `TrustAttestation`.
const TRUST_ATTESTATION_TAG: u32 = 12;
/// The `oneof payload` tag of `MembershipInvitation`.
const MEMBERSHIP_INVITATION_TAG: u32 = 14;
/// The `oneof payload` tag of `ProfileUpdate`.
const PROFILE_UPDATE_TAG: u32 = 17;
/// The `oneof root` tag of `RawRoot`.
const RAW_ROOT_TAG: u32 = 10;
/// The `oneof root` tag of `IdentityRoot`.
const IDENTITY_ROOT_TAG: u32 = 11;

/// Why a byte string is not a valid message.
///
/// Every variant names the message type and, where one applies, the field, so
/// a rejection vector can pin the exact reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WireError {
    /// The encoded message exceeded its size cap.
    #[error("{message} is {len} bytes, over the {cap}-byte cap")]
    MessageTooLarge {
        /// The message type.
        message: &'static str,
        /// The length of the input.
        len: usize,
        /// The cap the input exceeded.
        cap: usize,
    },
    /// The input ended in the middle of a record.
    #[error("{message} ends mid-record")]
    Truncated {
        /// The message type.
        message: &'static str,
    },
    /// A varint carried padding bytes, which the canonical encoding forbids.
    #[error("{message} holds a varint with a non-minimal encoding")]
    NonMinimalVarint {
        /// The message type.
        message: &'static str,
    },
    /// A varint did not fit in 64 bits.
    #[error("{message} holds a varint wider than 64 bits")]
    VarintOverflow {
        /// The message type.
        message: &'static str,
    },
    /// A field number the schema does not declare.
    #[error("unknown field number {number} in {message}")]
    UnknownField {
        /// The message type.
        message: &'static str,
        /// The field number found.
        number: u32,
    },
    /// A field number inside the `oneof` range that this version does not
    /// know, which a later version may have added.
    #[error("unrecognised {message}.{oneof} variant, field number {number}")]
    UnknownOneofVariant {
        /// The message type.
        message: &'static str,
        /// The `oneof` name.
        oneof: &'static str,
        /// The field number found.
        number: u32,
    },
    /// A field carried a wire type other than the one its type requires.
    #[error("{message}.{field} has wire type {actual}, expected {expected}")]
    WrongWireType {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// The wire type the schema requires.
        expected: u8,
        /// The wire type found.
        actual: u8,
    },
    /// A non-repeated field appeared more than once.
    #[error("{message}.{field} appears more than once")]
    DuplicateField {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// A field appeared before a field with a lower number, or a repeated
    /// field's entries were not consecutive.
    #[error("{message} field number {number} is out of ascending order")]
    FieldOutOfOrder {
        /// The message type.
        message: &'static str,
        /// The field number found out of order.
        number: u32,
    },
    /// A field was serialized with its proto3 default value, which the
    /// canonical encoding omits.
    #[error("{message}.{field} holds its proto3 default, which the canonical encoding omits")]
    DefaultValueEncoded {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// A field the field table requires to be absent was present.
    #[error("{message}.{field} must be absent")]
    FieldForbidden {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// A required field was absent.
    #[error("{message}.{field} is required")]
    MissingField {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// A required `oneof` named no variant.
    #[error("{message}.{oneof} names no variant")]
    MissingOneof {
        /// The message type.
        message: &'static str,
        /// The `oneof` name.
        oneof: &'static str,
    },
    /// A `oneof` named more than one variant.
    #[error("{message}.{oneof} names more than one variant")]
    MultipleOneofVariants {
        /// The message type.
        message: &'static str,
        /// The `oneof` name.
        oneof: &'static str,
    },
    /// An enum field held, or defaulted to, its `*_UNSPECIFIED` value.
    #[error("{message}.{field} is unspecified")]
    UnspecifiedEnum {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// An enum field held a value this field does not accept.
    #[error("{message}.{field} holds {value}, which is not a value it accepts")]
    EnumValue {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// The value found.
        value: u64,
    },
    /// A `bytes` field was not the exact length the field table states.
    #[error("{message}.{field} is {actual} bytes, expected exactly {expected}")]
    WrongLength {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// The length the field table states.
        expected: usize,
        /// The length found.
        actual: usize,
    },
    /// A `bytes` field exceeded its cap.
    #[error("{message}.{field} is {len} bytes, over the {cap}-byte cap")]
    FieldTooLong {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// The length found.
        len: usize,
        /// The cap the field exceeded.
        cap: usize,
    },
    /// A `string` field held bytes that are not UTF-8.
    #[error("{message}.{field} is not valid UTF-8")]
    InvalidUtf8 {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// A display name broke the codepoint policy of proposal 003 section 1.
    #[error("{message}.{field} is not an acceptable display name: {reason}")]
    InvalidDisplayName {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// Which rule the value broke.
        reason: &'static str,
    },
    /// A hostname broke the syntax of proposal 003 section 2.
    #[error("{message}.{field} is not a valid hostname: {reason}")]
    InvalidHostname {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// Which rule the value broke.
        reason: &'static str,
    },
    /// An email broke the rule of proposal 005.
    #[error("{message}.{field} is not a valid email: {reason}")]
    InvalidEmail {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// Which rule the value broke.
        reason: &'static str,
    },
    /// A numeric field fell outside the range the field table states.
    #[error("{message}.{field} holds {value}, outside {min}..={max}")]
    ValueOutOfRange {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// The value found.
        value: u64,
        /// The lowest accepted value.
        min: u64,
        /// The highest accepted value.
        max: u64,
    },
    /// A repeated field held too few or too many entries.
    #[error("{message}.{field} holds {count} entries, outside {min}..={max}")]
    RepeatedCount {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
        /// The number of entries found.
        count: usize,
        /// The fewest accepted entries.
        min: usize,
        /// The most accepted entries.
        max: usize,
    },
    /// A repeated field whose entries must be distinct repeated one.
    #[error("{message}.{field} repeats an entry")]
    RepeatedDuplicate {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// Two fields the field table requires to differ were equal.
    #[error("{first} and {second} must differ")]
    FieldsMustDiffer {
        /// The first field, qualified by its message type.
        first: &'static str,
        /// The second field, qualified by its message type.
        second: &'static str,
    },
    /// Two fields the field table requires to be equal differed.
    #[error("{first} and {second} must be equal")]
    FieldsMustMatch {
        /// The first field, qualified by its message type.
        first: &'static str,
        /// The second field, qualified by its message type.
        second: &'static str,
    },
    /// `ledger` or `prev` was present in a seq-0 event.
    #[error("EventBody.{field} must be absent at seq 0")]
    SetAtSeqZero {
        /// The field name.
        field: &'static str,
    },
    /// `ledger` or `prev` was absent from an event past seq 0.
    #[error("EventBody.{field} is required past seq 0")]
    MissingChainField {
        /// The field name.
        field: &'static str,
    },
    /// An inception payload sat at a sequence other than 0.
    #[error("an inception payload requires seq 0")]
    InceptionPastSeqZero,
    /// A seq-0 event carried a payload other than an inception.
    #[error("seq 0 requires an inception payload")]
    NonInceptionAtSeqZero,
    /// An embedded inception was not a raw-rooted seq-0 event, so it names no
    /// key of its own (proposal 002 section 2).
    #[error("the embedded inception is not a raw-rooted inception")]
    EmbeddedInceptionNotRawRooted,
    /// An embedded inception did not hash to the identity recorded beside it.
    #[error("{field} does not equal the event id of the inception embedded beside it")]
    InceptionIdMismatch {
        /// The recorded field, qualified by its message type.
        field: &'static str,
    },
    /// An embedded inception's active key was not the key recorded beside it.
    #[error("{field} does not equal the active key of the inception embedded beside it")]
    InceptionKeyMismatch {
        /// The recorded field, qualified by its message type.
        field: &'static str,
    },
    /// A 32-byte field that must be a public key was not a curve point.
    #[error("{message}.{field} is not a valid ed25519 public key")]
    InvalidPublicKey {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// A signature did not verify over the bytes it covers.
    #[error("{message}.{field} does not verify")]
    BadSignature {
        /// The message type.
        message: &'static str,
        /// The field name.
        field: &'static str,
    },
    /// Messages nested deeper than [`MAX_NESTING`].
    #[error("a message nests more than 8 levels deep")]
    TooDeeplyNested,
}

impl WireError {
    /// A stable snake-case name for this rejection class.
    ///
    /// Rejection vectors carry this code, so an implementation in another
    /// language can assert the class without matching English prose.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MessageTooLarge { .. } => "message_too_large",
            Self::Truncated { .. } => "truncated",
            Self::NonMinimalVarint { .. } => "non_minimal_varint",
            Self::VarintOverflow { .. } => "varint_overflow",
            Self::UnknownField { .. } => "unknown_field",
            Self::UnknownOneofVariant { .. } => "unknown_oneof_variant",
            Self::WrongWireType { .. } => "wrong_wire_type",
            Self::DuplicateField { .. } => "duplicate_field",
            Self::FieldOutOfOrder { .. } => "field_out_of_order",
            Self::DefaultValueEncoded { .. } => "default_value_encoded",
            Self::FieldForbidden { .. } => "field_forbidden",
            Self::MissingField { .. } => "missing_field",
            Self::MissingOneof { .. } => "missing_oneof",
            Self::MultipleOneofVariants { .. } => "multiple_oneof_variants",
            Self::UnspecifiedEnum { .. } => "unspecified_enum",
            Self::EnumValue { .. } => "enum_value",
            Self::WrongLength { .. } => "wrong_length",
            Self::FieldTooLong { .. } => "field_too_long",
            Self::InvalidUtf8 { .. } => "invalid_utf8",
            Self::InvalidDisplayName { .. } => "invalid_display_name",
            Self::InvalidHostname { .. } => "invalid_hostname",
            Self::InvalidEmail { .. } => "invalid_email",
            Self::ValueOutOfRange { .. } => "value_out_of_range",
            Self::RepeatedCount { .. } => "repeated_count",
            Self::RepeatedDuplicate { .. } => "repeated_duplicate",
            Self::FieldsMustDiffer { .. } => "fields_must_differ",
            Self::FieldsMustMatch { .. } => "fields_must_match",
            Self::SetAtSeqZero { .. } => "set_at_seq_zero",
            Self::MissingChainField { .. } => "missing_chain_field",
            Self::InceptionPastSeqZero => "inception_past_seq_zero",
            Self::NonInceptionAtSeqZero => "non_inception_at_seq_zero",
            Self::EmbeddedInceptionNotRawRooted => "embedded_inception_not_raw_rooted",
            Self::InceptionIdMismatch { .. } => "inception_id_mismatch",
            Self::InceptionKeyMismatch { .. } => "inception_key_mismatch",
            Self::InvalidPublicKey { .. } => "invalid_public_key",
            Self::BadSignature { .. } => "bad_signature",
            Self::TooDeeplyNested => "too_deeply_nested",
        }
    }
}

/// The two protobuf wire types the mabel schemas use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireKind {
    /// Wire type 0.
    Varint,
    /// Wire type 2.
    Len,
}

impl WireKind {
    /// The number the wire type carries in a record key.
    pub const fn number(self) -> u8 {
        match self {
            Self::Varint => 0,
            Self::Len => 2,
        }
    }
}

/// How often a field may appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// Never: the field table requires the field to be absent.
    Forbidden,
    /// At most once.
    Optional,
    /// Exactly once.
    Required,
    /// At most once, and one variant of the message's `oneof` must appear.
    Variant,
    /// Between `min` and `max` times, with entries optionally distinct.
    Repeated {
        /// The fewest accepted entries.
        min: usize,
        /// The most accepted entries.
        max: usize,
        /// Whether two entries may be equal.
        distinct: bool,
    },
}

/// One accepted value of an enum field.
#[derive(Debug, Clone, Copy)]
pub struct EnumValue {
    /// The value on the wire.
    pub number: u64,
    /// The name in the `.proto`.
    pub name: &'static str,
}

/// Which codepoint rule a `string` field carries beyond well-formed UTF-8.
///
/// Every rule starts from the shared policy of proposal 003 section 1 and
/// adds what its field needs, so one field never inherits another's rejection
/// code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringRule {
    /// Well-formed UTF-8 under the byte cap and nothing more: what a free-text
    /// protocol string carries, where no name is being published.
    Utf8,
    /// A human-facing name: the shared codepoint policy, plus a refusal of any
    /// value that parses as an identity id, so a name never masquerades as an
    /// identifier. Rejections are [`WireError::InvalidDisplayName`].
    DisplayName,
    /// A DNS name under the syntax of proposal 003 section 2. Rejections are
    /// [`WireError::InvalidHostname`].
    Hostname,
    /// An email address: the shared codepoint policy, plus exactly one `@` with
    /// at least one byte on each side (proposal 005). No deliverability check
    /// is possible here or anywhere else: the address is a claim, like
    /// everything else on a ledger. Rejections are
    /// [`WireError::InvalidEmail`].
    Email,
}

/// What a field holds and what the field table says about it.
#[derive(Debug)]
pub enum FieldKind {
    /// A varint whose value must fall in `min..=max`.
    Varint {
        /// The lowest accepted value.
        min: u64,
        /// The highest accepted value.
        max: u64,
    },
    /// An enum, which must hold one of `values`.
    Enum {
        /// The values this field accepts, never including `*_UNSPECIFIED`.
        values: &'static [EnumValue],
    },
    /// Opaque bytes with an exact length, a cap, or both.
    Bytes {
        /// The exact length the field table states, if it states one.
        exact: Option<usize>,
        /// The cap on the length.
        max: usize,
    },
    /// A `string` field: at most `max` bytes of well-formed UTF-8, then the
    /// per-field rule of [`StringRule`].
    String {
        /// The cap on the encoded length in bytes.
        max: usize,
        /// What the field table says about the characters, beyond UTF-8.
        rule: StringRule,
    },
    /// A `bytes` field carrying another message's encoded bytes verbatim,
    /// validated with `descriptor`.
    Nested {
        /// The descriptor of the message the bytes encode.
        descriptor: &'static MessageDescriptor,
        /// The cap on the length.
        max: usize,
    },
    /// A submessage field.
    Message {
        /// The descriptor of the submessage.
        descriptor: &'static MessageDescriptor,
    },
    /// A submessage that is an independent top-level object: its bytes are
    /// stored and served verbatim (a `SignedEvent` inside a frame, bundle or
    /// fork record), so it is scanned with a fresh nesting budget instead of
    /// inheriting the container's depth.
    Detached {
        /// The descriptor of the detached message.
        descriptor: &'static MessageDescriptor,
    },
}

/// One field of a message.
#[derive(Debug)]
pub struct FieldDescriptor {
    /// The field number.
    pub number: u32,
    /// The field name in the `.proto`.
    pub name: &'static str,
    /// How often the field may appear.
    pub cardinality: Cardinality,
    /// What the field holds.
    pub kind: FieldKind,
}

impl FieldDescriptor {
    /// The wire type this field's type requires.
    pub const fn wire_kind(&self) -> WireKind {
        match self.kind {
            FieldKind::Varint { .. } | FieldKind::Enum { .. } => WireKind::Varint,
            FieldKind::Bytes { .. }
            | FieldKind::String { .. }
            | FieldKind::Nested { .. }
            | FieldKind::Message { .. }
            | FieldKind::Detached { .. } => WireKind::Len,
        }
    }
}

/// The `oneof` of a message, if it has one.
#[derive(Debug, Clone, Copy)]
pub struct Oneof {
    /// The `oneof` name in the `.proto`.
    pub name: &'static str,
    /// The first field number reserved for variants. Every field number at or
    /// above this one belongs to the `oneof`, so a number this version does
    /// not know is an unrecognised variant rather than an unknown field.
    pub first_number: u32,
}

/// A message's cross-field rules, run after every field has passed.
pub type CrossFieldCheck = fn(&Scanned<'_>) -> Result<(), WireError>;

/// Everything the validator knows about one message type.
///
/// A new message registers a descriptor and calls [`message`]; nothing else
/// in the validator changes.
#[derive(Debug)]
pub struct MessageDescriptor {
    /// The message name in the `.proto`.
    pub name: &'static str,
    /// The cap on the encoded length.
    pub max_bytes: usize,
    /// The fields, in ascending field-number order.
    pub fields: &'static [FieldDescriptor],
    /// The `oneof`, if the message has one.
    pub oneof: Option<Oneof>,
    /// The cross-field rules the descriptor cannot express.
    pub check: Option<CrossFieldCheck>,
}

impl MessageDescriptor {
    fn field(&self, number: u32) -> Option<&'static FieldDescriptor> {
        // The slice is `&'static`, so the borrow outlives `&self`.
        self.fields.iter().find(|field| field.number == number)
    }
}

const fn forbidden(number: u32, name: &'static str, kind: FieldKind) -> FieldDescriptor {
    FieldDescriptor {
        number,
        name,
        cardinality: Cardinality::Forbidden,
        kind,
    }
}

const fn optional(number: u32, name: &'static str, kind: FieldKind) -> FieldDescriptor {
    FieldDescriptor {
        number,
        name,
        cardinality: Cardinality::Optional,
        kind,
    }
}

const fn required(number: u32, name: &'static str, kind: FieldKind) -> FieldDescriptor {
    FieldDescriptor {
        number,
        name,
        cardinality: Cardinality::Required,
        kind,
    }
}

const fn variant(
    number: u32,
    name: &'static str,
    descriptor: &'static MessageDescriptor,
) -> FieldDescriptor {
    FieldDescriptor {
        number,
        name,
        cardinality: Cardinality::Variant,
        kind: FieldKind::Message { descriptor },
    }
}

/// A 32-byte id, ledger id, event id or public key.
const ID: FieldKind = FieldKind::Bytes {
    exact: Some(ID_BYTES),
    max: ID_BYTES,
};

/// A 64-byte ed25519 signature.
const SIG: FieldKind = FieldKind::Bytes {
    exact: Some(SIG_BYTES),
    max: SIG_BYTES,
};

/// A 16-byte inception nonce.
const NONCE: FieldKind = FieldKind::Bytes {
    exact: Some(NONCE_BYTES),
    max: NONCE_BYTES,
};

/// An embedded raw-rooted inception, checked by the enclosing message's
/// cross-field rule rather than by the scanner, so the bytes are scanned once.
const EMBEDDED_INCEPTION: FieldKind = FieldKind::Bytes {
    exact: None,
    max: MAX_EMBEDDED_INCEPTION_BYTES,
};

/// A `uint32 version` field, which the canonical encoding never serializes.
const VERSION: FieldKind = FieldKind::Varint {
    min: 1,
    max: u32::MAX as u64,
};

/// The `DeclaredKind` values a defined kind may hold. `KIND_UNSPECIFIED` is
/// absent, so an unset or zero kind is rejected (proposal 002 section 3).
const DECLARED_KIND: FieldKind = FieldKind::Enum {
    values: &[
        EnumValue {
            number: 1,
            name: "PERSON",
        },
        EnumValue {
            number: 2,
            name: "ORGANIZATION",
        },
        EnumValue {
            number: 3,
            name: "AGENT",
        },
        EnumValue {
            number: 4,
            name: "SERVICE",
        },
    ],
};

/// The `Role` values an invitation may offer. Values 3 and up are the
/// narrower-capability slot of proposal 002 section 9 and are rejected here.
const ROLE: FieldKind = FieldKind::Enum {
    values: &[
        EnumValue {
            number: 1,
            name: "MEMBER",
        },
        EnumValue {
            number: 2,
            name: "CONTROLLER",
        },
    ],
};

/// `RawRoot` (proposal 002 section 8).
pub static RAW_ROOT: MessageDescriptor = MessageDescriptor {
    name: "RawRoot",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        required(1, "active_key", ID),
        required(2, "reserve_commit", ID),
    ],
    oneof: None,
    check: Some(check_raw_root),
};

/// `IdentityRoot` (proposal 002 section 8).
pub static IDENTITY_ROOT: MessageDescriptor = MessageDescriptor {
    name: "IdentityRoot",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        required(1, "founder", ID),
        required(2, "founder_key", ID),
        required(3, "founder_inception", EMBEDDED_INCEPTION),
    ],
    oneof: None,
    check: Some(check_identity_root),
};

/// `Inception`, the one seq-0 payload (proposal 002 section 8).
///
/// `kind` must be a defined value and the `root` `oneof` must name exactly
/// one recognised variant. The kind never selects the root: it is advisory.
pub static INCEPTION: MessageDescriptor = MessageDescriptor {
    name: "Inception",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        required(1, "kind", DECLARED_KIND),
        required(2, "nonce", NONCE),
        variant(RAW_ROOT_TAG, "raw_root", &RAW_ROOT),
        variant(IDENTITY_ROOT_TAG, "identity_root", &IDENTITY_ROOT),
    ],
    oneof: Some(Oneof {
        name: "root",
        first_number: RAW_ROOT_TAG,
    }),
    check: None,
};

/// `WitnessConfig` (proposal 001 section 3.4).
///
/// Retired for writing and read forever, with these rules unchanged (proposal
/// 006 section 1): the fold must accept whatever a valid chain contains, so a
/// chain written before tag 19 existed keeps folding.
pub static WITNESS_CONFIG: MessageDescriptor = MessageDescriptor {
    name: "WitnessConfig",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[FieldDescriptor {
        number: 1,
        name: "witnesses",
        cardinality: Cardinality::Repeated {
            min: 1,
            max: MAX_WITNESSES,
            distinct: true,
        },
        kind: ID,
    }],
    oneof: None,
    check: None,
};

/// `EndpointAdvertisement` (proposal 006 sections 2 and 3).
///
/// The list may be empty, which says nothing answers for this identity right
/// now. Each entry must also decompress to a valid ed25519 point, which the
/// fold checks: an endpoint id that is not a point can never be dialled.
pub static ENDPOINT_ADVERTISEMENT: MessageDescriptor = MessageDescriptor {
    name: "EndpointAdvertisement",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[FieldDescriptor {
        number: 1,
        name: "endpoints",
        cardinality: Cardinality::Repeated {
            min: 0,
            max: MAX_ENDPOINTS,
            distinct: true,
        },
        kind: ID,
    }],
    oneof: None,
    check: None,
};

/// `WitnessSet` (proposal 006 sections 1 and 3).
///
/// The list may be empty, which says nobody keeps this chain. Each entry is an
/// identity id, a digest and not a key, so no point check applies, and the
/// ledger's own id is allowed: that is how a self-hosted identity says it keeps
/// its own chain.
pub static WITNESS_SET: MessageDescriptor = MessageDescriptor {
    name: "WitnessSet",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[FieldDescriptor {
        number: 1,
        name: "witnesses",
        cardinality: Cardinality::Repeated {
            min: 0,
            max: MAX_WITNESSES,
            distinct: true,
        },
        kind: ID,
    }],
    oneof: None,
    check: None,
};

/// `TrustAttestation` (proposal 001 section 3.4).
pub static TRUST_ATTESTATION: MessageDescriptor = MessageDescriptor {
    name: "TrustAttestation",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[required(1, "subject", ID)],
    oneof: None,
    check: None,
};

/// `TrustRevocation` (proposal 001 section 3.4).
pub static TRUST_REVOCATION: MessageDescriptor = MessageDescriptor {
    name: "TrustRevocation",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[required(1, "target", ID)],
    oneof: None,
    check: None,
};

/// `MembershipInvitation` (proposal 002 section 8).
pub static MEMBERSHIP_INVITATION: MessageDescriptor = MessageDescriptor {
    name: "MembershipInvitation",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        required(1, "invitee", ID),
        required(2, "invitee_key", ID),
        required(3, "role", ROLE),
        required(4, "invitee_inception", EMBEDDED_INCEPTION),
    ],
    oneof: None,
    check: Some(check_membership_invitation),
};

/// `Acceptance`, the detached blob a `MembershipAcceptance` embeds verbatim
/// (proposal 001 section 3.5).
pub static ACCEPTANCE: MessageDescriptor = MessageDescriptor {
    name: "Acceptance",
    max_bytes: MAX_ACCEPTANCE_BYTES,
    fields: &[
        forbidden(1, "version", VERSION),
        required(2, "ledger", ID),
        required(3, "invitation_event", ID),
        required(4, "invitee", ID),
        required(5, "invitee_key", ID),
    ],
    oneof: None,
    check: None,
};

/// `MembershipAcceptance` (proposal 002 section 8).
pub static MEMBERSHIP_ACCEPTANCE: MessageDescriptor = MessageDescriptor {
    name: "MembershipAcceptance",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        required(
            1,
            "acceptance",
            FieldKind::Nested {
                descriptor: &ACCEPTANCE,
                max: MAX_ACCEPTANCE_BYTES,
            },
        ),
        required(2, "signature", SIG),
    ],
    oneof: None,
    check: Some(check_membership_acceptance),
};

/// `MembershipRemoval` (proposal 002 section 8).
pub static MEMBERSHIP_REMOVAL: MessageDescriptor = MessageDescriptor {
    name: "MembershipRemoval",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[required(1, "target", ID)],
    oneof: None,
    check: None,
};

/// `ProfileUpdate` (proposal 003 section 1, `email` from proposal 005).
///
/// All three fields are optional and a zero-length payload is legal: it clears
/// all three. An empty string never reaches these rows, because the canonical
/// encoding omits a proto3 default and an explicitly encoded one is
/// [`WireError::DefaultValueEncoded`].
pub static PROFILE_UPDATE: MessageDescriptor = MessageDescriptor {
    name: "ProfileUpdate",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        optional(
            1,
            "display_name",
            FieldKind::String {
                max: MAX_DISPLAY_NAME_BYTES,
                rule: StringRule::DisplayName,
            },
        ),
        optional(
            2,
            "hostname",
            FieldKind::String {
                max: MAX_HOSTNAME_BYTES,
                rule: StringRule::Hostname,
            },
        ),
        optional(
            3,
            "email",
            FieldKind::String {
                max: MAX_EMAIL_BYTES,
                rule: StringRule::Email,
            },
        ),
    ],
    oneof: None,
    check: None,
};

/// `EventBody`, the message that is hashed and signed (proposal 001
/// section 3.2).
pub static EVENT_BODY: MessageDescriptor = MessageDescriptor {
    name: "EventBody",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        forbidden(1, "version", VERSION),
        optional(2, "ledger", ID),
        optional(
            3,
            "seq",
            FieldKind::Varint {
                min: 1,
                max: u64::MAX,
            },
        ),
        optional(4, "prev", ID),
        required(
            5,
            "timestamp_ms",
            FieldKind::Varint {
                min: 1,
                max: MAX_TIMESTAMP_MS,
            },
        ),
        required(6, "author_key", ID),
        variant(INCEPTION_TAG, "inception", &INCEPTION),
        variant(11, "witness_config", &WITNESS_CONFIG),
        variant(
            TRUST_ATTESTATION_TAG,
            "trust_attestation",
            &TRUST_ATTESTATION,
        ),
        variant(13, "trust_revocation", &TRUST_REVOCATION),
        variant(
            MEMBERSHIP_INVITATION_TAG,
            "membership_invitation",
            &MEMBERSHIP_INVITATION,
        ),
        variant(15, "membership_acceptance", &MEMBERSHIP_ACCEPTANCE),
        variant(16, "membership_removal", &MEMBERSHIP_REMOVAL),
        variant(PROFILE_UPDATE_TAG, "profile_update", &PROFILE_UPDATE),
        variant(18, "endpoint_advertisement", &ENDPOINT_ADVERTISEMENT),
        variant(19, "witness_set", &WITNESS_SET),
    ],
    oneof: Some(Oneof {
        name: "payload",
        first_number: INCEPTION_TAG,
    }),
    check: Some(check_event_body),
};

/// `SignedEvent`, the byte string that crosses the network and lands on disk
/// (proposal 001 section 3.2).
pub static SIGNED_EVENT: MessageDescriptor = MessageDescriptor {
    name: "SignedEvent",
    max_bytes: MAX_EVENT_BYTES,
    fields: &[
        required(
            1,
            "body",
            FieldKind::Nested {
                descriptor: &EVENT_BODY,
                max: MAX_EVENT_BYTES,
            },
        ),
        required(2, "signature", SIG),
    ],
    oneof: None,
    check: None,
};

/// One scanned record: a field number and the value it carried.
#[derive(Debug, Clone, Copy)]
enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

/// The fields a message carried, in the order they appeared.
///
/// A cross-field rule reads its inputs from here; the slices borrow the input
/// bytes, so nothing is copied.
#[derive(Debug)]
pub struct Scanned<'a> {
    descriptor: &'static MessageDescriptor,
    depth: u32,
    entries: Vec<(u32, Value<'a>)>,
}

impl<'a> Scanned<'a> {
    /// The descriptor this scan used.
    pub fn descriptor(&self) -> &'static MessageDescriptor {
        self.descriptor
    }

    /// How deeply this message was nested, counting the outermost as 0.
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// The value of a varint field, or `None` if it was absent.
    pub fn varint(&self, number: u32) -> Option<u64> {
        self.entries.iter().find_map(|(found, value)| match value {
            Value::Varint(value) if *found == number => Some(*value),
            _ => None,
        })
    }

    /// The bytes of a length-delimited field, or `None` if it was absent.
    pub fn bytes(&self, number: u32) -> Option<&'a [u8]> {
        self.entries.iter().find_map(|(found, value)| match value {
            Value::Bytes(bytes) if *found == number => Some(*bytes),
            _ => None,
        })
    }

    /// Every entry of a repeated length-delimited field, in order.
    pub fn repeated_bytes(&self, number: u32) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.entries
            .iter()
            .filter_map(move |(found, value)| match value {
                Value::Bytes(bytes) if *found == number => Some(*bytes),
                _ => None,
            })
    }

    /// How many times a field appeared.
    pub fn count(&self, number: u32) -> usize {
        self.entries
            .iter()
            .filter(|(found, _)| *found == number)
            .count()
    }

    /// The `oneof` variant this message carried: its field number and the
    /// encoded submessage.
    pub fn oneof(&self) -> Option<(u32, &'a [u8])> {
        self.entries.iter().find_map(|(number, value)| {
            let is_variant = self
                .descriptor
                .field(*number)
                .is_some_and(|field| field.cardinality == Cardinality::Variant);
            match value {
                Value::Bytes(bytes) if is_variant => Some((*number, *bytes)),
                _ => None,
            }
        })
    }

    /// Re-reads a submessage's fields. The bytes have already been validated,
    /// so this only reads them back for a cross-field rule.
    fn subfields(
        &self,
        descriptor: &'static MessageDescriptor,
        bytes: &'a [u8],
    ) -> Result<Scanned<'a>, WireError> {
        scan(descriptor, bytes, self.depth + 1)
    }
}

/// Validates an encoded `SignedEvent`: the 4096-byte cap, the wire format of
/// the event and everything it embeds, and every stateless row of the field
/// table.
///
/// The event's own signature is not checked here: it needs the `author_key`
/// authorization the fold supplies (proposal 001 section 3.6, step 5).
pub fn signed_event(bytes: &[u8]) -> Result<(), WireError> {
    message(&SIGNED_EVENT, bytes)
}

/// Validates an encoded `EventBody`, the bytes that were hashed and signed.
pub fn event_body(bytes: &[u8]) -> Result<(), WireError> {
    message(&EVENT_BODY, bytes)
}

/// Validates an encoded `Acceptance` blob (proposal 001 section 3.5).
pub fn acceptance(bytes: &[u8]) -> Result<(), WireError> {
    message(&ACCEPTANCE, bytes)
}

/// Validates `bytes` against any registered descriptor.
///
/// This is the entry point a later message type uses: register a
/// [`MessageDescriptor`] and call this.
pub fn message(descriptor: &'static MessageDescriptor, bytes: &[u8]) -> Result<(), WireError> {
    validate(descriptor, bytes, 0).map(|_| ())
}

/// Validates `bytes` and hands back the fields it scanned.
///
/// The slices in the result borrow `bytes`, so a caller takes an embedded
/// message's bytes verbatim instead of re-encoding a decoded one (proposal 001
/// section 3.1, pitfall 1). The file artifacts of section 3.8 read their
/// embedded events this way.
pub(crate) fn message_fields<'a>(
    descriptor: &'static MessageDescriptor,
    bytes: &'a [u8],
) -> Result<Scanned<'a>, WireError> {
    validate(descriptor, bytes, 0)
}

/// Verifies an invitee's signature over a detached `Acceptance` blob.
///
/// `MembershipAcceptance` and the `AcceptanceFile` of section 3.8 carry the
/// same pair of fields under the same rule, so both run this (proposal 002
/// section 7). `blob` has already passed the `Acceptance` field table, which
/// is what makes reading `invitee_key` back out of it safe.
pub(crate) fn detached_acceptance(
    blob: &[u8],
    signature: &[u8],
    message: &'static str,
) -> Result<(), WireError> {
    let fields = scan(&ACCEPTANCE, blob, 0)?;
    let invitee_key = public_key(
        "Acceptance",
        "invitee_key",
        fields.bytes(5).expect("invitee_key is required"),
    )?;
    verify(
        &invitee_key,
        &accept_input(blob),
        signature,
        message,
        "signature",
    )
}

/// What an embedded inception proves about the identity it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandaloneInception {
    /// The recomputed `event_id` of the inception.
    pub event_id: EventId,
    /// The identity the inception creates, which is that same digest
    /// (proposal 001 section 3.3).
    pub identity: IdentityId,
    /// The `RawRoot.active_key` the inception records, authoritative for life.
    pub active_key: PublicKey,
}

/// Verifies an embedded `founder_inception` or `invitee_inception` on its own
/// (proposal 002 section 8).
///
/// The bytes must be a canonical `SignedEvent` that passes the field table,
/// carry an `Inception` at seq 0 whose root is a `RawRoot`, and bear a
/// signature that verifies under the `active_key` that root records. Declared
/// kind is ignored: a raw root is the requirement, because an identity-rooted
/// ledger holds no key of its own and cannot sign anything (proposal 002
/// section 9 caps nesting at depth 1). The caller compares the returned
/// `event_id` and `active_key` with the id and key recorded beside the
/// inception.
pub fn verify_inception_standalone(bytes: &[u8]) -> Result<StandaloneInception, WireError> {
    inception_standalone(bytes, 0)
}

fn inception_standalone(bytes: &[u8], depth: u32) -> Result<StandaloneInception, WireError> {
    let signed = validate(&SIGNED_EVENT, bytes, depth)?;
    let body = signed.bytes(1).expect("body is required");
    let signature = signed.bytes(2).expect("signature is required");

    let body_fields = scan(&EVENT_BODY, body, depth + 1)?;
    let (number, payload) = body_fields.oneof().expect("payload is required");
    if number != INCEPTION_TAG {
        return Err(WireError::EmbeddedInceptionNotRawRooted);
    }
    let inception = scan(&INCEPTION, payload, depth + 2)?;
    let (root_number, root) = inception.oneof().expect("root is required");
    if root_number != RAW_ROOT_TAG {
        return Err(WireError::EmbeddedInceptionNotRawRooted);
    }
    let raw_root = scan(&RAW_ROOT, root, depth + 3)?;
    let active_key = public_key(
        "RawRoot",
        "active_key",
        raw_root.bytes(1).expect("active_key is required"),
    )?;
    verify(
        &active_key,
        &sign_input(body),
        signature,
        "SignedEvent",
        "signature",
    )?;

    let id = event_id(body);
    Ok(StandaloneInception {
        event_id: id,
        identity: id.into(),
        active_key,
    })
}

fn validate<'a>(
    descriptor: &'static MessageDescriptor,
    bytes: &'a [u8],
    depth: u32,
) -> Result<Scanned<'a>, WireError> {
    if bytes.len() > descriptor.max_bytes {
        return Err(WireError::MessageTooLarge {
            message: descriptor.name,
            len: bytes.len(),
            cap: descriptor.max_bytes,
        });
    }
    let scanned = scan(descriptor, bytes, depth)?;
    check_cardinality(&scanned)?;
    if let Some(check) = descriptor.check {
        check(&scanned)?;
    }
    Ok(scanned)
}

/// Reads every record of `bytes`, rejecting the seven wire-format classes as
/// it goes and recursing into submessages.
fn scan<'a>(
    descriptor: &'static MessageDescriptor,
    bytes: &'a [u8],
    depth: u32,
) -> Result<Scanned<'a>, WireError> {
    if depth > MAX_NESTING {
        return Err(WireError::TooDeeplyNested);
    }
    let name = descriptor.name;
    let mut entries: Vec<(u32, Value<'a>)> = Vec::new();
    let mut last_number = 0u32;
    let mut pos = 0usize;

    while pos < bytes.len() {
        let key = read_varint(bytes, &mut pos, name)?;
        // A number past `u32::MAX` is unknown whatever it is; reporting the
        // saturated value keeps the error total.
        let number = u32::try_from(key >> 3).unwrap_or(u32::MAX);
        let wire = (key & 7) as u8;

        let field = descriptor
            .field(number)
            .ok_or_else(|| unknown_field(descriptor, number))?;

        let seen = entries.iter().any(|(found, _)| *found == number);
        let repeated = matches!(field.cardinality, Cardinality::Repeated { .. });
        if seen && !repeated {
            return Err(WireError::DuplicateField {
                message: name,
                field: field.name,
            });
        }
        if number < last_number || (seen && number != last_number) {
            return Err(WireError::FieldOutOfOrder {
                message: name,
                number,
            });
        }
        last_number = number;

        let expected = field.wire_kind();
        if wire != expected.number() {
            return Err(WireError::WrongWireType {
                message: name,
                field: field.name,
                expected: expected.number(),
                actual: wire,
            });
        }

        // The repeated cap is enforced here, on the occurrence that passes it,
        // rather than after the scan: the entry that breaks the cap is refused
        // before it is read and before the scan recurses into it, so a frame
        // claiming thousands of events costs one entry's work (pitfall 7).
        if let Cardinality::Repeated { min, max, .. } = field.cardinality {
            let seen = entries.iter().filter(|(found, _)| *found == number).count();
            if seen >= max {
                return Err(WireError::RepeatedCount {
                    message: name,
                    field: field.name,
                    count: seen + 1,
                    min,
                    max,
                });
            }
        }

        let value = match expected {
            WireKind::Varint => {
                let value = read_varint(bytes, &mut pos, name)?;
                check_varint(field, name, value)?;
                Value::Varint(value)
            }
            WireKind::Len => {
                let len = read_varint(bytes, &mut pos, name)?;
                let len = usize::try_from(len).unwrap_or(usize::MAX);
                if len > bytes.len() - pos {
                    return Err(WireError::Truncated { message: name });
                }
                let slice = &bytes[pos..pos + len];
                pos += len;
                check_bytes(field, name, slice, depth)?;
                Value::Bytes(slice)
            }
        };
        entries.push((number, value));
    }

    Ok(Scanned {
        descriptor,
        depth,
        entries,
    })
}

fn unknown_field(descriptor: &'static MessageDescriptor, number: u32) -> WireError {
    match descriptor.oneof {
        Some(oneof) if number >= oneof.first_number => WireError::UnknownOneofVariant {
            message: descriptor.name,
            oneof: oneof.name,
            number,
        },
        _ => WireError::UnknownField {
            message: descriptor.name,
            number,
        },
    }
}

/// Reads a base-128 varint, rejecting padding and values wider than 64 bits.
fn read_varint(bytes: &[u8], pos: &mut usize, message: &'static str) -> Result<u64, WireError> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *bytes.get(*pos).ok_or(WireError::Truncated { message })?;
        *pos += 1;
        let payload = u64::from(byte & 0x7f);
        if shift == 63 && payload > 1 {
            return Err(WireError::VarintOverflow { message });
        }
        value |= payload << shift;
        if byte & 0x80 == 0 {
            // A trailing zero group is padding: the canonical encoding uses
            // the shortest form, so only a single-byte varint may be zero.
            if payload == 0 && shift > 0 {
                return Err(WireError::NonMinimalVarint { message });
            }
            return Ok(value);
        }
        shift += 7;
        if shift > 63 {
            return Err(WireError::VarintOverflow { message });
        }
    }
}

fn check_varint(
    field: &'static FieldDescriptor,
    message: &'static str,
    value: u64,
) -> Result<(), WireError> {
    match field.kind {
        FieldKind::Enum { values } => {
            // An absent enum and an encoded 0 mean the same thing, so both
            // land on the same rejection.
            if value == 0 {
                return Err(WireError::UnspecifiedEnum {
                    message,
                    field: field.name,
                });
            }
            if !values.iter().any(|accepted| accepted.number == value) {
                return Err(WireError::EnumValue {
                    message,
                    field: field.name,
                    value,
                });
            }
            Ok(())
        }
        FieldKind::Varint { min, max } => {
            if value == 0 {
                return Err(WireError::DefaultValueEncoded {
                    message,
                    field: field.name,
                });
            }
            if value < min || value > max {
                return Err(WireError::ValueOutOfRange {
                    message,
                    field: field.name,
                    value,
                    min,
                    max,
                });
            }
            Ok(())
        }
        _ => unreachable!("only varint fields reach check_varint"),
    }
}

fn check_bytes(
    field: &'static FieldDescriptor,
    message: &'static str,
    slice: &[u8],
    depth: u32,
) -> Result<(), WireError> {
    match field.kind {
        FieldKind::Bytes { exact, max } => {
            if let Some(exact) = exact
                && slice.len() != exact
            {
                return Err(WireError::WrongLength {
                    message,
                    field: field.name,
                    expected: exact,
                    actual: slice.len(),
                });
            }
            if slice.is_empty() {
                return Err(WireError::DefaultValueEncoded {
                    message,
                    field: field.name,
                });
            }
            if slice.len() > max {
                return Err(WireError::FieldTooLong {
                    message,
                    field: field.name,
                    len: slice.len(),
                    cap: max,
                });
            }
            Ok(())
        }
        FieldKind::String { max, rule } => {
            if slice.is_empty() {
                return Err(WireError::DefaultValueEncoded {
                    message,
                    field: field.name,
                });
            }
            if slice.len() > max {
                return Err(WireError::FieldTooLong {
                    message,
                    field: field.name,
                    len: slice.len(),
                    cap: max,
                });
            }
            // Borrowed from the scanned slice: nothing is copied or allocated
            // to read the characters.
            let text = std::str::from_utf8(slice).map_err(|_| WireError::InvalidUtf8 {
                message,
                field: field.name,
            })?;
            match rule {
                StringRule::Utf8 => Ok(()),
                StringRule::DisplayName => {
                    check_display_name(text).map_err(|reason| WireError::InvalidDisplayName {
                        message,
                        field: field.name,
                        reason,
                    })
                }
                StringRule::Hostname => {
                    check_hostname(text).map_err(|reason| WireError::InvalidHostname {
                        message,
                        field: field.name,
                        reason,
                    })
                }
                StringRule::Email => check_email(text).map_err(|reason| WireError::InvalidEmail {
                    message,
                    field: field.name,
                    reason,
                }),
            }
        }
        FieldKind::Nested { descriptor, max } => {
            if slice.is_empty() {
                return Err(WireError::DefaultValueEncoded {
                    message,
                    field: field.name,
                });
            }
            if slice.len() > max {
                return Err(WireError::FieldTooLong {
                    message,
                    field: field.name,
                    len: slice.len(),
                    cap: max,
                });
            }
            validate(descriptor, slice, depth + 1).map(|_| ())
        }
        // An empty submessage is legal protobuf; its own required fields and
        // repeated bounds reject it.
        FieldKind::Message { descriptor } => validate(descriptor, slice, depth + 1).map(|_| ()),
        // A detached object restarts the budget: its bytes stand alone
        // outside this container, so its depth is its own.
        FieldKind::Detached { descriptor } => validate(descriptor, slice, 0).map(|_| ()),
        _ => unreachable!("only length-delimited fields reach check_bytes"),
    }
}

/// The codepoint policy of proposal 003 section 1, which every published
/// string starts from: no control character, no bidi control and no
/// zero-width or invisible format character.
fn check_codepoints(text: &str) -> Result<(), &'static str> {
    for character in text.chars() {
        match character {
            '\u{0}'..='\u{1f}' | '\u{7f}' => return Err("it holds a C0 control character"),
            '\u{80}'..='\u{9f}' => return Err("it holds a C1 control character"),
            // Bidi embeddings, overrides and isolates reorder the rendered
            // text without changing the bytes.
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
                return Err("it holds a bidi control character");
            }
            '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}' => {
                return Err("it holds a zero-width or invisible format character");
            }
            _ => {}
        }
    }
    Ok(())
}

/// The codepoint policy of proposal 003 section 1, plus the rule that a
/// display name may not parse as an identity id.
fn check_display_name(text: &str) -> Result<(), &'static str> {
    check_codepoints(text)?;
    if text.trim() != text {
        return Err("it has leading or trailing whitespace");
    }
    // A name that reads as an identifier is the confusion this rule exists to
    // stop: every surface prints both.
    if text.parse::<IdentityId>().is_ok() {
        return Err("it parses as an identity id");
    }
    Ok(())
}

/// The hostname syntax of proposal 003 section 2: ASCII lowercase LDH labels
/// of 1 to 63 bytes with alphanumeric edges, at least one dot and no trailing
/// dot. The 246-byte cap is the field's, checked before this runs.
fn check_hostname(text: &str) -> Result<(), &'static str> {
    if !text.is_ascii() {
        return Err("it holds a character outside ASCII");
    }
    if text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err("it holds an uppercase letter");
    }
    if text.ends_with('.') {
        return Err("it ends with a dot");
    }
    if !text.contains('.') {
        return Err("it holds no dot");
    }
    for label in text.split('.') {
        let bytes = label.as_bytes();
        let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
            return Err("it holds an empty label");
        };
        if bytes.len() > MAX_HOSTNAME_LABEL_BYTES {
            return Err("it holds a label over 63 bytes");
        }
        if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
            return Err("a label does not start and end with a letter or digit");
        }
        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return Err("a label holds a character outside [a-z0-9-]");
        }
    }
    Ok(())
}

/// The email rule of proposal 005: the shared codepoint policy, then exactly
/// one `@` with at least one byte on each side. The 254-byte cap is the
/// field's, checked before this runs.
///
/// Nothing here says the address exists or answers. Deliverability is not a
/// property a ledger can carry, so the address is a claim like every other
/// field of the profile.
fn check_email(text: &str) -> Result<(), &'static str> {
    check_codepoints(text)?;
    let mut parts = text.split('@');
    let (Some(local), Some(domain)) = (parts.next(), parts.next()) else {
        return Err("it holds no at sign");
    };
    if parts.next().is_some() {
        return Err("it holds more than one at sign");
    }
    if local.is_empty() {
        return Err("it holds nothing before the at sign");
    }
    if domain.is_empty() {
        return Err("it holds nothing after the at sign");
    }
    Ok(())
}

fn check_cardinality(scanned: &Scanned<'_>) -> Result<(), WireError> {
    let message = scanned.descriptor.name;
    for field in scanned.descriptor.fields {
        let count = scanned.count(field.number);
        match field.cardinality {
            Cardinality::Forbidden if count > 0 => {
                return Err(WireError::FieldForbidden {
                    message,
                    field: field.name,
                });
            }
            Cardinality::Required if count == 0 => {
                return Err(match field.kind {
                    FieldKind::Enum { .. } => WireError::UnspecifiedEnum {
                        message,
                        field: field.name,
                    },
                    _ => WireError::MissingField {
                        message,
                        field: field.name,
                    },
                });
            }
            Cardinality::Repeated { min, max, distinct } => {
                if count < min || count > max {
                    return Err(WireError::RepeatedCount {
                        message,
                        field: field.name,
                        count,
                        min,
                        max,
                    });
                }
                if distinct {
                    let entries: Vec<&[u8]> = scanned.repeated_bytes(field.number).collect();
                    for (index, entry) in entries.iter().enumerate() {
                        if entries[index + 1..].contains(entry) {
                            return Err(WireError::RepeatedDuplicate {
                                message,
                                field: field.name,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(oneof) = scanned.descriptor.oneof {
        let variants = scanned
            .descriptor
            .fields
            .iter()
            .filter(|field| field.cardinality == Cardinality::Variant)
            .filter(|field| scanned.count(field.number) > 0)
            .count();
        if variants == 0 {
            return Err(WireError::MissingOneof {
                message,
                oneof: oneof.name,
            });
        }
        if variants > 1 {
            return Err(WireError::MultipleOneofVariants {
                message,
                oneof: oneof.name,
            });
        }
    }
    Ok(())
}

fn check_event_body(scanned: &Scanned<'_>) -> Result<(), WireError> {
    let seq = scanned.varint(3);
    let ledger = scanned.bytes(2);
    let prev = scanned.bytes(4);
    if seq.is_none() {
        for (field, value) in [("ledger", ledger), ("prev", prev)] {
            if value.is_some() {
                return Err(WireError::SetAtSeqZero { field });
            }
        }
    } else {
        for (field, value) in [("ledger", ledger), ("prev", prev)] {
            if value.is_none() {
                return Err(WireError::MissingChainField { field });
            }
        }
    }

    let (number, payload) = scanned.oneof().expect("payload is required");
    let is_inception = number == INCEPTION_TAG;
    if is_inception && seq.is_some() {
        return Err(WireError::InceptionPastSeqZero);
    }
    if !is_inception && seq.is_none() {
        return Err(WireError::NonInceptionAtSeqZero);
    }

    let author_key = scanned.bytes(6).expect("author_key is required");
    match number {
        // Seq 0 self-authorizes under its root: the raw root's `active_key`
        // or the identity root's `founder_key` (proposal 002 section 5).
        INCEPTION_TAG => {
            let inception = scanned.subfields(&INCEPTION, payload)?;
            let (root_number, root) = inception.oneof().expect("root is required");
            // The scan already refused every tag but these two.
            let (descriptor, field, name) = match root_number {
                RAW_ROOT_TAG => (&RAW_ROOT, 1, "RawRoot.active_key"),
                _ => (&IDENTITY_ROOT, 2, "IdentityRoot.founder_key"),
            };
            let root = inception.subfields(descriptor, root)?;
            if root.bytes(field) != Some(author_key) {
                return Err(WireError::FieldsMustMatch {
                    first: "EventBody.author_key",
                    second: name,
                });
            }
        }
        // Issuer and subject must differ (proposal 001 section 3.4).
        TRUST_ATTESTATION_TAG => {
            let attestation = scanned.subfields(&TRUST_ATTESTATION, payload)?;
            if ledger.is_some() && attestation.bytes(1) == ledger {
                return Err(WireError::FieldsMustDiffer {
                    first: "TrustAttestation.subject",
                    second: "EventBody.ledger",
                });
            }
        }
        // An invitee equal to the ledger id would shadow the root principal
        // of a raw-rooted ledger (proposal 002 section 4).
        MEMBERSHIP_INVITATION_TAG => {
            let invitation = scanned.subfields(&MEMBERSHIP_INVITATION, payload)?;
            if ledger.is_some() && invitation.bytes(1) == ledger {
                return Err(WireError::FieldsMustDiffer {
                    first: "MembershipInvitation.invitee",
                    second: "EventBody.ledger",
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn check_raw_root(scanned: &Scanned<'_>) -> Result<(), WireError> {
    if scanned.bytes(1) == scanned.bytes(2) {
        return Err(WireError::FieldsMustDiffer {
            first: "RawRoot.active_key",
            second: "RawRoot.reserve_commit",
        });
    }
    Ok(())
}

fn check_identity_root(scanned: &Scanned<'_>) -> Result<(), WireError> {
    check_embedded_inception(
        scanned,
        3,
        (1, "IdentityRoot.founder"),
        (2, "IdentityRoot.founder_key"),
    )
}

fn check_membership_invitation(scanned: &Scanned<'_>) -> Result<(), WireError> {
    check_embedded_inception(
        scanned,
        4,
        (1, "MembershipInvitation.invitee"),
        (2, "MembershipInvitation.invitee_key"),
    )
}

/// The rule of proposal 002 section 8: an event that names another identity
/// embeds that identity's inception, which must verify standalone, carry a
/// raw root, hash to the recorded id and record the key recorded beside it.
fn check_embedded_inception(
    scanned: &Scanned<'_>,
    inception_field: u32,
    id: (u32, &'static str),
    key: (u32, &'static str),
) -> Result<(), WireError> {
    let embedded = scanned
        .bytes(inception_field)
        .expect("the embedded inception is required");
    let inception = inception_standalone(embedded, scanned.depth + 1)?;
    if scanned.bytes(id.0) != Some(&inception.event_id.as_bytes()[..]) {
        return Err(WireError::InceptionIdMismatch { field: id.1 });
    }
    if scanned.bytes(key.0) != Some(&inception.active_key.as_bytes()[..]) {
        return Err(WireError::InceptionKeyMismatch { field: key.1 });
    }
    Ok(())
}

fn check_membership_acceptance(scanned: &Scanned<'_>) -> Result<(), WireError> {
    let blob = scanned.bytes(1).expect("acceptance is required");
    let signature = scanned.bytes(2).expect("signature is required");
    detached_acceptance(blob, signature, "MembershipAcceptance")
}

fn public_key(
    message: &'static str,
    field: &'static str,
    bytes: &[u8],
) -> Result<PublicKey, WireError> {
    let bytes: [u8; ID_BYTES] = bytes
        .try_into()
        .map_err(|_| WireError::InvalidPublicKey { message, field })?;
    PublicKey::from_bytes(&bytes).map_err(|_| WireError::InvalidPublicKey { message, field })
}

fn verify(
    key: &PublicKey,
    input: &[u8],
    signature: &[u8],
    message: &'static str,
    field: &'static str,
) -> Result<(), WireError> {
    let signature: [u8; SIG_BYTES] = signature.try_into().map_err(|_| WireError::WrongLength {
        message,
        field,
        expected: SIG_BYTES,
        actual: signature.len(),
    })?;
    key.verify(input, &Signature::from_bytes(&signature))
        .map_err(|_| WireError::BadSignature { message, field })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ID_STR_LEN;
    use crate::sign::{
        BuiltEvent, Position, Root, build_endpoint_advertisement, build_inception,
        build_trust_attestation, build_witness_config, build_witness_set,
    };
    use iroh_base::SecretKey;
    use mabel_proto::v0::DeclaredKind;

    const T0: u64 = 1_700_000_000_000;

    fn secret(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    /// A raw-rooted ledger: the shape that signs for itself.
    fn raw_rooted() -> BuiltEvent {
        build_inception(
            &secret(1),
            DeclaredKind::Person,
            Root::Raw {
                reserve_key: &secret(2).public(),
            },
            [3u8; NONCE_BYTES],
            T0,
        )
        .expect("builds")
    }

    /// An identity-rooted ledger founded by [`raw_rooted`].
    fn identity_rooted(founder: &BuiltEvent, founder_id: IdentityId) -> BuiltEvent {
        build_inception(
            &secret(1),
            DeclaredKind::Organization,
            Root::Identity {
                founder: founder_id,
                founder_inception: &founder.signed_event,
            },
            [4u8; NONCE_BYTES],
            T0,
        )
        .expect("builds")
    }

    fn attestation() -> BuiltEvent {
        let head = raw_rooted();
        build_trust_attestation(
            &secret(1),
            &Position {
                ledger: head.event_id.into(),
                seq: 1,
                prev: head.event_id,
                prev_timestamp_ms: T0,
            },
            IdentityId::from_bytes([9u8; ID_BYTES]),
            T0,
        )
        .expect("builds")
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

    /// A valid `EventBody` carrying a `TrustAttestation`, which every
    /// envelope-level test mutates.
    fn body() -> Vec<u8> {
        attestation().body
    }

    /// The encoded `Inception` of a raw root, as parts a test can mutate.
    fn raw_root_payload(
        active: &PublicKey,
        reserve: &PublicKey,
        nonce: [u8; NONCE_BYTES],
    ) -> Vec<u8> {
        let mut root = len_field(1, active.as_bytes());
        root.extend_from_slice(&len_field(2, &crate::digest::reserve_commit(reserve)));
        let mut payload = varint_field(1, 1);
        payload.extend_from_slice(&len_field(2, &nonce));
        payload.extend_from_slice(&len_field(RAW_ROOT_TAG, &root));
        payload
    }

    // The seven wire-format classes of proposal 001 section 3.1.

    #[test]
    fn a_valid_event_passes() {
        signed_event(&raw_rooted().signed_event).expect("the built event passes");
        event_body(&raw_rooted().body).expect("the built body passes");
        signed_event(&attestation().signed_event).expect("the built event passes");
    }

    #[test]
    fn unknown_field_numbers_are_rejected() {
        let mut bytes = body();
        bytes.extend_from_slice(&varint_field(7, 1));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::UnknownField {
                message: "EventBody",
                number: 7,
            })
        );
    }

    #[test]
    fn duplicate_non_repeated_fields_are_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&len_field(2, &[1u8; ID_BYTES]));
        bytes.extend_from_slice(&varint_field(3, 1));
        bytes.extend_from_slice(&len_field(4, &[2u8; ID_BYTES]));
        bytes.extend_from_slice(&varint_field(5, T0));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::DuplicateField {
                message: "EventBody",
                field: "author_key",
            })
        );
    }

    #[test]
    fn out_of_order_fields_are_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&len_field(2, &[1u8; ID_BYTES]));
        bytes.extend_from_slice(&varint_field(3, 1));
        bytes.extend_from_slice(&len_field(4, &[2u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&varint_field(5, T0));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::FieldOutOfOrder {
                message: "EventBody",
                number: 5,
            })
        );
    }

    #[test]
    fn repeated_entries_must_be_consecutive() {
        let witnesses = [[1u8; ID_BYTES], [2u8; ID_BYTES]];
        let mut payload = Vec::new();
        payload.extend_from_slice(&len_field(1, &witnesses[0]));
        payload.extend_from_slice(&len_field(1, &witnesses[1]));
        message(&WITNESS_CONFIG, &payload).expect("consecutive entries pass");
    }

    #[test]
    fn non_minimal_varints_are_rejected() {
        // seq 1 written as two bytes.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&len_field(2, &[1u8; ID_BYTES]));
        bytes.extend_from_slice(&key(3, 0));
        bytes.extend_from_slice(&[0x81, 0x00]);
        bytes.extend_from_slice(&len_field(4, &[2u8; ID_BYTES]));
        bytes.extend_from_slice(&varint_field(5, T0));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::NonMinimalVarint {
                message: "EventBody",
            })
        );
    }

    #[test]
    fn varints_wider_than_64_bits_are_rejected() {
        let mut bytes = key(5, 0);
        bytes.extend_from_slice(&[0xff; 10]);
        bytes.push(0x01);
        assert_eq!(
            event_body(&bytes),
            Err(WireError::VarintOverflow {
                message: "EventBody",
            })
        );
    }

    #[test]
    fn wrong_wire_types_are_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&varint_field(2, 7));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::WrongWireType {
                message: "EventBody",
                field: "ledger",
                expected: 2,
                actual: 0,
            })
        );
    }

    #[test]
    fn unrecognised_oneof_variants_are_rejected() {
        // Tags 10 to 19 are all assigned and 20 to 29 are reserved for the
        // deferred payloads, so tag 30 is the first a later version may take
        // (proposal 006 section 3).
        let mut bytes = body();
        bytes.extend_from_slice(&len_field(30, &[]));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::UnknownOneofVariant {
                message: "EventBody",
                oneof: "payload",
                number: 30,
            })
        );
    }

    /// The tags proposal 002 section 9 holds for the deferred payloads read as
    /// unrecognised variants, not as unknown fields.
    #[test]
    fn the_reserved_payload_tags_read_as_unrecognised_variants() {
        for number in [20, 25, 29] {
            let mut bytes = body();
            bytes.extend_from_slice(&len_field(number, &[]));
            assert_eq!(
                event_body(&bytes),
                Err(WireError::UnknownOneofVariant {
                    message: "EventBody",
                    oneof: "payload",
                    number,
                })
            );
        }
    }

    #[test]
    fn unspecified_enums_are_rejected() {
        // An explicit zero and an absent field both mean KIND_UNSPECIFIED.
        let mut payload = varint_field(1, 0);
        payload.extend_from_slice(&len_field(2, &[1u8; NONCE_BYTES]));
        assert_eq!(
            message(&INCEPTION, &payload),
            Err(WireError::UnspecifiedEnum {
                message: "Inception",
                field: "kind",
            })
        );

        let absent = len_field(2, &[1u8; NONCE_BYTES]);
        assert_eq!(
            message(&INCEPTION, &absent),
            Err(WireError::UnspecifiedEnum {
                message: "Inception",
                field: "kind",
            })
        );
    }

    /// Declared kind gates nothing, so every defined value is accepted with
    /// either root (proposal 002 section 3).
    #[test]
    fn every_declared_kind_is_accepted_with_a_raw_root() {
        for kind in [
            DeclaredKind::Person,
            DeclaredKind::Organization,
            DeclaredKind::Agent,
            DeclaredKind::Service,
        ] {
            let built = build_inception(
                &secret(1),
                kind,
                Root::Raw {
                    reserve_key: &secret(2).public(),
                },
                [3u8; NONCE_BYTES],
                T0,
            )
            .expect("builds");
            signed_event(&built.signed_event)
                .unwrap_or_else(|err| panic!("{} is rejected: {err}", kind.as_str_name()));
        }

        let mut payload =
            raw_root_payload(&secret(1).public(), &secret(2).public(), [3u8; NONCE_BYTES]);
        // Kind 5 is not a value this version defines.
        payload.splice(0..2, varint_field(1, 5));
        assert_eq!(
            message(&INCEPTION, &payload),
            Err(WireError::EnumValue {
                message: "Inception",
                field: "kind",
                value: 5,
            })
        );
    }

    #[test]
    fn an_inception_with_no_root_is_rejected() {
        let mut payload = varint_field(1, 1);
        payload.extend_from_slice(&len_field(2, &[3u8; NONCE_BYTES]));
        assert_eq!(
            message(&INCEPTION, &payload),
            Err(WireError::MissingOneof {
                message: "Inception",
                oneof: "root",
            })
        );
    }

    #[test]
    fn an_inception_with_two_roots_is_rejected() {
        let founder = raw_rooted();
        let mut identity_root = len_field(1, founder.event_id.as_bytes());
        identity_root.extend_from_slice(&len_field(2, secret(1).public().as_bytes()));
        identity_root.extend_from_slice(&len_field(3, &founder.signed_event));

        let mut payload =
            raw_root_payload(&secret(1).public(), &secret(2).public(), [3u8; NONCE_BYTES]);
        payload.extend_from_slice(&len_field(IDENTITY_ROOT_TAG, &identity_root));
        assert_eq!(
            message(&INCEPTION, &payload),
            Err(WireError::MultipleOneofVariants {
                message: "Inception",
                oneof: "root",
            })
        );
    }

    #[test]
    fn truncated_input_is_rejected() {
        let bytes = raw_rooted().signed_event;
        for cut in [1, bytes.len() / 2, bytes.len() - 1] {
            let err = signed_event(&bytes[..cut]).expect_err("truncated input is rejected");
            assert!(
                matches!(err, WireError::Truncated { .. }),
                "cut at {cut} gave {err}"
            );
        }
        // A length that claims more than the input holds allocates nothing.
        let mut claim = key(1, 2);
        claim.extend_from_slice(&varint(4000));
        claim.extend_from_slice(&[0u8; 4]);
        assert_eq!(
            signed_event(&claim),
            Err(WireError::Truncated {
                message: "SignedEvent",
            })
        );
    }

    // Caps.

    #[test]
    fn oversize_events_are_rejected_before_scanning() {
        let bytes = vec![0u8; MAX_EVENT_BYTES + 1];
        assert_eq!(
            signed_event(&bytes),
            Err(WireError::MessageTooLarge {
                message: "SignedEvent",
                len: MAX_EVENT_BYTES + 1,
                cap: MAX_EVENT_BYTES,
            })
        );
    }

    #[test]
    fn oversize_embedded_inceptions_are_rejected() {
        let mut root = len_field(1, &[1u8; ID_BYTES]);
        root.extend_from_slice(&len_field(2, &[2u8; ID_BYTES]));
        root.extend_from_slice(&len_field(3, &[0u8; MAX_EMBEDDED_INCEPTION_BYTES + 1]));
        assert_eq!(
            message(&IDENTITY_ROOT, &root),
            Err(WireError::FieldTooLong {
                message: "IdentityRoot",
                field: "founder_inception",
                len: MAX_EMBEDDED_INCEPTION_BYTES + 1,
                cap: MAX_EMBEDDED_INCEPTION_BYTES,
            })
        );
    }

    /// A `SignedEvent` carrying an identity-rooted `Inception` whose
    /// `founder_inception` is `inner`. Well formed enough to reach the
    /// embedded-inception check, which is what recurses.
    fn identity_root_around(inner: &[u8]) -> Vec<u8> {
        let author_key = [7u8; ID_BYTES];
        let mut root = len_field(1, &[8u8; ID_BYTES]);
        root.extend_from_slice(&len_field(2, &author_key));
        root.extend_from_slice(&len_field(3, inner));

        let mut payload = varint_field(1, 2);
        payload.extend_from_slice(&len_field(2, &[9u8; NONCE_BYTES]));
        payload.extend_from_slice(&len_field(IDENTITY_ROOT_TAG, &root));

        let mut body = varint_field(5, T0);
        body.extend_from_slice(&len_field(6, &author_key));
        body.extend_from_slice(&len_field(INCEPTION_TAG, &payload));

        let mut signed = len_field(1, &body);
        signed.extend_from_slice(&len_field(2, &[0u8; SIG_BYTES]));
        signed
    }

    #[test]
    fn deep_nesting_is_rejected() {
        // Each embedded inception costs four levels, so three of them reach
        // the guard before any signature is checked.
        let mut bytes = vec![0xffu8; 8];
        for _ in 0..3 {
            bytes = identity_root_around(&bytes);
        }
        assert!(bytes.len() <= MAX_EVENT_BYTES);
        assert_eq!(signed_event(&bytes), Err(WireError::TooDeeplyNested));

        // Two of them stop short of the guard and fail on their content.
        let shallow = identity_root_around(&identity_root_around(&[0xffu8; 8]));
        assert_ne!(signed_event(&shallow), Err(WireError::TooDeeplyNested));
    }

    // Field-table rows the descriptors alone do not express.

    #[test]
    fn a_forbidden_version_is_rejected() {
        let mut bytes = varint_field(1, 1);
        bytes.extend_from_slice(&body());
        assert_eq!(
            event_body(&bytes),
            Err(WireError::FieldForbidden {
                message: "EventBody",
                field: "version",
            })
        );
    }

    #[test]
    fn a_default_value_on_the_wire_is_rejected() {
        let mut bytes = varint_field(3, 0);
        bytes.extend_from_slice(&varint_field(5, T0));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::DefaultValueEncoded {
                message: "EventBody",
                field: "seq",
            })
        );
    }

    #[test]
    fn timestamps_outside_the_bounds_are_rejected() {
        let mut bytes = len_field(2, &[1u8; ID_BYTES]);
        bytes.extend_from_slice(&varint_field(3, 1));
        bytes.extend_from_slice(&len_field(4, &[2u8; ID_BYTES]));
        bytes.extend_from_slice(&varint_field(5, MAX_TIMESTAMP_MS + 1));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::ValueOutOfRange {
                message: "EventBody",
                field: "timestamp_ms",
                value: MAX_TIMESTAMP_MS + 1,
                min: 1,
                max: MAX_TIMESTAMP_MS,
            })
        );
    }

    #[test]
    fn a_missing_payload_is_rejected() {
        let mut bytes = varint_field(5, T0);
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::MissingOneof {
                message: "EventBody",
                oneof: "payload",
            })
        );
    }

    #[test]
    fn two_payload_variants_are_rejected() {
        let mut bytes = varint_field(5, T0);
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        bytes.extend_from_slice(&len_field(13, &len_field(1, &[5u8; ID_BYTES])));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::MultipleOneofVariants {
                message: "EventBody",
                oneof: "payload",
            })
        );
    }

    #[test]
    fn chain_fields_follow_the_sequence() {
        let head = raw_rooted();
        let mut bytes = len_field(2, head.event_id.as_bytes());
        bytes.extend_from_slice(&varint_field(5, T0));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::SetAtSeqZero { field: "ledger" })
        );

        let mut bytes = varint_field(3, 1);
        bytes.extend_from_slice(&varint_field(5, T0));
        bytes.extend_from_slice(&len_field(6, &[3u8; ID_BYTES]));
        bytes.extend_from_slice(&len_field(
            TRUST_ATTESTATION_TAG,
            &len_field(1, &[4u8; ID_BYTES]),
        ));
        assert_eq!(
            event_body(&bytes),
            Err(WireError::MissingChainField { field: "ledger" })
        );
    }

    #[test]
    fn witness_sets_are_bounded_and_distinct() {
        let empty: Vec<u8> = Vec::new();
        assert_eq!(
            message(&WITNESS_CONFIG, &empty),
            Err(WireError::RepeatedCount {
                message: "WitnessConfig",
                field: "witnesses",
                count: 0,
                min: 1,
                max: MAX_WITNESSES,
            })
        );

        let mut repeated = len_field(1, &[1u8; ID_BYTES]);
        repeated.extend_from_slice(&len_field(1, &[1u8; ID_BYTES]));
        assert_eq!(
            message(&WITNESS_CONFIG, &repeated),
            Err(WireError::RepeatedDuplicate {
                message: "WitnessConfig",
                field: "witnesses",
            })
        );

        let many: Vec<u8> = (0..=MAX_WITNESSES as u8)
            .flat_map(|seed| len_field(1, &[seed; ID_BYTES]))
            .collect();
        assert_eq!(
            message(&WITNESS_CONFIG, &many),
            Err(WireError::RepeatedCount {
                message: "WitnessConfig",
                field: "witnesses",
                count: MAX_WITNESSES + 1,
                min: 1,
                max: MAX_WITNESSES,
            })
        );
    }

    /// Both new lists may be empty and both refuse a repeat and a seventeenth
    /// or ninth entry (proposal 006 sections 1, 2 and 3).
    #[test]
    fn the_witness_set_and_the_advertisement_are_bounded_and_distinct() {
        let empty: Vec<u8> = Vec::new();
        message(&WITNESS_SET, &empty).expect("an empty witness set is legal");
        message(&ENDPOINT_ADVERTISEMENT, &empty).expect("an empty advertisement is legal");

        for (descriptor, name, field, max) in [
            (&WITNESS_SET, "WitnessSet", "witnesses", MAX_WITNESSES),
            (
                &ENDPOINT_ADVERTISEMENT,
                "EndpointAdvertisement",
                "endpoints",
                MAX_ENDPOINTS,
            ),
        ] {
            let mut repeated = len_field(1, &[1u8; ID_BYTES]);
            repeated.extend_from_slice(&len_field(1, &[1u8; ID_BYTES]));
            assert_eq!(
                message(descriptor, &repeated),
                Err(WireError::RepeatedDuplicate {
                    message: name,
                    field,
                })
            );

            let many: Vec<u8> = (0..=max as u8)
                .flat_map(|seed| len_field(1, &[seed; ID_BYTES]))
                .collect();
            assert_eq!(
                message(descriptor, &many),
                Err(WireError::RepeatedCount {
                    message: name,
                    field,
                    count: max + 1,
                    min: 0,
                    max,
                })
            );

            let short = len_field(1, &[1u8; ID_BYTES - 1]);
            assert_eq!(
                message(descriptor, &short),
                Err(WireError::WrongLength {
                    message: name,
                    field,
                    expected: ID_BYTES,
                    actual: ID_BYTES - 1,
                })
            );
        }
    }

    /// A ledger naming itself in its own witness set is legal, which is how a
    /// self-hosted identity says it keeps its own chain (proposal 006
    /// section 1).
    #[test]
    fn a_witness_set_from_the_signing_path_may_name_the_ledger_itself() {
        let head = raw_rooted();
        let ledger: IdentityId = head.event_id.into();
        let built = build_witness_set(
            &secret(1),
            &Position {
                ledger,
                seq: 1,
                prev: head.event_id,
                prev_timestamp_ms: T0,
            },
            &[ledger],
            T0,
        )
        .expect("builds");
        signed_event(&built.signed_event).expect("passes");

        let empty = build_witness_set(
            &secret(1),
            &Position {
                ledger,
                seq: 1,
                prev: head.event_id,
                prev_timestamp_ms: T0,
            },
            &[],
            T0,
        )
        .expect("builds");
        signed_event(&empty.signed_event).expect("an empty set passes");
    }

    #[test]
    fn an_endpoint_advertisement_from_the_signing_path_passes() {
        let head = raw_rooted();
        let at = Position {
            ledger: head.event_id.into(),
            seq: 1,
            prev: head.event_id,
            prev_timestamp_ms: T0,
        };
        let built = build_endpoint_advertisement(
            &secret(1),
            &at,
            &[secret(7).public(), secret(8).public()],
            T0,
        )
        .expect("builds");
        signed_event(&built.signed_event).expect("passes");

        let empty =
            build_endpoint_advertisement(&secret(1), &at, &[], T0).expect("an empty list builds");
        signed_event(&empty.signed_event).expect("an empty advertisement passes");
    }

    #[test]
    fn a_witness_config_from_the_signing_path_passes() {
        let head = raw_rooted();
        let built = build_witness_config(
            &secret(1),
            &Position {
                ledger: head.event_id.into(),
                seq: 1,
                prev: head.event_id,
                prev_timestamp_ms: T0,
            },
            &[secret(7).public(), secret(8).public()],
            T0,
        )
        .expect("builds");
        signed_event(&built.signed_event).expect("passes");
    }

    // The seq-0 author_key row of proposal 002 section 8.

    #[test]
    fn a_raw_rooted_seq_zero_event_is_signed_by_its_active_key() {
        // The author key is Bob's while the raw root records Alice's.
        let mut body = varint_field(5, T0);
        body.extend_from_slice(&len_field(6, secret(9).public().as_bytes()));
        body.extend_from_slice(&len_field(
            INCEPTION_TAG,
            &raw_root_payload(&secret(1).public(), &secret(2).public(), [3u8; NONCE_BYTES]),
        ));
        let mut signed = len_field(1, &body);
        signed.extend_from_slice(&len_field(2, &[0u8; SIG_BYTES]));
        assert_eq!(
            signed_event(&signed),
            Err(WireError::FieldsMustMatch {
                first: "EventBody.author_key",
                second: "RawRoot.active_key",
            })
        );
    }

    #[test]
    fn an_identity_rooted_seq_zero_event_is_signed_by_the_founder_key() {
        let founder = raw_rooted();
        let mut root = len_field(1, founder.event_id.as_bytes());
        root.extend_from_slice(&len_field(2, secret(1).public().as_bytes()));
        root.extend_from_slice(&len_field(3, &founder.signed_event));
        let mut payload = varint_field(1, 2);
        payload.extend_from_slice(&len_field(2, &[4u8; NONCE_BYTES]));
        payload.extend_from_slice(&len_field(IDENTITY_ROOT_TAG, &root));

        let mut body = varint_field(5, T0);
        body.extend_from_slice(&len_field(6, secret(9).public().as_bytes()));
        body.extend_from_slice(&len_field(INCEPTION_TAG, &payload));
        let mut signed = len_field(1, &body);
        signed.extend_from_slice(&len_field(2, &[0u8; SIG_BYTES]));
        assert_eq!(
            signed_event(&signed),
            Err(WireError::FieldsMustMatch {
                first: "EventBody.author_key",
                second: "IdentityRoot.founder_key",
            })
        );
    }

    // verify_inception_standalone.

    #[test]
    fn a_standalone_inception_returns_its_id_and_root_key() {
        let built = raw_rooted();
        let inception = verify_inception_standalone(&built.signed_event).expect("verifies");
        assert_eq!(inception.event_id, built.event_id);
        assert_eq!(inception.identity, built.event_id.into());
        assert_eq!(inception.active_key, secret(1).public());
    }

    #[test]
    fn a_standalone_inception_rejects_a_broken_signature() {
        let mut bytes = raw_rooted().signed_event;
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert_eq!(
            verify_inception_standalone(&bytes),
            Err(WireError::BadSignature {
                message: "SignedEvent",
                field: "signature",
            })
        );
    }

    /// Declared kind is ignored; the raw root is the requirement.
    #[test]
    fn a_standalone_inception_rejects_an_identity_root_and_ignores_the_kind() {
        let founder = raw_rooted();
        let organization = identity_rooted(&founder, founder.event_id.into());
        assert_eq!(
            verify_inception_standalone(&organization.signed_event),
            Err(WireError::EmbeddedInceptionNotRawRooted)
        );
        // The identity-rooted event itself is valid: its embedded founder
        // inception is raw-rooted.
        signed_event(&organization.signed_event).expect("passes");

        // A raw root whose declared kind is ORGANIZATION verifies standalone.
        let mislabelled = build_inception(
            &secret(1),
            DeclaredKind::Organization,
            Root::Raw {
                reserve_key: &secret(2).public(),
            },
            [3u8; NONCE_BYTES],
            T0,
        )
        .expect("builds");
        verify_inception_standalone(&mislabelled.signed_event)
            .expect("declared kind is not checked");
    }

    #[test]
    fn an_embedded_inception_must_match_the_id_recorded_beside_it() {
        let founder = raw_rooted();
        let organization = identity_rooted(&founder, IdentityId::from_bytes([0xaa; ID_BYTES]));
        assert_eq!(
            signed_event(&organization.signed_event),
            Err(WireError::InceptionIdMismatch {
                field: "IdentityRoot.founder",
            })
        );
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            WireError::TooDeeplyNested.code(),
            "too_deeply_nested",
            "codes are the cross-language name of a rejection class"
        );
        assert_eq!(
            WireError::UnknownField {
                message: "EventBody",
                number: 7
            }
            .code(),
            "unknown_field"
        );
        assert_eq!(
            WireError::EmbeddedInceptionNotRawRooted.code(),
            "embedded_inception_not_raw_rooted"
        );
        assert_eq!(
            WireError::InvalidDisplayName {
                message: "ProfileUpdate",
                field: "display_name",
                reason: "it parses as an identity id",
            }
            .code(),
            "invalid_display_name"
        );
        assert_eq!(
            WireError::InvalidHostname {
                message: "ProfileUpdate",
                field: "hostname",
                reason: "it holds no dot",
            }
            .code(),
            "invalid_hostname"
        );
        assert_eq!(
            WireError::InvalidEmail {
                message: "ProfileUpdate",
                field: "email",
                reason: "it holds no at sign",
            }
            .code(),
            "invalid_email"
        );
    }

    /// An encoded `ProfileUpdate`. An empty argument is an absent field, which
    /// is how the canonical encoding spells "unset".
    fn profile_update(display_name: &[u8], hostname: &[u8], email: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if !display_name.is_empty() {
            out.extend_from_slice(&len_field(1, display_name));
        }
        if !hostname.is_empty() {
            out.extend_from_slice(&len_field(2, hostname));
        }
        if !email.is_empty() {
            out.extend_from_slice(&len_field(3, email));
        }
        out
    }

    fn profile(display_name: &str, hostname: &str) -> Result<(), WireError> {
        message(
            &PROFILE_UPDATE,
            &profile_update(display_name.as_bytes(), hostname.as_bytes(), b""),
        )
    }

    /// A `ProfileUpdate` carrying an email and nothing else.
    fn profile_email(email: &str) -> Result<(), WireError> {
        message(&PROFILE_UPDATE, &profile_update(b"", b"", email.as_bytes()))
    }

    #[test]
    fn a_profile_update_takes_any_of_its_three_fields_or_none() {
        profile("Alice Ashworth", "alice.example").expect("both names pass");
        profile("Alice Ashworth", "").expect("a display name alone passes");
        profile("", "alice.example").expect("a hostname alone passes");
        profile_email("alice@alice.example").expect("an email alone passes");
        message(
            &PROFILE_UPDATE,
            &profile_update(b"Alice Ashworth", b"alice.example", b"alice@alice.example"),
        )
        .expect("all three pass");
        // Clearing every field is a zero-length payload, which is legal
        // protobuf and means "unset all three" (proposal 003 section 1,
        // proposal 005).
        profile("", "").expect("an empty payload passes");
    }

    #[test]
    fn an_email_holds_exactly_one_at_sign_with_bytes_on_each_side() {
        for address in [
            "alice@alice.example",
            "a@b",
            "alice+zines@alice.example",
            "アリス@例え.example",
        ] {
            profile_email(address).unwrap_or_else(|err| panic!("{address} must pass: {err}"));
        }

        for (address, reason) in [
            ("alice.example", "it holds no at sign"),
            ("alice@bob@alice.example", "it holds more than one at sign"),
            ("@alice.example", "it holds nothing before the at sign"),
            ("alice@", "it holds nothing after the at sign"),
            ("@", "it holds nothing before the at sign"),
        ] {
            assert_eq!(
                profile_email(address),
                Err(WireError::InvalidEmail {
                    message: "ProfileUpdate",
                    field: "email",
                    reason,
                }),
                "{address} must be refused"
            );
        }
    }

    #[test]
    fn an_email_holding_a_forbidden_codepoint_is_refused() {
        for (address, reason) in [
            ("alice\t@alice.example", "it holds a C0 control character"),
            (
                "alice\u{85}@alice.example",
                "it holds a C1 control character",
            ),
            (
                "alice\u{202e}@alice.example",
                "it holds a bidi control character",
            ),
            (
                "alice\u{feff}@alice.example",
                "it holds a zero-width or invisible format character",
            ),
        ] {
            assert_eq!(
                profile_email(address),
                Err(WireError::InvalidEmail {
                    message: "ProfileUpdate",
                    field: "email",
                    reason,
                }),
                "{address:?} must be refused"
            );
        }
    }

    #[test]
    fn an_email_is_utf8_and_at_most_254_bytes() {
        assert_eq!(
            message(
                &PROFILE_UPDATE,
                &profile_update(b"", b"", b"alice@alice.exampl\xff")
            ),
            Err(WireError::InvalidUtf8 {
                message: "ProfileUpdate",
                field: "email",
            })
        );
        let local = "a".repeat(MAX_EMAIL_BYTES - "@alice.example".len());
        let at_cap = format!("{local}@alice.example");
        assert_eq!(at_cap.len(), MAX_EMAIL_BYTES);
        profile_email(&at_cap).expect("254 bytes pass");
        assert_eq!(
            profile_email(&format!("a{at_cap}")),
            Err(WireError::FieldTooLong {
                message: "ProfileUpdate",
                field: "email",
                len: MAX_EMAIL_BYTES + 1,
                cap: MAX_EMAIL_BYTES,
            })
        );
    }

    #[test]
    fn a_display_name_holding_a_forbidden_codepoint_is_refused() {
        for (name, reason) in [
            ("Alice\tAshworth", "it holds a C0 control character"),
            ("Alice\u{7f}Ashworth", "it holds a C0 control character"),
            ("Alice\u{85}Ashworth", "it holds a C1 control character"),
            ("Alice\u{202a}Ashworth", "it holds a bidi control character"),
            ("Alice\u{202e}Ashworth", "it holds a bidi control character"),
            ("Alice\u{2066}Ashworth", "it holds a bidi control character"),
            ("Alice\u{2069}Ashworth", "it holds a bidi control character"),
            (
                "Alice\u{200b}Ashworth",
                "it holds a zero-width or invisible format character",
            ),
            (
                "Alice\u{200f}Ashworth",
                "it holds a zero-width or invisible format character",
            ),
            (
                "Alice\u{2060}Ashworth",
                "it holds a zero-width or invisible format character",
            ),
            (
                "Alice\u{feff}Ashworth",
                "it holds a zero-width or invisible format character",
            ),
            (" Alice", "it has leading or trailing whitespace"),
            ("Alice ", "it has leading or trailing whitespace"),
        ] {
            assert_eq!(
                profile(name, ""),
                Err(WireError::InvalidDisplayName {
                    message: "ProfileUpdate",
                    field: "display_name",
                    reason,
                }),
                "{name:?} must be refused"
            );
        }
        // An interior space is a name, not a rule break.
        profile("Alice Ashworth", "").expect("an interior space passes");
    }

    #[test]
    fn a_display_name_that_parses_as_an_identity_id_is_refused() {
        let id = IdentityId::from_bytes([0x42; ID_BYTES]).to_string();
        for spelling in [id.clone(), id.to_ascii_uppercase()] {
            assert_eq!(
                profile(&spelling, ""),
                Err(WireError::InvalidDisplayName {
                    message: "ProfileUpdate",
                    field: "display_name",
                    reason: "it parses as an identity id",
                })
            );
        }
        // 52 characters that are not base32 are an ordinary name.
        profile(&"z".repeat(ID_STR_LEN), "").expect("not an id");
    }

    #[test]
    fn a_display_name_is_utf8_and_at_most_64_bytes() {
        assert_eq!(
            message(&PROFILE_UPDATE, &profile_update(b"Alice\xff", b"", b"")),
            Err(WireError::InvalidUtf8 {
                message: "ProfileUpdate",
                field: "display_name",
            })
        );
        profile(&"a".repeat(MAX_DISPLAY_NAME_BYTES), "").expect("64 bytes pass");
        assert_eq!(
            profile(&"a".repeat(MAX_DISPLAY_NAME_BYTES + 1), ""),
            Err(WireError::FieldTooLong {
                message: "ProfileUpdate",
                field: "display_name",
                len: MAX_DISPLAY_NAME_BYTES + 1,
                cap: MAX_DISPLAY_NAME_BYTES,
            })
        );
        // A 64-byte cap counts bytes, not characters.
        assert!(matches!(
            profile(&"é".repeat(33), ""),
            Err(WireError::FieldTooLong { len: 66, .. })
        ));
    }

    #[test]
    fn an_explicitly_encoded_empty_name_is_refused() {
        for (number, field) in [(1, "display_name"), (2, "hostname"), (3, "email")] {
            assert_eq!(
                message(&PROFILE_UPDATE, &len_field(number, b"")),
                Err(WireError::DefaultValueEncoded {
                    message: "ProfileUpdate",
                    field,
                })
            );
        }
    }

    #[test]
    fn a_hostname_follows_the_syntax_of_proposal_003_section_2() {
        for host in [
            "alice.example",
            "a.b",
            "alice.example.com",
            "a-b.c-d.example",
            "0.1.example",
            &format!("{}.example", "a".repeat(63)),
        ] {
            profile("", host).unwrap_or_else(|err| panic!("{host} must pass: {err}"));
        }

        for (host, reason) in [
            ("älice.example", "it holds a character outside ASCII"),
            ("Alice.example", "it holds an uppercase letter"),
            ("alice.EXAMPLE", "it holds an uppercase letter"),
            ("alice.example.", "it ends with a dot"),
            ("localhost", "it holds no dot"),
            ("alice..example", "it holds an empty label"),
            (".alice.example", "it holds an empty label"),
            (
                "-alice.example",
                "a label does not start and end with a letter or digit",
            ),
            (
                "alice-.example",
                "a label does not start and end with a letter or digit",
            ),
            (
                "ali_ce.example",
                "a label holds a character outside [a-z0-9-]",
            ),
        ] {
            assert_eq!(
                profile("", host),
                Err(WireError::InvalidHostname {
                    message: "ProfileUpdate",
                    field: "hostname",
                    reason,
                }),
                "{host} must be refused"
            );
        }

        let long_label = format!("{}.example", "a".repeat(64));
        assert_eq!(
            profile("", &long_label),
            Err(WireError::InvalidHostname {
                message: "ProfileUpdate",
                field: "hostname",
                reason: "it holds a label over 63 bytes",
            })
        );
    }

    #[test]
    fn a_hostname_is_at_most_246_bytes() {
        let label = "a".repeat(63);
        let under = format!("{label}.{label}.{label}.{}", "a".repeat(54));
        assert_eq!(under.len(), MAX_HOSTNAME_BYTES);
        profile("", &under).expect("246 bytes pass");

        let over = format!("{under}a");
        assert_eq!(
            profile("", &over),
            Err(WireError::FieldTooLong {
                message: "ProfileUpdate",
                field: "hostname",
                len: MAX_HOSTNAME_BYTES + 1,
                cap: MAX_HOSTNAME_BYTES,
            })
        );
    }

    /// Every payload sits at the tag `ledger.proto` gives it, tags 10 to 19 are
    /// all spent and 20 to 29 stay reserved (proposal 006 section 3).
    #[test]
    fn the_payload_tags_are_spent_to_nineteen() {
        for (number, name) in [
            (PROFILE_UPDATE_TAG, "profile_update"),
            (18, "endpoint_advertisement"),
            (19, "witness_set"),
        ] {
            let field = EVENT_BODY
                .field(number)
                .unwrap_or_else(|| panic!("EventBody declares tag {number}"));
            assert_eq!(field.name, name);
            assert_eq!(field.cardinality, Cardinality::Variant);
        }
        for number in [20, 29] {
            assert_eq!(
                EVENT_BODY.field(number).map(|field| field.name),
                None,
                "tag {number} must stay reserved"
            );
        }
    }
}

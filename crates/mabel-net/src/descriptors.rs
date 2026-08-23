//! Field tables for the sync messages of `proto/mabel/v0/sync.proto`.
//!
//! These are [`mabel_core::validate`] descriptors, so a sync frame passes the
//! same scanner an event does: unknown field numbers, duplicate non-repeated
//! fields, out-of-order fields, non-minimal varints, wrong wire types,
//! unrecognised `oneof` variants and encoded proto3 defaults are all
//! rejected before anything decodes the frame.
//!
//! Caps live in the descriptors rather than in the handlers, so a frame that
//! exceeds one is refused during the scan, before an allocation sized by it
//! (proposal 001 section 5, pitfall 7).

use mabel_core::validate::{
    Cardinality, EnumValue, FieldDescriptor, FieldKind, MessageDescriptor, Oneof, SIGNED_EVENT,
};
use mabel_core::{ID_BYTES, MAX_TIMESTAMP_MS};

use crate::{
    MAX_EVENT_BYTES, MAX_FORKS_LIMIT, MAX_FRAME_BYTES, MAX_GET_LIMIT, MAX_LIST_LIMIT,
    MAX_PUSH_BYTES, MAX_PUSH_EVENTS, MAX_REJECT_MSG_BYTES,
};

/// A 32-byte id, ledger id, event id or endpoint id.
const ID: FieldKind = FieldKind::Bytes {
    exact: Some(ID_BYTES),
    max: ID_BYTES,
};

/// A `uint64` counter, which the canonical encoding omits when it is 0.
const COUNT64: FieldKind = FieldKind::Varint {
    min: 1,
    max: u64::MAX,
};

/// A `uint32` counter, which the canonical encoding omits when it is 0.
const COUNT32: FieldKind = FieldKind::Varint {
    min: 1,
    max: u32::MAX as u64,
};

/// A millisecond timestamp, bounded like every other mabel timestamp.
const TIMESTAMP: FieldKind = FieldKind::Varint {
    min: 1,
    max: MAX_TIMESTAMP_MS,
};

/// A `bool`, which the canonical encoding only ever writes as 1.
const FLAG: FieldKind = FieldKind::Varint { min: 1, max: 1 };

/// A `SignedEvent` submessage, capped at [`MAX_EVENT_BYTES`] by its own
/// descriptor. Detached: an event's bytes are stored and served verbatim,
/// so its nesting budget is its own, not the frame's (an identity-rooted
/// inception reaches depth 7 by itself).
const EVENT: FieldKind = FieldKind::Detached {
    descriptor: &SIGNED_EVENT,
};

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

const fn repeated(
    number: u32,
    name: &'static str,
    min: usize,
    max: usize,
    kind: FieldKind,
) -> FieldDescriptor {
    FieldDescriptor {
        number,
        name,
        cardinality: Cardinality::Repeated {
            min,
            max,
            distinct: false,
        },
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

/// The smallest cap that fits a request carrying only ids and counters.
const SMALL_REQUEST_BYTES: usize = 128;

/// `HeadReq`.
pub static HEAD_REQ: MessageDescriptor = MessageDescriptor {
    name: "HeadReq",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[required(1, "ledger", ID)],
    oneof: None,
    check: None,
};

/// `GetReq`. `limit` is clamped by the server rather than rejected here, so a
/// client that asks for more than [`MAX_GET_LIMIT`] gets a short answer
/// instead of an error.
pub static GET_REQ: MessageDescriptor = MessageDescriptor {
    name: "GetReq",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[
        required(1, "ledger", ID),
        optional(2, "since", COUNT64),
        optional(3, "limit", COUNT32),
    ],
    oneof: None,
    check: None,
};

/// `PushReq`, capped at [`MAX_PUSH_EVENTS`] events and [`MAX_PUSH_BYTES`].
pub static PUSH_REQ: MessageDescriptor = MessageDescriptor {
    name: "PushReq",
    max_bytes: MAX_PUSH_BYTES,
    fields: &[
        required(1, "ledger", ID),
        repeated(2, "events", 1, MAX_PUSH_EVENTS, EVENT),
    ],
    oneof: None,
    check: None,
};

/// `ListReq`.
pub static LIST_REQ: MessageDescriptor = MessageDescriptor {
    name: "ListReq",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[
        optional(1, "offset", COUNT32),
        optional(2, "limit", COUNT32),
    ],
    oneof: None,
    check: None,
};

/// `ForksReq`. An absent `ledger` means every ledger.
pub static FORKS_REQ: MessageDescriptor = MessageDescriptor {
    name: "ForksReq",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[
        optional(1, "ledger", ID),
        optional(2, "offset", COUNT32),
        optional(3, "limit", COUNT32),
    ],
    oneof: None,
    check: None,
};

/// `Request`, the frame a client sends.
///
/// Every field number belongs to the `kind` `oneof`, so a variant a later
/// version adds arrives as `WireError::UnknownOneofVariant` and is answered
/// `UNSUPPORTED` rather than `MALFORMED`.
pub static REQUEST: MessageDescriptor = MessageDescriptor {
    name: "Request",
    max_bytes: MAX_FRAME_BYTES,
    fields: &[
        variant(1, "head", &HEAD_REQ),
        variant(2, "get", &GET_REQ),
        variant(3, "push", &PUSH_REQ),
        variant(4, "list", &LIST_REQ),
        variant(5, "forks", &FORKS_REQ),
    ],
    oneof: Some(Oneof {
        name: "kind",
        first_number: 1,
    }),
    check: None,
};

/// `HeadResp`.
pub static HEAD_RESP: MessageDescriptor = MessageDescriptor {
    name: "HeadResp",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[
        optional(1, "head_seq", COUNT64),
        required(2, "head_event", ID),
        optional(3, "updated_ms", TIMESTAMP),
    ],
    oneof: None,
    check: None,
};

/// `EventsResp`.
pub static EVENTS_RESP: MessageDescriptor = MessageDescriptor {
    name: "EventsResp",
    max_bytes: MAX_FRAME_BYTES,
    fields: &[
        repeated(1, "events", 0, MAX_GET_LIMIT as usize, EVENT),
        optional(2, "head_seq", COUNT64),
        optional(3, "more", FLAG),
    ],
    oneof: None,
    check: None,
};

/// `AcceptedResp`.
pub static ACCEPTED_RESP: MessageDescriptor = MessageDescriptor {
    name: "AcceptedResp",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[
        optional(1, "head_seq", COUNT64),
        optional(2, "stored", COUNT32),
    ],
    oneof: None,
    check: None,
};

/// `LedgerSummary`, one entry of a `LedgersResp`.
///
/// `declared_kind` is advisory and carries every value of `DeclaredKind`,
/// `AGENT` and `SERVICE` included (proposal 002 section 3).
pub static LEDGER_SUMMARY: MessageDescriptor = MessageDescriptor {
    name: "LedgerSummary",
    max_bytes: SMALL_REQUEST_BYTES,
    fields: &[
        required(1, "ledger", ID),
        required(
            2,
            "declared_kind",
            FieldKind::Enum {
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
            },
        ),
        optional(3, "head_seq", COUNT64),
        required(4, "head_event", ID),
        optional(5, "event_count", COUNT64),
        optional(6, "first_seen_ms", TIMESTAMP),
        optional(7, "updated_ms", TIMESTAMP),
        optional(8, "fork_count", COUNT32),
        optional(9, "forks_truncated", FLAG),
    ],
    oneof: None,
    check: None,
};

/// `LedgersResp`.
pub static LEDGERS_RESP: MessageDescriptor = MessageDescriptor {
    name: "LedgersResp",
    max_bytes: MAX_FRAME_BYTES,
    fields: &[
        repeated(
            1,
            "entries",
            0,
            MAX_LIST_LIMIT as usize,
            FieldKind::Message {
                descriptor: &LEDGER_SUMMARY,
            },
        ),
        optional(2, "more", FLAG),
    ],
    oneof: None,
    check: None,
};

/// `ForkRecord`, one entry of a `ForksResp`.
///
/// A fork at seq 0 cannot exist: the ledger id is the event id of the seq-0
/// event, so two distinct seq-0 events are two distinct ledgers.
pub static FORK_RECORD: MessageDescriptor = MessageDescriptor {
    name: "ForkRecord",
    max_bytes: 2 * MAX_EVENT_BYTES + SMALL_REQUEST_BYTES,
    fields: &[
        required(1, "ledger", ID),
        required(2, "seq", COUNT64),
        required(3, "kept", EVENT),
        required(4, "conflicting", EVENT),
        optional(5, "observed_ms", TIMESTAMP),
        optional(6, "source_endpoint", ID),
    ],
    oneof: None,
    check: None,
};

/// `ForksResp`.
pub static FORKS_RESP: MessageDescriptor = MessageDescriptor {
    name: "ForksResp",
    max_bytes: MAX_FRAME_BYTES,
    fields: &[
        repeated(
            1,
            "entries",
            0,
            MAX_FORKS_LIMIT as usize,
            FieldKind::Message {
                descriptor: &FORK_RECORD,
            },
        ),
        optional(2, "more", FLAG),
    ],
    oneof: None,
    check: None,
};

/// `NotFoundResp`, which carries nothing.
pub static NOT_FOUND_RESP: MessageDescriptor = MessageDescriptor {
    name: "NotFoundResp",
    max_bytes: 0,
    fields: &[],
    oneof: None,
    check: None,
};

/// `RejectedResp`. `REJECT_CODE_UNSPECIFIED` is not an accepted value, so a
/// peer that forgets to set `code` is rejected rather than read as `MALFORMED`
/// by accident.
pub static REJECTED_RESP: MessageDescriptor = MessageDescriptor {
    name: "RejectedResp",
    max_bytes: SMALL_REQUEST_BYTES + MAX_REJECT_MSG_BYTES,
    fields: &[
        required(
            1,
            "code",
            FieldKind::Enum {
                values: &[
                    EnumValue {
                        number: 1,
                        name: "MALFORMED",
                    },
                    EnumValue {
                        number: 2,
                        name: "TOO_LARGE",
                    },
                    EnumValue {
                        number: 3,
                        name: "INVALID",
                    },
                    EnumValue {
                        number: 4,
                        name: "FORK",
                    },
                    EnumValue {
                        number: 5,
                        name: "UNSUPPORTED",
                    },
                    EnumValue {
                        number: 6,
                        name: "NOT_ADMITTED",
                    },
                    EnumValue {
                        number: 7,
                        name: "BUSY",
                    },
                ],
            },
        ),
        optional(2, "at_seq", COUNT64),
        // `msg` is a proto `string`, so a peer that puts arbitrary bytes there
        // is refused rather than read with replacement characters.
        optional(
            3,
            "msg",
            FieldKind::String {
                max: MAX_REJECT_MSG_BYTES,
            },
        ),
    ],
    oneof: None,
    check: None,
};

/// `Response`, the frame a server sends.
pub static RESPONSE: MessageDescriptor = MessageDescriptor {
    name: "Response",
    max_bytes: MAX_FRAME_BYTES,
    fields: &[
        variant(1, "head", &HEAD_RESP),
        variant(2, "events", &EVENTS_RESP),
        variant(3, "accepted", &ACCEPTED_RESP),
        variant(4, "ledgers", &LEDGERS_RESP),
        variant(5, "forks", &FORKS_RESP),
        variant(6, "not_found", &NOT_FOUND_RESP),
        variant(7, "rejected", &REJECTED_RESP),
    ],
    oneof: Some(Oneof {
        name: "kind",
        first_number: 1,
    }),
    check: None,
};

//! Sync frames on the wire: the canonical encoders, the reader and the
//! mapping from a validator failure to a `RejectCode`.
//!
//! Nothing here re-encodes an event. A `SignedEvent` is copied into a frame
//! and read back out as the same byte string, because those bytes are what
//! was signed (proposal 001 section 3.1). That is why this module writes
//! protobuf records by hand instead of round-tripping through generated
//! structs.
//!
//! The encoders emit the canonical form the field tables in
//! [`crate::descriptors`] accept: ascending field numbers, minimal varints
//! and no proto3 default value written.

use mabel_core::proto::{DeclaredKind, RejectCode};
use mabel_core::validate::{self, WireError};
use mabel_core::{EventId, LedgerId};

use crate::MAX_REJECT_MSG_BYTES;
use crate::descriptors;
use crate::error::{Error, Rejection};
use crate::store::{EventPage, ForkRecord, Head, LedgerSummary, Page, PushOutcome};

/// The `Request.kind` field numbers.
mod req {
    pub const HEAD: u32 = 1;
    pub const GET: u32 = 2;
    pub const PUSH: u32 = 3;
    pub const LIST: u32 = 4;
    pub const FORKS: u32 = 5;
}

/// The `Response.kind` field numbers.
mod resp {
    pub const HEAD: u32 = 1;
    pub const EVENTS: u32 = 2;
    pub const ACCEPTED: u32 = 3;
    pub const LEDGERS: u32 = 4;
    pub const FORKS: u32 = 5;
    pub const NOT_FOUND: u32 = 6;
    pub const REJECTED: u32 = 7;
}

// --- writing ---------------------------------------------------------------

fn put_varint(buf: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

fn varint_len(mut value: u64) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn put_key(buf: &mut Vec<u8>, number: u32, wire: u8) {
    put_varint(buf, (u64::from(number) << 3) | u64::from(wire));
}

/// Writes a varint field, omitting it when it holds the proto3 default.
fn put_uint(buf: &mut Vec<u8>, number: u32, value: u64) {
    if value == 0 {
        return;
    }
    put_key(buf, number, 0);
    put_varint(buf, value);
}

/// Writes a `bool` field, omitting it when false.
fn put_flag(buf: &mut Vec<u8>, number: u32, value: bool) {
    if value {
        put_key(buf, number, 0);
        put_varint(buf, 1);
    }
}

/// Writes a length-delimited field, omitting it when empty.
fn put_bytes(buf: &mut Vec<u8>, number: u32, value: &[u8]) {
    if value.is_empty() {
        return;
    }
    put_message(buf, number, value);
}

/// Writes a length-delimited field even when it is empty, which is what an
/// empty submessage such as `NotFoundResp` needs.
fn put_message(buf: &mut Vec<u8>, number: u32, value: &[u8]) {
    put_key(buf, number, 2);
    put_varint(buf, value.len() as u64);
    buf.extend_from_slice(value);
}

/// How many bytes one entry of a repeated length-delimited field costs.
pub fn entry_len(number: u32, body: &[u8]) -> usize {
    varint_len((u64::from(number) << 3) | 2) + varint_len(body.len() as u64) + body.len()
}

fn envelope(number: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    put_message(&mut out, number, body);
    out
}

/// Clips a reason to [`MAX_REJECT_MSG_BYTES`] on a character boundary.
fn clip(msg: &str) -> &str {
    if msg.len() <= MAX_REJECT_MSG_BYTES {
        return msg;
    }
    let mut end = MAX_REJECT_MSG_BYTES;
    while end > 0 && !msg.is_char_boundary(end) {
        end -= 1;
    }
    &msg[..end]
}

/// Encodes a `Request` carrying a `HeadReq`.
pub fn head_request(ledger: LedgerId) -> Vec<u8> {
    let mut body = Vec::new();
    put_bytes(&mut body, 1, ledger.as_bytes());
    envelope(req::HEAD, &body)
}

/// Encodes a `Request` carrying a `GetReq`.
pub fn get_request(ledger: LedgerId, since: u64, limit: u32) -> Vec<u8> {
    let mut body = Vec::new();
    put_bytes(&mut body, 1, ledger.as_bytes());
    put_uint(&mut body, 2, since);
    put_uint(&mut body, 3, u64::from(limit));
    envelope(req::GET, &body)
}

/// Encodes a `Request` carrying a `PushReq`, copying each event verbatim.
pub fn push_request(ledger: LedgerId, events: &[Vec<u8>]) -> Vec<u8> {
    let mut body = Vec::new();
    put_bytes(&mut body, 1, ledger.as_bytes());
    for event in events {
        put_message(&mut body, 2, event);
    }
    envelope(req::PUSH, &body)
}

/// Encodes a `Request` carrying a `ListReq`.
pub fn list_request(offset: u32, limit: u32) -> Vec<u8> {
    let mut body = Vec::new();
    put_uint(&mut body, 1, u64::from(offset));
    put_uint(&mut body, 2, u64::from(limit));
    envelope(req::LIST, &body)
}

/// Encodes a `Request` carrying a `ForksReq`. `None` asks for every ledger.
pub fn forks_request(ledger: Option<LedgerId>, offset: u32, limit: u32) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(ledger) = ledger {
        put_bytes(&mut body, 1, ledger.as_bytes());
    }
    put_uint(&mut body, 2, u64::from(offset));
    put_uint(&mut body, 3, u64::from(limit));
    envelope(req::FORKS, &body)
}

/// Encodes a `Response` carrying a `HeadResp`.
pub fn head_response(head: &Head) -> Vec<u8> {
    let mut body = Vec::new();
    put_uint(&mut body, 1, head.head_seq);
    put_bytes(&mut body, 2, head.head_event.as_bytes());
    put_uint(&mut body, 3, head.updated_ms);
    envelope(resp::HEAD, &body)
}

/// Encodes a `Response` carrying an `EventsResp`, copying each event
/// verbatim.
pub fn events_response(events: &[&[u8]], head_seq: u64, more: bool) -> Vec<u8> {
    let mut body = Vec::new();
    for event in events {
        put_message(&mut body, 1, event);
    }
    put_uint(&mut body, 2, head_seq);
    put_flag(&mut body, 3, more);
    envelope(resp::EVENTS, &body)
}

/// Encodes a `Response` carrying an `AcceptedResp`.
pub fn accepted_response(outcome: &PushOutcome) -> Vec<u8> {
    let mut body = Vec::new();
    put_uint(&mut body, 1, outcome.head_seq);
    put_uint(&mut body, 2, u64::from(outcome.stored));
    envelope(resp::ACCEPTED, &body)
}

/// Encodes one `LedgerSummary`, the entry body a `LedgersResp` repeats.
pub fn summary_entry(summary: &LedgerSummary) -> Vec<u8> {
    let mut body = Vec::new();
    put_bytes(&mut body, 1, summary.ledger.as_bytes());
    put_uint(&mut body, 2, summary.declared_kind as u64);
    put_uint(&mut body, 3, summary.head_seq);
    put_bytes(&mut body, 4, summary.head_event.as_bytes());
    put_uint(&mut body, 5, summary.event_count);
    put_uint(&mut body, 6, summary.first_seen_ms);
    put_uint(&mut body, 7, summary.updated_ms);
    put_uint(&mut body, 8, u64::from(summary.fork_count));
    put_flag(&mut body, 9, summary.forks_truncated);
    body
}

/// Encodes a `Response` carrying a `LedgersResp` over already encoded
/// entries.
pub fn ledgers_response(entries: &[Vec<u8>], more: bool) -> Vec<u8> {
    let mut body = Vec::new();
    for entry in entries {
        put_message(&mut body, 1, entry);
    }
    put_flag(&mut body, 2, more);
    envelope(resp::LEDGERS, &body)
}

/// Encodes one `ForkRecord`, the entry body a `ForksResp` repeats.
pub fn fork_entry(record: &ForkRecord) -> Vec<u8> {
    let mut body = Vec::new();
    put_bytes(&mut body, 1, record.ledger.as_bytes());
    put_uint(&mut body, 2, record.seq);
    put_message(&mut body, 3, &record.kept);
    put_message(&mut body, 4, &record.conflicting);
    put_uint(&mut body, 5, record.observed_ms);
    if let Some(endpoint) = record.source_endpoint {
        put_bytes(&mut body, 6, endpoint.as_bytes());
    }
    body
}

/// Encodes a `Response` carrying a `ForksResp` over already encoded entries.
pub fn forks_response(entries: &[Vec<u8>], more: bool) -> Vec<u8> {
    let mut body = Vec::new();
    for entry in entries {
        put_message(&mut body, 1, entry);
    }
    put_flag(&mut body, 2, more);
    envelope(resp::FORKS, &body)
}

/// Encodes a `Response` carrying a `NotFoundResp`.
pub fn not_found_response() -> Vec<u8> {
    envelope(resp::NOT_FOUND, &[])
}

/// Encodes a `Response` carrying a `RejectedResp`.
pub fn rejected_response(rejection: &Rejection) -> Vec<u8> {
    let mut body = Vec::new();
    put_uint(&mut body, 1, rejection.code as u64);
    put_uint(&mut body, 2, rejection.at_seq);
    put_bytes(&mut body, 3, clip(&rejection.msg).as_bytes());
    envelope(resp::REJECTED, &body)
}

/// Fills entries into a byte budget, reporting whether it had to stop early.
///
/// The response is filled to `min(count limit, byte budget)` and the caller
/// sets `more` accordingly (proposal 001 section 5).
pub fn fill_budget<T>(
    items: &[T],
    number: u32,
    budget: usize,
    encode: impl Fn(&T) -> Vec<u8>,
) -> (Vec<Vec<u8>>, bool) {
    let mut out = Vec::with_capacity(items.len());
    let mut used = 0usize;
    for item in items {
        let body = encode(item);
        let cost = entry_len(number, &body);
        if used + cost > budget && !out.is_empty() {
            return (out, true);
        }
        used += cost;
        out.push(body);
    }
    (out, false)
}

// --- reading ---------------------------------------------------------------

/// One record of a scanned message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field<'a> {
    /// Wire type 0.
    Varint(u64),
    /// Wire type 2.
    Len(&'a [u8]),
}

fn read_varint(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// Reads the records of a message that has already passed its field table.
///
/// Returns `None` for input the scanner would have rejected, so a caller that
/// validated first can treat `None` as impossible without risking a panic.
pub fn fields(bytes: &[u8]) -> Option<Vec<(u32, Field<'_>)>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let key = read_varint(bytes, &mut pos)?;
        let number = u32::try_from(key >> 3).ok()?;
        match key & 7 {
            0 => out.push((number, Field::Varint(read_varint(bytes, &mut pos)?))),
            2 => {
                let len = usize::try_from(read_varint(bytes, &mut pos)?).ok()?;
                let end = pos.checked_add(len)?;
                if end > bytes.len() {
                    return None;
                }
                out.push((number, Field::Len(&bytes[pos..end])));
                pos = end;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The value of a varint field, 0 when it is absent.
pub fn uint(fields: &[(u32, Field<'_>)], number: u32) -> u64 {
    fields
        .iter()
        .find_map(|(found, value)| match value {
            Field::Varint(value) if *found == number => Some(*value),
            _ => None,
        })
        .unwrap_or(0)
}

/// Whether a `bool` field is set.
pub fn flag(fields: &[(u32, Field<'_>)], number: u32) -> bool {
    uint(fields, number) != 0
}

/// The bytes of a length-delimited field, or `None` if it is absent.
pub fn bytes<'a>(fields: &[(u32, Field<'a>)], number: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|(found, value)| match value {
        Field::Len(bytes) if *found == number => Some(*bytes),
        _ => None,
    })
}

/// Every entry of a repeated length-delimited field, in order.
pub fn repeated<'a>(fields: &[(u32, Field<'a>)], number: u32) -> Vec<&'a [u8]> {
    fields
        .iter()
        .filter_map(|(found, value)| match value {
            Field::Len(bytes) if *found == number => Some(*bytes),
            _ => None,
        })
        .collect()
}

/// The `oneof` variant a `Request` or `Response` carries.
fn variant<'a>(fields: &[(u32, Field<'a>)]) -> Option<(u32, &'a [u8])> {
    fields.iter().find_map(|(number, value)| match value {
        Field::Len(bytes) => Some((*number, *bytes)),
        Field::Varint(_) => None,
    })
}

fn ledger_id(bytes: Option<&[u8]>) -> Option<LedgerId> {
    LedgerId::from_slice(bytes?).ok()
}

fn event_id(bytes: Option<&[u8]>) -> Option<EventId> {
    EventId::from_slice(bytes?).ok()
}

/// A `string` field, or `None` if the bytes are not UTF-8.
///
/// An absent field reads as the empty string. Nothing is decoded lossily: a
/// peer that sends bytes where a string belongs is refused, and the field
/// table refuses it first (`RejectedResp.msg`).
fn text(bytes: Option<&[u8]>) -> Option<String> {
    match bytes {
        None => Some(String::new()),
        Some(bytes) => std::str::from_utf8(bytes).ok().map(str::to_owned),
    }
}

/// What a client asked for.
///
/// `Push` borrows its events from the frame, so the encoded `SignedEvent`
/// bytes reach the store exactly as they arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request<'a> {
    /// `HeadReq`.
    Head {
        /// The ledger asked about.
        ledger: LedgerId,
    },
    /// `GetReq`, with `since` inclusive.
    Get {
        /// The ledger asked about.
        ledger: LedgerId,
        /// The first sequence wanted.
        since: u64,
        /// The requested count, not yet clamped.
        limit: u32,
    },
    /// `PushReq`.
    Push {
        /// The ledger the events belong to.
        ledger: LedgerId,
        /// The encoded `SignedEvent`s, verbatim.
        events: Vec<&'a [u8]>,
    },
    /// `ListReq`.
    List {
        /// How many ledgers to skip.
        offset: u32,
        /// The requested count, not yet clamped.
        limit: u32,
    },
    /// `ForksReq`. `None` asks about every ledger.
    Forks {
        /// The ledger asked about.
        ledger: Option<LedgerId>,
        /// How many records to skip.
        offset: u32,
        /// The requested count, not yet clamped.
        limit: u32,
    },
}

impl Request<'_> {
    /// The name of this request, for logs and error messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Head { .. } => "Head",
            Self::Get { .. } => "Get",
            Self::Push { .. } => "Push",
            Self::List { .. } => "List",
            Self::Forks { .. } => "Forks",
        }
    }
}

/// The `RejectCode` a validator failure on a request frame answers.
///
/// A cap is `TOO_LARGE`, a `Request` variant this version does not know is
/// `UNSUPPORTED`, and everything else is `MALFORMED` (proposal 001
/// section 5).
///
/// Every `Request` field number belongs to the `kind` `oneof`, so the
/// validator reports any unknown number as an unrecognised variant. Noise
/// hits that too, which would answer `UNSUPPORTED` to a random byte string.
/// [`is_unknown_variant`] separates the two: only a frame that is otherwise a
/// well-formed single-variant `Request` is `UNSUPPORTED`.
pub fn reject_code(frame: &[u8], error: &WireError) -> RejectCode {
    match error {
        WireError::MessageTooLarge { .. } | WireError::FieldTooLong { .. } => RejectCode::TooLarge,
        WireError::RepeatedCount { count, max, .. } if count > max => RejectCode::TooLarge,
        WireError::UnknownOneofVariant { message, .. }
            if *message == descriptors::REQUEST.name && is_unknown_variant(frame) =>
        {
            RejectCode::Unsupported
        }
        _ => RejectCode::Malformed,
    }
}

/// Whether a frame is one well-formed length-delimited record whose field
/// number no `Request` variant of this version declares.
///
/// That is exactly the shape a later version's request has, and no shape
/// random bytes reach except by chance.
pub fn is_unknown_variant(frame: &[u8]) -> bool {
    let Some(records) = fields(frame) else {
        return false;
    };
    match records.as_slice() {
        [(number, Field::Len(_))] => !(req::HEAD..=req::FORKS).contains(number),
        _ => false,
    }
}

fn malformed(msg: &str) -> Rejection {
    Rejection::new(RejectCode::Malformed, msg)
}

/// Validates a request frame against the field table and reads it.
///
/// Validation runs before anything is decoded and before any allocation
/// sized by the frame, so a cap violation is answered rather than served.
pub fn parse_request(frame: &[u8]) -> Result<Request<'_>, Rejection> {
    if let Err(error) = validate::message(&descriptors::REQUEST, frame) {
        return Err(Rejection::new(
            reject_code(frame, &error),
            error.to_string(),
        ));
    }
    let outer = fields(frame).ok_or_else(|| malformed("Request is not readable"))?;
    let (number, body) =
        variant(&outer).ok_or_else(|| malformed("Request.kind names no variant"))?;
    let inner = fields(body).ok_or_else(|| malformed("Request.kind is not readable"))?;
    let unreadable = || malformed("Request.kind is missing a required field");
    match number {
        req::HEAD => Ok(Request::Head {
            ledger: ledger_id(bytes(&inner, 1)).ok_or_else(unreadable)?,
        }),
        req::GET => Ok(Request::Get {
            ledger: ledger_id(bytes(&inner, 1)).ok_or_else(unreadable)?,
            since: uint(&inner, 2),
            limit: uint(&inner, 3) as u32,
        }),
        req::PUSH => Ok(Request::Push {
            ledger: ledger_id(bytes(&inner, 1)).ok_or_else(unreadable)?,
            events: repeated(&inner, 2),
        }),
        req::LIST => Ok(Request::List {
            offset: uint(&inner, 1) as u32,
            limit: uint(&inner, 2) as u32,
        }),
        req::FORKS => Ok(Request::Forks {
            ledger: match bytes(&inner, 1) {
                Some(raw) => Some(LedgerId::from_slice(raw).map_err(|_| unreadable())?),
                None => None,
            },
            offset: uint(&inner, 2) as u32,
            limit: uint(&inner, 3) as u32,
        }),
        // The field table already refused every other number.
        _ => Err(Rejection::new(
            RejectCode::Unsupported,
            "unrecognised Request.kind variant",
        )),
    }
}

/// What a server answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// `HeadResp`.
    Head(Head),
    /// `EventsResp`.
    Events(EventPage),
    /// `AcceptedResp`.
    Accepted(PushOutcome),
    /// `LedgersResp`.
    Ledgers(Page<LedgerSummary>),
    /// `ForksResp`.
    Forks(Page<ForkRecord>),
    /// `NotFoundResp`.
    NotFound,
    /// `RejectedResp`.
    Rejected(Rejection),
}

impl Response {
    /// The name of this response, for logs and error messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Head(_) => "Head",
            Self::Events(_) => "Events",
            Self::Accepted(_) => "Accepted",
            Self::Ledgers(_) => "Ledgers",
            Self::Forks(_) => "Forks",
            Self::NotFound => "NotFound",
            Self::Rejected(_) => "Rejected",
        }
    }
}

fn broken(msg: &str) -> Error {
    Error::Protocol(msg.to_string())
}

/// Validates a response frame against the field table and reads it.
pub fn parse_response(frame: &[u8]) -> Result<Response, Error> {
    validate::message(&descriptors::RESPONSE, frame)?;
    let outer = fields(frame).ok_or_else(|| broken("Response is not readable"))?;
    let (number, body) = variant(&outer).ok_or_else(|| broken("Response.kind names no variant"))?;
    let inner = fields(body).ok_or_else(|| broken("Response.kind is not readable"))?;
    let missing = || broken("Response.kind is missing a required field");
    match number {
        resp::HEAD => Ok(Response::Head(Head {
            head_seq: uint(&inner, 1),
            head_event: event_id(bytes(&inner, 2)).ok_or_else(missing)?,
            updated_ms: uint(&inner, 3),
        })),
        resp::EVENTS => Ok(Response::Events(EventPage {
            events: repeated(&inner, 1)
                .into_iter()
                .map(<[u8]>::to_vec)
                .collect(),
            head_seq: uint(&inner, 2),
            more: flag(&inner, 3),
        })),
        resp::ACCEPTED => Ok(Response::Accepted(PushOutcome {
            head_seq: uint(&inner, 1),
            stored: uint(&inner, 2) as u32,
        })),
        resp::LEDGERS => {
            let mut items = Vec::new();
            for entry in repeated(&inner, 1) {
                items.push(read_summary(entry).ok_or_else(missing)?);
            }
            Ok(Response::Ledgers(Page {
                items,
                more: flag(&inner, 2),
            }))
        }
        resp::FORKS => {
            let mut items = Vec::new();
            for entry in repeated(&inner, 1) {
                items.push(read_fork(entry).ok_or_else(missing)?);
            }
            Ok(Response::Forks(Page {
                items,
                more: flag(&inner, 2),
            }))
        }
        resp::NOT_FOUND => Ok(Response::NotFound),
        resp::REJECTED => Ok(Response::Rejected(Rejection {
            code: RejectCode::try_from(uint(&inner, 1) as i32)
                .map_err(|_| broken("RejectedResp.code is not a known code"))?,
            at_seq: uint(&inner, 2),
            msg: text(bytes(&inner, 3))
                .ok_or_else(|| broken("RejectedResp.msg is not valid UTF-8"))?,
        })),
        // The field table already refused every other number.
        _ => Err(broken("unrecognised Response.kind variant")),
    }
}

fn read_summary(entry: &[u8]) -> Option<LedgerSummary> {
    let f = fields(entry)?;
    Some(LedgerSummary {
        ledger: ledger_id(bytes(&f, 1))?,
        declared_kind: DeclaredKind::try_from(uint(&f, 2) as i32).ok()?,
        head_seq: uint(&f, 3),
        head_event: event_id(bytes(&f, 4))?,
        event_count: uint(&f, 5),
        first_seen_ms: uint(&f, 6),
        updated_ms: uint(&f, 7),
        fork_count: uint(&f, 8) as u32,
        forks_truncated: flag(&f, 9),
    })
}

fn read_fork(entry: &[u8]) -> Option<ForkRecord> {
    let f = fields(entry)?;
    let source = match bytes(&f, 6) {
        Some(raw) => Some(iroh_base::EndpointId::from_bytes(&raw.try_into().ok()?).ok()?),
        None => None,
    };
    Some(ForkRecord {
        ledger: ledger_id(bytes(&f, 1))?,
        seq: uint(&f, 2),
        kept: bytes(&f, 3)?.to_vec(),
        conflicting: bytes(&f, 4)?.to_vec(),
        observed_ms: uint(&f, 5),
        source_endpoint: source,
    })
}

/// The encoded `EventBody` of an encoded `SignedEvent`.
pub fn signed_event_body(event: &[u8]) -> Option<&[u8]> {
    bytes(&fields(event)?, 1)
}

/// The `event_id` of an encoded `SignedEvent`.
pub fn signed_event_id(event: &[u8]) -> Option<EventId> {
    Some(mabel_core::event_id(signed_event_body(event)?))
}

/// The `seq` of an encoded `SignedEvent`, 0 for an inception.
pub fn signed_event_seq(event: &[u8]) -> Option<u64> {
    Some(uint(&fields(signed_event_body(event)?)?, 3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_events;
    use crate::{MAX_EVENT_BYTES, MAX_PUSH_EVENTS};
    use mabel_core::IdentityId;

    fn ledger() -> LedgerId {
        IdentityId::from_bytes([7u8; 32])
    }

    #[test]
    fn every_request_round_trips() {
        let events = sample_events(2);
        let frames = [
            head_request(ledger()),
            get_request(ledger(), 3, 9),
            get_request(ledger(), 0, 0),
            push_request(ledger(), &events),
            list_request(0, 10),
            list_request(4, 0),
            forks_request(Some(ledger()), 1, 2),
            forks_request(None, 0, 0),
        ];
        for frame in frames {
            let parsed = parse_request(&frame).expect("the frame parses");
            if let Request::Push { events: pushed, .. } = &parsed {
                let pushed: Vec<Vec<u8>> = pushed.iter().map(|e| e.to_vec()).collect();
                assert_eq!(pushed, events, "pushed events keep their bytes");
            }
        }
    }

    /// The scanner enforces the repeated cap on the entry that passes it, so
    /// the 513th event is refused for the count and is never validated: an
    /// entry that would fail the per-event cap on its own still comes back as
    /// `repeated_count`.
    #[test]
    fn a_push_over_the_event_cap_is_refused_before_the_extra_event_is_read() {
        let valid = sample_events(1).remove(0);
        let mut events = vec![valid; MAX_PUSH_EVENTS];
        events.push(vec![0u8; MAX_EVENT_BYTES + 1]);
        let frame = push_request(ledger(), &events);

        let rejection = parse_request(&frame).expect_err("513 events pass the cap");
        assert_eq!(rejection.code, RejectCode::TooLarge);
        assert!(
            rejection.msg.contains("PushReq.events holds 513 entries"),
            "the count is what fired, not the oversize entry: {}",
            rejection.msg
        );

        // The same body through the validator, where the class is visible.
        let outer = fields(&frame).expect("the frame is readable");
        let (_, body) = variant(&outer).expect("the frame carries a PushReq");
        let error = validate::message(&descriptors::PUSH_REQ, body).expect_err("513 events");
        assert_eq!(error.code(), "repeated_count");
    }

    #[test]
    fn every_response_round_trips() {
        let events = sample_events(2);
        let borrowed: Vec<&[u8]> = events.iter().map(Vec::as_slice).collect();
        let head = Head {
            head_seq: 4,
            head_event: signed_event_id(&events[0]).unwrap(),
            updated_ms: 1_700_000_000_000,
        };
        let summary = LedgerSummary {
            ledger: ledger(),
            declared_kind: DeclaredKind::Person,
            head_seq: 1,
            head_event: head.head_event,
            event_count: 2,
            first_seen_ms: 1_700_000_000_000,
            updated_ms: 1_700_000_000_001,
            fork_count: 0,
            forks_truncated: false,
        };
        let fork = ForkRecord {
            ledger: ledger(),
            seq: 1,
            kept: events[0].clone(),
            conflicting: events[1].clone(),
            observed_ms: 1_700_000_000_002,
            source_endpoint: Some(iroh_base::SecretKey::from_bytes(&[9u8; 32]).public()),
        };

        assert_eq!(
            parse_response(&head_response(&head)).unwrap(),
            Response::Head(head)
        );
        assert_eq!(
            parse_response(&events_response(&borrowed, 7, true)).unwrap(),
            Response::Events(EventPage {
                events: events.clone(),
                head_seq: 7,
                more: true,
            })
        );
        assert_eq!(
            parse_response(&accepted_response(&PushOutcome {
                head_seq: 2,
                stored: 1
            }))
            .unwrap(),
            Response::Accepted(PushOutcome {
                head_seq: 2,
                stored: 1
            })
        );
        assert_eq!(
            parse_response(&ledgers_response(&[summary_entry(&summary)], false)).unwrap(),
            Response::Ledgers(Page {
                items: vec![summary],
                more: false,
            })
        );
        assert_eq!(
            parse_response(&forks_response(&[fork_entry(&fork)], true)).unwrap(),
            Response::Forks(Page {
                items: vec![fork],
                more: true,
            })
        );
        assert_eq!(
            parse_response(&not_found_response()).unwrap(),
            Response::NotFound
        );
    }

    #[test]
    fn every_reject_code_round_trips() {
        for code in [
            RejectCode::Malformed,
            RejectCode::TooLarge,
            RejectCode::Invalid,
            RejectCode::Fork,
            RejectCode::Unsupported,
            RejectCode::NotAdmitted,
            RejectCode::Busy,
        ] {
            let rejection = Rejection::at(code, 12, "why");
            let frame = rejected_response(&rejection);
            assert_eq!(
                parse_response(&frame).unwrap(),
                Response::Rejected(rejection),
                "{} does not round-trip",
                code.as_str_name()
            );
        }
    }

    #[test]
    fn an_unspecified_reject_code_is_refused() {
        let frame = rejected_response(&Rejection::new(RejectCode::Unspecified, "oops"));
        let error = parse_response(&frame).expect_err("REJECT_CODE_UNSPECIFIED is not a code");
        assert!(matches!(error, Error::Malformed(_)), "{error}");
    }

    /// `RejectedResp.msg` is a proto `string`, so a peer cannot smuggle
    /// arbitrary bytes into a message a client logs or prints.
    #[test]
    fn a_reject_reason_that_is_not_utf8_is_refused() {
        let mut body = Vec::new();
        put_uint(&mut body, 1, u64::from(RejectCode::Busy as u32));
        put_uint(&mut body, 2, 3);
        // A lone continuation byte is not UTF-8 under any decoding.
        put_bytes(&mut body, 3, &[0x61, 0xff, 0x9f, 0x62]);
        let frame = envelope(resp::REJECTED, &body);

        let error = parse_response(&frame).expect_err("msg is not UTF-8");
        let Error::Malformed(wire) = &error else {
            panic!("expected a field-table rejection, got {error}");
        };
        assert_eq!(wire.code(), "invalid_utf8");
        assert!(error.to_string().contains("RejectedResp.msg"), "{error}");
    }

    #[test]
    fn a_long_reject_reason_is_clipped() {
        let rejection = Rejection::new(RejectCode::Busy, "x".repeat(MAX_REJECT_MSG_BYTES + 50));
        let frame = rejected_response(&rejection);
        let Response::Rejected(parsed) = parse_response(&frame).unwrap() else {
            panic!("expected a rejection");
        };
        assert_eq!(parsed.msg.len(), MAX_REJECT_MSG_BYTES);
    }

    #[test]
    fn garbage_is_malformed_and_an_unknown_variant_is_unsupported() {
        for garbage in [
            vec![0xff, 0xff, 0xff, 0x7f],
            vec![0x08, 0x01],
            vec![0x0a, 0xff],
            b"hello".to_vec(),
        ] {
            let error = parse_request(&garbage).expect_err("garbage is refused");
            assert_eq!(error.code, RejectCode::Malformed, "{garbage:?}");
        }

        let mut frame = Vec::new();
        put_message(&mut frame, 6, &[]);
        let unknown = parse_request(&frame).expect_err("variant 6 is refused");
        assert_eq!(unknown.code, RejectCode::Unsupported);
    }

    #[test]
    fn a_truncated_frame_is_malformed() {
        let full = head_request(ledger());
        let truncated = &full[..full.len() - 4];
        assert_eq!(
            parse_request(truncated)
                .expect_err("truncation is refused")
                .code,
            RejectCode::Malformed
        );
    }

    #[test]
    fn the_budget_stops_before_it_overflows() {
        let events = sample_events(4);
        let two = entry_len(1, &events[0]) + entry_len(1, &events[1]);
        let (filled, more) = fill_budget(&events, 1, two, |event| event.clone());
        assert_eq!(filled.len(), 2);
        assert!(more);

        let (filled, more) = fill_budget(&events, 1, usize::MAX, |event| event.clone());
        assert_eq!(filled.len(), 4);
        assert!(!more);
    }

    #[test]
    fn one_entry_always_fits_even_over_budget() {
        let events = sample_events(2);
        let (filled, more) = fill_budget(&events, 1, 1, |event| event.clone());
        assert_eq!(filled.len(), 1, "a response never comes back empty");
        assert!(more);
    }

    #[test]
    fn event_helpers_read_the_encoded_event() {
        let events = sample_events(3);
        assert_eq!(signed_event_seq(&events[0]), Some(0));
        assert_eq!(signed_event_seq(&events[2]), Some(2));
        assert!(signed_event_id(&events[0]).is_some());
        assert_eq!(signed_event_seq(b"not an event"), None);
    }

    /// Regression: an identity-rooted inception reaches nesting depth 7 by
    /// itself, so counting a pushed event's depth from the frame root made
    /// every `identity create --founder` ledger unpushable. Events are
    /// detached objects with their own budget.
    #[test]
    fn an_identity_rooted_inception_survives_frame_validation() {
        use mabel_core::proto::DeclaredKind;
        use mabel_core::sign::{Root, build_inception};

        let founder_signer = iroh_base::SecretKey::from_bytes(&[9u8; 32]);
        let reserve = iroh_base::SecretKey::from_bytes(&[137u8; 32]);
        let founder = build_inception(
            &founder_signer,
            DeclaredKind::Person,
            Root::Raw {
                reserve_key: &reserve.public(),
            },
            [9u8; 16],
            1_700_000_000_000,
        )
        .expect("the founder inception builds");
        let team = build_inception(
            &founder_signer,
            DeclaredKind::Organization,
            Root::Identity {
                founder: founder.event_id.into(),
                founder_inception: &founder.signed_event,
            },
            [10u8; 16],
            1_700_000_000_000,
        )
        .expect("the identity-rooted inception builds");

        let ledger: LedgerId = team.event_id.into();
        let frame = push_request(ledger, std::slice::from_ref(&team.signed_event));
        let parsed = parse_request(&frame).expect("the push frame validates");
        match parsed {
            Request::Push {
                ledger: got,
                events,
            } => {
                assert_eq!(got, ledger);
                assert_eq!(events.len(), 1);
                assert_eq!(events[0], team.signed_event.as_slice());
            }
            other => panic!("expected a push request, got {other:?}"),
        }
    }
}

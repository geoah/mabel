//! Rendering stored event bytes as the event document of
//! `contracts/README.md`.
//!
//! The node decodes and the UI holds no keys, so no raw event bytes cross the
//! HTTP surface (proposal 001 section 10). Every byte field renders as
//! lowercase RFC 4648 base32 without padding, the one spelling every document
//! uses.
//!
//! # Not frozen
//!
//! `payload_kind` is the `oneof` tag name from `ledger.proto` in snake_case, so
//! an inception is `inception`. The fixture
//! `contracts/http/witness-get-ledger-events.json` still spells it
//! `person_inception` with the root's fields flat beside `declared_kind`, from
//! before proposal 002 replaced the two inception messages with one carrying a
//! `root` oneof; `contracts/README.md` lists the inception and membership
//! payload names as not frozen, so this module follows `ledger.proto` and the
//! fixture is the one that has to move.
//!
//! The blob fields an event embeds verbatim, `founder_inception`,
//! `invitee_inception`, `acceptance` and its `signature`, render as base32 of
//! those bytes rather than as decoded messages: they are signed objects, and a
//! reader that wants their contents asks for the ledger they came from.

use data_encoding::BASE32_NOPAD;
use mabel_proto::prost::Message;
use mabel_proto::v0::{
    DeclaredKind as ProtoKind, EventBody, Role, SignedEvent, event_body, inception,
};
use serde_json::{Value, json};

use crate::api::documents::{DeclaredKind, Event, Id};
use crate::api::error::ServiceError;

/// Renders 32 bytes as a document id.
///
/// # Panics
///
/// Panics if `bytes` is not 32 bytes, which the field table already refused.
#[must_use]
pub(crate) fn id_of(bytes: &[u8; 32]) -> Id {
    Id::parse(&BASE32_NOPAD.encode(bytes).to_ascii_lowercase())
        .expect("32 bytes encode as 52 base32 characters")
}

/// Renders a byte field of any length as base32, for the blobs an event
/// embeds.
fn base32(bytes: &[u8]) -> String {
    BASE32_NOPAD.encode(bytes).to_ascii_lowercase()
}

/// Renders a 32-byte field of a validated event.
fn id_field(bytes: &[u8]) -> Option<Id> {
    let bytes: [u8; 32] = bytes.try_into().ok()?;
    Some(id_of(&bytes))
}

/// Reads a stored `SignedEvent` back as an event document.
///
/// # Errors
///
/// Returns code 10 with reason `malformed_event` when the stored bytes do not
/// decode, which means the file was corrupted after it was verified.
pub(crate) fn event_document(bytes: &[u8]) -> Result<Event, ServiceError> {
    let signed = SignedEvent::decode(bytes).map_err(|error| malformed(&error.to_string()))?;
    let body =
        EventBody::decode(&signed.body[..]).map_err(|error| malformed(&error.to_string()))?;
    let payload = body
        .payload
        .as_ref()
        .ok_or_else(|| malformed("the event carries no payload"))?;
    Ok(Event {
        event_id: id_of(mabel_core::event_id(&signed.body).as_bytes()),
        seq: body.seq,
        ledger_id: id_field(&body.ledger),
        prev: id_field(&body.prev),
        timestamp_ms: body.timestamp_ms,
        author_key: id_field(&body.author_key)
            .ok_or_else(|| malformed("author_key is not 32 bytes"))?,
        payload_kind: payload_kind(payload).to_owned(),
        payload: payload_document(payload),
    })
}

/// The `oneof` tag name, in snake_case.
fn payload_kind(payload: &event_body::Payload) -> &'static str {
    match payload {
        event_body::Payload::Inception(_) => "inception",
        event_body::Payload::WitnessConfig(_) => "witness_config",
        event_body::Payload::TrustAttestation(_) => "trust_attestation",
        event_body::Payload::TrustRevocation(_) => "trust_revocation",
        event_body::Payload::MembershipInvitation(_) => "membership_invitation",
        event_body::Payload::MembershipAcceptance(_) => "membership_acceptance",
        event_body::Payload::MembershipRemoval(_) => "membership_removal",
        event_body::Payload::ProfileUpdate(_) => "profile_update",
    }
}

/// That variant's fields, under the names `ledger.proto` gives them.
fn payload_document(payload: &event_body::Payload) -> Value {
    match payload {
        event_body::Payload::Inception(inception) => json!({
            "declared_kind": declared_kind_name(inception.kind),
            "nonce": base32(&inception.nonce),
            "root": root_document(inception.root.as_ref()),
        }),
        event_body::Payload::WitnessConfig(config) => json!({
            "witnesses": config
                .witnesses
                .iter()
                .map(|witness| optional(id_field(witness)))
                .collect::<Vec<Value>>(),
        }),
        event_body::Payload::TrustAttestation(attestation) => json!({
            "subject": optional(id_field(&attestation.subject)),
        }),
        event_body::Payload::TrustRevocation(revocation) => json!({
            "target": optional(id_field(&revocation.target)),
        }),
        event_body::Payload::MembershipInvitation(invitation) => json!({
            "invitee": optional(id_field(&invitation.invitee)),
            "invitee_key": optional(id_field(&invitation.invitee_key)),
            "role": role_name(invitation.role),
            "invitee_inception": base32(&invitation.invitee_inception),
        }),
        event_body::Payload::MembershipAcceptance(acceptance) => json!({
            "acceptance": base32(&acceptance.acceptance),
            "signature": base32(&acceptance.signature),
        }),
        event_body::Payload::MembershipRemoval(removal) => json!({
            "target": optional(id_field(&removal.target)),
        }),
        // An absent field means unset, and the encoding cannot tell an absent
        // string from an empty one, so both render as `null` (proposal 003
        // section 1).
        event_body::Payload::ProfileUpdate(profile) => json!({
            "display_name": text(&profile.display_name),
            "hostname": text(&profile.hostname),
            "email": text(&profile.email),
        }),
    }
}

/// A string field renders as `null` when it is unset, never as `""`.
fn text(value: &str) -> Value {
    if value.is_empty() {
        return Value::Null;
    }
    Value::String(value.to_owned())
}

/// The root the inception fixed (proposal 002 section 2).
fn root_document(root: Option<&inception::Root>) -> Value {
    match root {
        Some(inception::Root::RawRoot(raw)) => json!({"raw_root": {
            "active_key": optional(id_field(&raw.active_key)),
            "reserve_commit": optional(id_field(&raw.reserve_commit)),
        }}),
        Some(inception::Root::IdentityRoot(identity)) => json!({"identity_root": {
            "founder": optional(id_field(&identity.founder)),
            "founder_key": optional(id_field(&identity.founder_key)),
            "founder_inception": base32(&identity.founder_inception),
        }}),
        None => Value::Null,
    }
}

/// The advisory declared kind, as every document spells it.
pub(crate) fn declared_kind(kind: ProtoKind) -> DeclaredKind {
    match kind {
        ProtoKind::Organization => DeclaredKind::Organization,
        ProtoKind::Agent => DeclaredKind::Agent,
        ProtoKind::Service => DeclaredKind::Service,
        // A ledger whose inception the field table accepted names a defined
        // kind; `person` is the value the fixtures use for anything else.
        ProtoKind::Person | ProtoKind::KindUnspecified => DeclaredKind::Person,
    }
}

fn declared_kind_name(kind: i32) -> String {
    ProtoKind::try_from(kind)
        .map(|kind| declared_kind(kind).as_str().to_owned())
        .unwrap_or_else(|_| "person".to_owned())
}

fn role_name(role: i32) -> &'static str {
    match Role::try_from(role) {
        Ok(Role::Controller) => "controller",
        Ok(Role::Member) => "member",
        _ => "unspecified",
    }
}

/// An id field renders as `null` rather than being dropped when it is not 32
/// bytes (`contracts/README.md`, "Nullability").
fn optional(id: Option<Id>) -> Value {
    id.map_or(Value::Null, |id| Value::String(id.to_string()))
}

fn malformed(message: &str) -> ServiceError {
    ServiceError::schema(
        "malformed_event",
        format!("a stored event does not decode: {message}"),
    )
}

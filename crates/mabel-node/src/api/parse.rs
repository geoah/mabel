//! Request validation, and the errors the fixtures pin for it.
//!
//! Every message and every `details.reason` here is copied from the `errors`
//! arrays of `contracts/http/*.json`. Ids arrive as strings and leave as
//! [`Id`], so no handler passes an unvalidated id to a service.

use std::collections::HashMap;

use axum::http::StatusCode;
use data_encoding::BASE64;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::documents::{DeclaredKind, ID_LENGTH, Id, RoleName};
use super::error::ServiceError;
use super::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, FetchIdentity,
    ForkQuery, Invite, LookupRequest, PageRequest, PushRequest, RemoveMembership, ReplaceProfile,
    ResolveInput, SetContact,
};
use crate::contacts::{ContactTextError, MAX_NICKNAME_BYTES, MAX_NOTE_BYTES, normalize};
use crate::verification::check_hostname;
use mabel_core::{MAX_ENDPOINTS, MAX_WITNESSES, MabelLink};

/// The query string, one value per name.
pub(super) type Query = HashMap<String, String>;

/// Largest `limit` on the event routes.
pub(super) const MAX_EVENT_LIMIT: u32 = 512;

/// Largest `limit` on `GET /api/ledgers`.
pub(super) const MAX_LEDGER_LIMIT: u32 = 256;

/// Largest `limit` on `GET /api/forks`.
pub(super) const MAX_FORK_LIMIT: u32 = 64;

/// Which id a malformed value was meant to be, which fixes the `reason`.
#[derive(Debug, Clone, Copy)]
pub(super) enum IdKind {
    Identity,
    Ledger,
    Event,
    Endpoint,
}

impl IdKind {
    const fn reason(self) -> &'static str {
        match self {
            Self::Identity => "malformed_identity_id",
            Self::Ledger => "malformed_ledger_id",
            Self::Event => "malformed_event_id",
            Self::Endpoint => "malformed_endpoint_id",
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::Identity => "identity id",
            Self::Ledger => "ledger id",
            Self::Event => "event id",
            Self::Endpoint => "endpoint id",
        }
    }
}

/// Parses a path segment or a body field as an id.
pub(super) fn id(kind: IdKind, raw: &str) -> Result<Id, ServiceError> {
    Id::parse(raw).ok_or_else(|| {
        ServiceError::schema(
            kind.reason(),
            format!("{} must be {ID_LENGTH} base32 characters", kind.noun()),
        )
        .with_detail("value", raw)
    })
}

/// Parses a body field as an id, naming the field in `details`.
fn id_field(kind: IdKind, field: &str, raw: &str) -> Result<Id, ServiceError> {
    id(kind, raw).map_err(|error| error.with_detail("field", field))
}

/// Code 2 for a body field that is absent, null or empty.
fn missing_field(field: &str) -> ServiceError {
    ServiceError::usage("missing_field", format!("{field} is required")).with_detail("field", field)
}

fn required<'a>(field: &str, value: Option<&'a String>) -> Result<&'a str, ServiceError> {
    match value.map(|value| value.trim()) {
        Some(trimmed) if !trimmed.is_empty() => Ok(trimmed),
        _ => Err(missing_field(field)),
    }
}

fn malformed_query(parameter: &str, value: &str, expectation: &str) -> ServiceError {
    ServiceError::usage(
        "malformed_query_parameter",
        format!("{parameter} must be {expectation}"),
    )
    .with_detail("parameter", parameter)
    .with_detail("value", value)
}

/// Rejects a query parameter this route does not read.
pub(super) fn only(query: &Query, allowed: &[&str]) -> Result<(), ServiceError> {
    for name in query.keys() {
        if !allowed.contains(&name.as_str()) {
            return Err(ServiceError::usage(
                "unknown_query_parameter",
                format!("{name} is not a parameter of this route"),
            )
            .with_detail("parameter", name));
        }
    }
    Ok(())
}

/// A parameter that was sent with a non-empty value.
fn present<'a>(query: &'a Query, name: &str) -> Option<&'a str> {
    query
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

/// `?since=`, inclusive, defaulting to 0.
fn since(query: &Query) -> Result<u64, ServiceError> {
    match present(query, "since") {
        None => Ok(0),
        Some(raw) => raw
            .parse()
            .map_err(|_| malformed_query("since", raw, "a non-negative integer")),
    }
}

/// `?offset=`, defaulting to 0.
fn offset(query: &Query) -> Result<u32, ServiceError> {
    match present(query, "offset") {
        None => Ok(0),
        Some(raw) => raw
            .parse()
            .map_err(|_| malformed_query("offset", raw, "a non-negative integer")),
    }
}

/// `?limit=`, defaulting to `max` and clamped to it.
fn limit(query: &Query, max: u32) -> Result<u32, ServiceError> {
    let Some(raw) = present(query, "limit") else {
        return Ok(max);
    };
    let expectation = format!("a positive integer, clamped to {max}");
    let parsed: u32 = raw
        .parse()
        .map_err(|_| malformed_query("limit", raw, &expectation))?;
    if parsed == 0 {
        return Err(malformed_query("limit", raw, &expectation));
    }
    Ok(parsed.min(max))
}

/// `?since=` and `?limit=` for the two event routes.
pub(super) fn event_page(query: &Query) -> Result<EventPageRequest, ServiceError> {
    only(query, &["since", "limit"])?;
    Ok(EventPageRequest {
        since: since(query)?,
        limit: limit(query, MAX_EVENT_LIMIT)?,
    })
}

/// `?offset=` and `?limit=` for `GET /api/ledgers` and for the witness ledger
/// proxy of proposal 004, which pages the same way and clamps to the same
/// maximum.
pub(super) fn ledger_page(query: &Query) -> Result<PageRequest, ServiceError> {
    only(query, &["offset", "limit"])?;
    Ok(PageRequest {
        offset: offset(query)?,
        limit: limit(query, MAX_LEDGER_LIMIT)?,
    })
}

/// `?ledger_id=`, `?offset=` and `?limit=` for `GET /api/forks`.
pub(super) fn fork_query(query: &Query) -> Result<ForkQuery, ServiceError> {
    only(query, &["ledger_id", "offset", "limit"])?;
    let ledger_id = match present(query, "ledger_id") {
        None => None,
        Some(raw) => Some(id(IdKind::Ledger, raw)?),
    };
    Ok(ForkQuery {
        ledger_id,
        page: PageRequest {
            offset: offset(query)?,
            limit: limit(query, MAX_FORK_LIMIT)?,
        },
    })
}

/// Parses a request body, which the loopback rules already forced to be
/// `application/json`.
fn body<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ServiceError> {
    serde_json::from_slice(bytes).map_err(|error| {
        ServiceError::schema("malformed_json", "request body is not valid JSON")
            .with_detail("error", error.to_string())
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateIdentityBody {
    alias: Option<String>,
    declared_kind: Option<String>,
    founder: Option<String>,
    display_name: Option<String>,
    email: Option<String>,
}

/// `POST /api/identities`.
///
/// An absent `declared_kind` means `person`. An absent `founder` means a raw
/// root: the new ledger keys itself (proposal 002 section 2). `display_name`
/// and `email` are optional, and either one makes the node append one
/// `ProfileUpdate` at seq 1 (proposal 005): unlike the profile route, this one
/// takes no whole document, so an absent key publishes nothing rather than
/// clearing something.
pub(super) fn create_identity(bytes: &[u8]) -> Result<CreateIdentity, ServiceError> {
    let parsed: CreateIdentityBody = body(bytes)?;
    let alias = required("alias", parsed.alias.as_ref())?.to_owned();
    let declared_kind = match parsed.declared_kind.as_deref() {
        None | Some("") => DeclaredKind::Person,
        Some(raw) => declared_kind(raw)?,
    };
    let founder = match parsed.founder.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(id_field(IdKind::Identity, "founder", raw)?),
    };
    Ok(CreateIdentity {
        alias,
        declared_kind,
        founder,
        display_name: given(parsed.display_name),
        email: given(parsed.email),
    })
}

/// An optional string field: absent, `null` and empty all mean "not given".
fn given(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn declared_kind(raw: &str) -> Result<DeclaredKind, ServiceError> {
    let Some(kind) = DeclaredKind::parse(raw) else {
        let names = DeclaredKind::ALL.map(DeclaredKind::as_str).join(", ");
        return Err(ServiceError::schema(
            "unknown_enum_value",
            format!("declared_kind must be one of {names}"),
        )
        .with_detail("field", "declared_kind")
        .with_detail("value", raw));
    };
    if kind.is_implemented() {
        Ok(kind)
    } else {
        Err(ServiceError::unsupported(
            "unsupported_declared_kind",
            format!("declared_kind {kind} is not implemented"),
        )
        .with_detail("field", "declared_kind")
        .with_detail("value", raw))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessesBody {
    witnesses: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointsBody {
    endpoints: Option<Vec<String>>,
}

/// `POST /api/identities/{identity_id}/witnesses`.
///
/// The list names 0 to 16 distinct identity ids: a witness is an identity, and
/// an empty list says nobody keeps this chain (proposal 006 section 1). The key
/// must be sent, because the operation replaces the whole set.
pub(super) fn witnesses(bytes: &[u8]) -> Result<Vec<Id>, ServiceError> {
    const RANGE_MESSAGE: &str = "witnesses must hold 0 to 16 distinct identity ids";

    let parsed: WitnessesBody = body(bytes)?;
    let raw = parsed.witnesses.ok_or_else(|| missing_field("witnesses"))?;
    entries(
        &raw,
        IdKind::Identity,
        "witnesses",
        MAX_WITNESSES,
        RANGE_MESSAGE,
        "witnesses_out_of_range",
        "duplicate_witness",
    )
}

/// `POST /api/identities/{identity_id}/endpoints`.
///
/// The list names 0 to 8 distinct endpoint ids, and an empty list says nothing
/// answers for this identity right now (proposal 006 section 2). The key must be
/// sent, because the operation replaces the whole list.
pub(super) fn endpoints(bytes: &[u8]) -> Result<Vec<Id>, ServiceError> {
    const RANGE_MESSAGE: &str = "endpoints must hold 0 to 8 distinct endpoint ids";

    let parsed: EndpointsBody = body(bytes)?;
    let raw = parsed.endpoints.ok_or_else(|| missing_field("endpoints"))?;
    entries(
        &raw,
        IdKind::Endpoint,
        "endpoints",
        MAX_ENDPOINTS,
        RANGE_MESSAGE,
        "endpoints_out_of_range",
        "duplicate_endpoint",
    )
}

/// One whole-replacement list body: at most `max` ids of one kind, no repeat.
#[allow(clippy::too_many_arguments)]
fn entries(
    raw: &[String],
    kind: IdKind,
    field: &'static str,
    max: usize,
    range_message: &'static str,
    out_of_range: &'static str,
    duplicate: &'static str,
) -> Result<Vec<Id>, ServiceError> {
    if raw.len() > max {
        return Err(ServiceError::schema(out_of_range, range_message)
            .with_detail("field", field)
            .with_detail("count", raw.len()));
    }
    let mut parsed = Vec::with_capacity(raw.len());
    for value in raw {
        let id = id_field(kind, field, value)?;
        if parsed.contains(&id) {
            return Err(ServiceError::schema(duplicate, range_message)
                .with_detail("field", field)
                .with_detail("value", id.as_str()));
        }
        parsed.push(id);
    }
    Ok(parsed)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileBody {
    /// Present and `null` clears the field; absent is refused. The outer
    /// `Option` is "was the key sent", the inner one is its value.
    #[serde(default, deserialize_with = "sent")]
    display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "sent")]
    hostname: Option<Option<String>>,
    #[serde(default, deserialize_with = "sent")]
    email: Option<Option<String>>,
}

/// Reads a key that may be `null`, keeping "sent as null" apart from "not
/// sent".
///
/// `Option<Option<T>>` alone cannot tell them apart: serde folds an explicit
/// `null` into the outer `None`, and this route has to refuse the key that
/// was never sent while accepting the one that was sent empty.
fn sent<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

/// `POST /api/identities/{identity_id}/profile`.
///
/// All three keys are required and any may be `null`: the operation is
/// replacement, and a body that names one field would silently clear the
/// others (proposal 003 section 1, proposal 005).
pub(super) fn replace_profile(
    identity_id: Id,
    bytes: &[u8],
) -> Result<ReplaceProfile, ServiceError> {
    let parsed: ProfileBody = body(bytes)?;
    let field = |field: &str, value: Option<Option<String>>| match value {
        None => Err(missing_field(field)),
        Some(value) => Ok(given(value)),
    };
    Ok(ReplaceProfile {
        display_name: field("display_name", parsed.display_name)?,
        hostname: field("hostname", parsed.hostname)?,
        email: field("email", parsed.email)?,
        identity_id,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContactBody {
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// `PUT /api/identities/{identity_id}/contact`.
///
/// The note is replaced whole: an absent or `null` field clears it, and both
/// absent removes the file.
pub(super) fn set_contact(identity_id: Id, bytes: &[u8]) -> Result<SetContact, ServiceError> {
    let parsed: ContactBody = body(bytes)?;
    Ok(SetContact {
        nickname: contact_field("nickname", parsed.nickname.as_deref(), MAX_NICKNAME_BYTES)?,
        note: contact_field("note", parsed.note.as_deref(), MAX_NOTE_BYTES)?,
        identity_id,
    })
}

/// One contact field against the caps and the codepoint policy of proposal
/// 003 section 1.
fn contact_field(
    field: &'static str,
    value: Option<&str>,
    cap: usize,
) -> Result<Option<String>, ServiceError> {
    normalize(field, value, cap).map_err(|error| match error {
        ContactTextError::TooLong { len, cap, .. } => {
            ServiceError::schema("contact_field_too_long", error.to_string())
                .with_detail("field", field)
                .with_detail("len", len)
                .with_detail("cap", cap)
        }
        ContactTextError::Invalid { detail, .. } => {
            ServiceError::schema("invalid_contact_text", error.to_string())
                .with_detail("field", field)
                .with_detail("detail", detail)
        }
    })
}

/// `?from=` on `GET /api/lookup/{identity_id}`.
pub(super) fn lookup(identity_id: Id, query: &Query) -> Result<LookupRequest, ServiceError> {
    only(query, &["from"])?;
    let from = match present(query, "from") {
        None => None,
        Some(raw) => Some(id(IdKind::Identity, raw).map_err(|error| {
            error
                .with_detail("parameter", "from")
                .with_detail("field", "from")
        })?),
    };
    Ok(LookupRequest { identity_id, from })
}

/// Reads a `*_base64` artifact field (`contracts/README.md`, "Artifacts over
/// JSON").
///
/// The cap is checked on the encoded length first, so an oversize body is
/// refused before anything allocates in proportion to it (pitfall 7).
fn artifact(
    field: &str,
    name: &str,
    cap: usize,
    value: Option<&String>,
) -> Result<Vec<u8>, ServiceError> {
    let raw = required(field, value)?;
    let too_large = |len: usize| {
        ServiceError::schema(
            "message_too_large",
            format!("{name} is {len} bytes, over the {cap}-byte cap"),
        )
        .with_detail("field", field)
        .with_detail("artifact", name)
        .with_detail("cap", cap)
    };
    // Four characters carry three bytes.
    if raw.len() > 4 * cap.div_ceil(3) {
        return Err(too_large(raw.len() / 4 * 3));
    }
    let decoded = BASE64.decode(raw.as_bytes()).map_err(|_| {
        ServiceError::schema("malformed_base64", format!("{field} is not base64"))
            .with_detail("field", field)
    })?;
    if decoded.len() > cap {
        return Err(too_large(decoded.len()));
    }
    Ok(decoded)
}

fn role(raw: &str) -> Result<RoleName, ServiceError> {
    RoleName::parse(raw).ok_or_else(|| {
        let names = RoleName::ALL.map(RoleName::as_str).join(", ");
        ServiceError::schema("unknown_enum_value", format!("role must be one of {names}"))
            .with_detail("field", "role")
            .with_detail("value", raw)
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InviteBody {
    by: Option<String>,
    role: Option<String>,
    invitee_descriptor_base64: Option<String>,
}

/// `POST /api/identities/{identity_id}/memberships/invitations`.
pub(super) fn invite(ledger_id: Id, bytes: &[u8]) -> Result<Invite, ServiceError> {
    let parsed: InviteBody = body(bytes)?;
    Ok(Invite {
        ledger_id,
        by: id_field(IdKind::Identity, "by", required("by", parsed.by.as_ref())?)?,
        role: role(required("role", parsed.role.as_ref())?)?,
        invitee_descriptor: artifact(
            "invitee_descriptor_base64",
            "IdentityDescriptor",
            mabel_core::MAX_IDENTITY_DESCRIPTOR_BYTES,
            parsed.invitee_descriptor_base64.as_ref(),
        )?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptBody {
    invitation_bundle_base64: Option<String>,
}

/// `POST /api/identities/{identity_id}/memberships/acceptances`.
pub(super) fn accept_invitation(
    identity_id: Id,
    bytes: &[u8],
) -> Result<AcceptInvitation, ServiceError> {
    let parsed: AcceptBody = body(bytes)?;
    Ok(AcceptInvitation {
        identity_id,
        invitation_bundle: artifact(
            "invitation_bundle_base64",
            "InvitationBundle",
            mabel_core::MAX_INVITATION_BUNDLE_BYTES,
            parsed.invitation_bundle_base64.as_ref(),
        )?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmitBody {
    by: Option<String>,
    acceptance_base64: Option<String>,
}

/// `POST /api/identities/{identity_id}/memberships/admissions`.
pub(super) fn admit_acceptance(
    ledger_id: Id,
    bytes: &[u8],
) -> Result<AdmitAcceptance, ServiceError> {
    let parsed: AdmitBody = body(bytes)?;
    Ok(AdmitAcceptance {
        ledger_id,
        by: id_field(IdKind::Identity, "by", required("by", parsed.by.as_ref())?)?,
        acceptance: artifact(
            "acceptance_base64",
            "AcceptanceFile",
            mabel_core::MAX_ACCEPTANCE_FILE_BYTES,
            parsed.acceptance_base64.as_ref(),
        )?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveBody {
    by: Option<String>,
    target: Option<String>,
}

/// `POST /api/identities/{identity_id}/memberships/removals`.
pub(super) fn remove_membership(
    ledger_id: Id,
    bytes: &[u8],
) -> Result<RemoveMembership, ServiceError> {
    let parsed: RemoveBody = body(bytes)?;
    Ok(RemoveMembership {
        ledger_id,
        by: id_field(IdKind::Identity, "by", required("by", parsed.by.as_ref())?)?,
        target: id_field(
            IdKind::Identity,
            "target",
            required("target", parsed.target.as_ref())?,
        )?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustBody {
    issuer: Option<String>,
    subject: Option<String>,
}

/// `POST /api/trust`.
pub(super) fn add_trust(bytes: &[u8]) -> Result<AddTrust, ServiceError> {
    let parsed: TrustBody = body(bytes)?;
    let issuer = id_field(
        IdKind::Identity,
        "issuer",
        required("issuer", parsed.issuer.as_ref())?,
    )?;
    let subject_raw = required("subject", parsed.subject.as_ref())?;
    let subject = id_field(IdKind::Identity, "subject", subject_raw)?;
    if subject == issuer {
        return Err(ServiceError::schema(
            "subject_equals_ledger",
            "subject must differ from the issuer ledger id",
        )
        .with_detail("field", "subject")
        .with_detail("value", subject.as_str()));
    }
    Ok(AddTrust { issuer, subject })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokeBody {
    issuer: Option<String>,
}

/// `POST /api/trust/{event_id}/revoke`.
///
/// The body names the issuer so the node needs no event-id-to-ledger index
/// (`contracts/README.md`, "Decisions taken here").
pub(super) fn revoke(bytes: &[u8]) -> Result<Id, ServiceError> {
    let parsed: RevokeBody = body(bytes)?;
    id_field(
        IdKind::Identity,
        "issuer",
        required("issuer", parsed.issuer.as_ref())?,
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PushBody {
    identity_id: Option<String>,
    to: Option<String>,
}

/// `POST /api/sync/push`.
pub(super) fn push(bytes: &[u8]) -> Result<PushRequest, ServiceError> {
    let parsed: PushBody = body(bytes)?;
    let identity_id = id_field(
        IdKind::Identity,
        "identity_id",
        required("identity_id", parsed.identity_id.as_ref())?,
    )?;
    let to = match parsed.to.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(id_field(IdKind::Endpoint, "to", raw)?),
    };
    Ok(PushRequest { identity_id, to })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchBody {
    #[serde(default)]
    from: Option<String>,
}

/// `POST /api/identities/{identity_id}/fetch`.
///
/// An absent or `null` `from` means every known witness, in the crawler's
/// source order (proposal 004).
pub(super) fn fetch_identity(identity_id: Id, bytes: &[u8]) -> Result<FetchIdentity, ServiceError> {
    let parsed: FetchBody = body(bytes)?;
    let from = match parsed.from.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(id_field(IdKind::Endpoint, "from", raw)?),
    };
    Ok(FetchIdentity { identity_id, from })
}

/// `?input=` on `GET /api/resolve`: one identity id, one hostname or one
/// `mabel://` link (proposal 006 section 7).
///
/// The parameter is decoded exactly once, here, which is what a query decoder
/// does; the decoded bytes go to the link grammar unchanged, and that grammar
/// refuses percent-encoding outright. So `%252f` decodes once to `%2f` and is
/// refused rather than decoded again into `/`. No layer below this one decodes
/// anything.
///
/// The raw query string is read pair by pair rather than through a map, because
/// a map cannot tell `input` sent twice from `input` sent once.
///
/// # Errors
///
/// Returns code 2 with `unknown_query_parameter` for any other key and for a
/// repeated `input`, `missing_field` when `input` is absent or empty,
/// `invalid_mabel_link` for a string that means to be a link and is not, and
/// `malformed_hostname` for anything else that is neither an id nor a hostname.
pub(super) fn resolve(raw_query: Option<&str>) -> Result<ResolveInput, ServiceError> {
    const INPUT: &str = "input";

    let mut given: Option<String> = None;
    for pair in raw_query.unwrap_or_default().split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != INPUT {
            return Err(ServiceError::usage(
                "unknown_query_parameter",
                format!("{name} is not a parameter of this route"),
            )
            .with_detail("parameter", name));
        }
        if given.is_some() {
            return Err(ServiceError::usage(
                "unknown_query_parameter",
                format!("{INPUT} may be given once"),
            )
            .with_detail("parameter", INPUT));
        }
        given = Some(decode_once(value));
    }
    let input = given.filter(|value| !value.is_empty()).ok_or_else(|| {
        ServiceError::usage("missing_field", format!("{INPUT} is required"))
            .with_detail("field", INPUT)
    })?;

    if MabelLink::looks_like_link(&input) {
        let link = MabelLink::parse(&input).map_err(|error| {
            ServiceError::usage(
                error.reason(),
                format!("{input} is not a mabel link: {error}"),
            )
            .with_detail("input", input.clone())
            .with_detail("detail", error.clause())
        })?;
        return Ok(ResolveInput::Link {
            identity_id: render_id(link.identity().as_bytes()),
            endpoints: link
                .endpoints()
                .iter()
                .map(|endpoint| render_id(endpoint.as_bytes()))
                .collect(),
        });
    }
    if let Some(identity) = Id::parse(&input) {
        return Ok(ResolveInput::Identity(identity));
    }
    Ok(ResolveInput::Hostname(hostname(&input)?))
}

/// One pass of percent-decoding, leaving a malformed escape as its own bytes so
/// the grammar below refuses it rather than a decoder guessing at it.
///
/// Nothing is trimmed: a link is refused whole, whitespace included, and the
/// hostname parser does its own trimming for the one kind that tolerates it.
fn decode_once(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

/// 32 bytes as the base32 id every document spells.
fn render_id(bytes: &[u8; 32]) -> Id {
    Id::parse(&mabel_core::render_id(bytes)).expect("32 bytes render as 52 base32 characters")
}

/// The hostname kind of `GET /api/resolve?input=`, and the `hostname` field of
/// a profile.
///
/// The same syntax a profile hostname must satisfy, so a name this route
/// accepts is a name a ledger could claim (proposal 003 section 2).
pub(super) fn hostname(raw: &str) -> Result<String, ServiceError> {
    let trimmed = raw.trim().to_ascii_lowercase();
    check_hostname(&trimmed).map_err(|detail| {
        ServiceError::schema(
            "malformed_hostname",
            format!("{trimmed} is not a hostname: {detail}"),
        )
        .with_detail("value", trimmed.clone())
        .with_detail("detail", detail)
    })?;
    Ok(trimmed)
}

/// The 404 for a path no route claims, so an unknown route answers the
/// envelope like every other failure.
pub(super) fn unknown_route(method: &str, path: &str) -> ServiceError {
    ServiceError::usage("unknown_route", format!("no route for {method} {path}"))
        .with_detail("method", method)
        .with_detail("path", path)
        .with_status(StatusCode::NOT_FOUND)
}

/// The 405 for a route that exists under another method, which is what a
/// mutating request to the read-only witness API gets.
pub(super) fn method_not_allowed(method: &str, path: &str) -> ServiceError {
    ServiceError::usage(
        "method_not_allowed",
        format!("{method} is not allowed on {path}"),
    )
    .with_detail("method", method)
    .with_detail("path", path)
    .with_status(StatusCode::METHOD_NOT_ALLOWED)
}

#[cfg(test)]
mod tests {
    use super::{
        IdKind, MAX_EVENT_LIMIT, Query, ResolveInput, create_identity, endpoints, event_page,
        fetch_identity, fork_query, hostname, id, ledger_page, limit, resolve, witnesses,
    };
    use crate::api::documents::Id;
    use serde_json::json;

    const ALICE: &str = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
    const BOB: &str = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
    /// A real endpoint id, which the link grammar checks is a curve point.
    const WITNESS_ONE: &str = "zbj22dym2k3btlvjftxmj7kwujgwjgovqthhsjl6ixh5qe43mctq";

    fn query(pairs: &[(&str, &str)]) -> Query {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn an_absent_page_takes_the_defaults() {
        let page = event_page(&query(&[])).expect("no parameters");
        assert_eq!(page.since, 0);
        assert_eq!(page.limit, MAX_EVENT_LIMIT);
    }

    #[test]
    fn a_limit_over_the_maximum_is_clamped_not_rejected() {
        assert_eq!(limit(&query(&[("limit", "1000")]), 256).unwrap(), 256);
        assert_eq!(limit(&query(&[("limit", "12")]), 256).unwrap(), 12);
        assert!(limit(&query(&[("limit", "0")]), 256).is_err());
    }

    #[test]
    fn an_unknown_query_parameter_is_rejected() {
        let error = ledger_page(&query(&[("page", "2")])).expect_err("page is not a parameter");
        assert_eq!(error.reason(), "unknown_query_parameter");
        assert_eq!(error.code(), 2);
    }

    #[test]
    fn an_empty_ledger_id_parameter_means_every_ledger() {
        let parsed = fork_query(&query(&[("ledger_id", "")])).expect("empty means all");
        assert!(parsed.ledger_id.is_none());
    }

    #[test]
    fn an_id_of_the_wrong_shape_names_its_kind_in_the_reason() {
        assert_eq!(
            id(IdKind::Identity, "alice")
                .expect_err("not an id")
                .reason(),
            "malformed_identity_id"
        );
        assert_eq!(
            id(IdKind::Ledger, "sfttwjzd")
                .expect_err("not an id")
                .reason(),
            "malformed_ledger_id"
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_a_schema_error() {
        let error = create_identity(b"not json").expect_err("not json");
        assert_eq!(error.code(), 10);
        assert_eq!(error.reason(), "malformed_json");
    }

    #[test]
    fn an_unknown_body_field_is_rejected() {
        let bytes = json!({"alias": "alice", "kind": "person"}).to_string();
        let error = create_identity(bytes.as_bytes()).expect_err("kind is not a field");
        assert_eq!(error.reason(), "malformed_json");
    }

    #[test]
    fn a_duplicate_witness_is_rejected_before_the_service_is_called() {
        let bytes = json!({"witnesses": [ALICE, ALICE]}).to_string();
        let error = witnesses(bytes.as_bytes()).expect_err("duplicate");
        assert_eq!(error.reason(), "duplicate_witness");

        // An empty set is legal and says nobody keeps this chain, but the key
        // must be sent: the operation replaces the whole set (proposal 006
        // section 1).
        let bytes = json!({"witnesses": []}).to_string();
        assert_eq!(
            witnesses(bytes.as_bytes()).expect("empty is legal"),
            Vec::new()
        );
        let error = witnesses(b"{}").expect_err("the key is required");
        assert_eq!(error.reason(), "missing_field");

        let many: Vec<String> = (0..17)
            .map(|index| {
                let mut bytes = [0u8; 32];
                bytes[0] = index;
                data_encoding::BASE32_NOPAD
                    .encode(&bytes)
                    .to_ascii_lowercase()
            })
            .collect();
        let bytes = json!({"witnesses": many}).to_string();
        let error = witnesses(bytes.as_bytes()).expect_err("seventeen");
        assert_eq!(error.reason(), "witnesses_out_of_range");
    }

    /// The advertisement body takes 0 to 8 distinct endpoint ids (proposal 006
    /// section 2).
    #[test]
    fn an_endpoints_body_is_bounded_and_distinct() {
        let bytes = json!({"endpoints": []}).to_string();
        assert_eq!(
            endpoints(bytes.as_bytes()).expect("empty is legal"),
            Vec::new()
        );

        let bytes = json!({"endpoints": [ALICE, ALICE]}).to_string();
        let error = endpoints(bytes.as_bytes()).expect_err("duplicate");
        assert_eq!(error.reason(), "duplicate_endpoint");

        let many: Vec<String> = std::iter::repeat_n(ALICE, 9).map(str::to_owned).collect();
        let bytes = json!({"endpoints": many}).to_string();
        let error = endpoints(bytes.as_bytes()).expect_err("nine");
        assert_eq!(error.reason(), "endpoints_out_of_range");

        let error = endpoints(b"{}").expect_err("the key is required");
        assert_eq!(error.reason(), "missing_field");
    }

    #[test]
    fn a_fetch_body_reads_from_as_optional() {
        let ledger = Id::parse(ALICE).expect("a fixture id");
        for body in [json!({}), json!({"from": null}), json!({"from": ""})] {
            let request = fetch_identity(ledger.clone(), body.to_string().as_bytes())
                .unwrap_or_else(|error| panic!("{body}: {error}"));
            assert_eq!(request.from, None, "{body}");
            assert_eq!(request.identity_id, ledger, "{body}");
        }
        let body = json!({"from": BOB}).to_string();
        let request = fetch_identity(ledger, body.as_bytes()).expect("a pinned source");
        assert_eq!(request.from.as_ref().map(Id::as_str), Some(BOB));
    }

    #[test]
    fn a_fetch_body_refuses_a_from_that_is_not_an_endpoint_id() {
        let ledger = Id::parse(ALICE).expect("a fixture id");
        let body = json!({"from": "witness-one"}).to_string();
        let error = fetch_identity(ledger, body.as_bytes()).expect_err("not an endpoint id");
        assert_eq!(error.reason(), "malformed_endpoint_id");
    }

    /// `?input=` reads one of three kinds, and says which (proposal 006
    /// section 7).
    #[test]
    fn resolve_reads_an_id_a_hostname_and_a_link() {
        let query = format!("input={ALICE}");
        assert_eq!(
            resolve(Some(&query)).expect("an identity id"),
            ResolveInput::Identity(Id::parse(ALICE).expect("a fixture id"))
        );
        assert_eq!(
            resolve(Some("input=Alice.Example")).expect("a hostname"),
            ResolveInput::Hostname("alice.example".to_owned())
        );

        // `://` is percent-encoded by any honest client, so the one decode
        // this layer does has to happen before the grammar reads the string.
        let link = format!("input=mabel%3A%2F%2F{ALICE}%3Fendpoints%3D{WITNESS_ONE}");
        assert_eq!(
            resolve(Some(&link)).expect("a link"),
            ResolveInput::Link {
                identity_id: Id::parse(ALICE).expect("a fixture id"),
                endpoints: vec![Id::parse(WITNESS_ONE).expect("a fixture id")],
            }
        );
        // An uppercase paste is the same link, rendered lowercase.
        let shouted = format!("input=MABEL://{}", ALICE.to_ascii_uppercase());
        assert_eq!(
            resolve(Some(&shouted)).expect("a link"),
            ResolveInput::Link {
                identity_id: Id::parse(ALICE).expect("a fixture id"),
                endpoints: Vec::new(),
            }
        );
    }

    /// A string that means to be a link is refused as one rather than looked
    /// up as a hostname, and `%252f` is refused rather than decoded twice.
    #[test]
    fn resolve_refuses_a_broken_link_and_never_decodes_twice() {
        let error = resolve(Some(&format!("input=mabel%3A%2F%2F{ALICE}%252f")))
            .expect_err("percent-encoding");
        assert_eq!(error.reason(), "invalid_mabel_link");
        assert_eq!(error.code(), 2);
        assert_eq!(
            error.details()["input"],
            json!(format!("mabel://{ALICE}%2f")),
            "the string as this layer received it, decoded once"
        );
        assert_eq!(
            error.details()["detail"],
            json!("it holds percent-encoding")
        );

        // Three good endpoints and one bad one are refused together.
        let error = resolve(Some(&format!(
            "input=mabel%3A%2F%2F{ALICE}%3Fendpoints%3D{WITNESS_ONE},nope"
        )))
        .expect_err("one bad endpoint");
        assert_eq!(error.reason(), "invalid_mabel_link");

        // A hostname-shaped string is not a link attempt and gets the hostname
        // refusal instead.
        assert_eq!(
            resolve(Some("input=alice_example"))
                .expect_err("not a hostname")
                .reason(),
            "malformed_hostname"
        );
    }

    #[test]
    fn resolve_refuses_a_repeated_input_an_unknown_key_and_no_input() {
        let error =
            resolve(Some(&format!("input={ALICE}&input=alice.example"))).expect_err("input twice");
        assert_eq!(error.reason(), "unknown_query_parameter");
        assert_eq!(error.details()["parameter"], json!("input"));

        let error = resolve(Some("hostname=alice.example")).expect_err("not a parameter");
        assert_eq!(error.reason(), "unknown_query_parameter");
        assert_eq!(error.details()["parameter"], json!("hostname"));

        for query in [None, Some(""), Some("input=")] {
            let error = resolve(query).expect_err("no input");
            assert_eq!(error.reason(), "missing_field", "{query:?}");
            assert_eq!(error.details()["field"], json!("input"), "{query:?}");
        }
    }

    #[test]
    fn a_hostname_is_lowercased_and_checked_against_the_profile_rule() {
        assert_eq!(
            hostname(" Alice.Example ").expect("a hostname"),
            "alice.example"
        );
        for bad in ["alice_example", "alice", "alice.example.", ""] {
            let error = hostname(bad).expect_err(bad);
            assert_eq!(error.reason(), "malformed_hostname", "{bad}");
            assert_eq!(error.code(), 10, "{bad}");
        }
    }
}

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

use super::documents::{DeclaredKind, ID_LENGTH, Id, RoleName, VerifyKind};
use super::error::ServiceError;
use super::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, ForkQuery,
    Invite, LookupRequest, PageRequest, PushRequest, RemoveMembership, ReplaceProfile, SetContact,
    VerifyRequest,
};
use crate::contacts::{ContactTextError, MAX_NICKNAME_BYTES, MAX_NOTE_BYTES, normalize};

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

/// `?offset=` and `?limit=` for `GET /api/ledgers`.
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
}

/// `POST /api/identities`.
///
/// An absent `declared_kind` means `person`. An absent `founder` means a raw
/// root: the new ledger keys itself (proposal 002 section 2).
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
    })
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

/// `POST /api/identities/{identity_id}/witnesses`.
///
/// The witness list must hold 1 to 16 distinct endpoint ids (proposal 001
/// section 3.3).
pub(super) fn witnesses(bytes: &[u8]) -> Result<Vec<Id>, ServiceError> {
    const MAX_WITNESSES: usize = 16;
    const RANGE_MESSAGE: &str = "witnesses must hold 1 to 16 distinct endpoint ids";

    let parsed: WitnessesBody = body(bytes)?;
    let raw = parsed.witnesses.ok_or_else(|| missing_field("witnesses"))?;
    if raw.is_empty() || raw.len() > MAX_WITNESSES {
        return Err(
            ServiceError::schema("witnesses_out_of_range", RANGE_MESSAGE)
                .with_detail("field", "witnesses")
                .with_detail("count", raw.len()),
        );
    }
    let mut witnesses = Vec::with_capacity(raw.len());
    for value in &raw {
        let endpoint = id_field(IdKind::Endpoint, "witnesses", value)?;
        if witnesses.contains(&endpoint) {
            return Err(ServiceError::schema("duplicate_witness", RANGE_MESSAGE)
                .with_detail("field", "witnesses")
                .with_detail("value", endpoint.as_str()));
        }
        witnesses.push(endpoint);
    }
    Ok(witnesses)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileBody {
    /// Present and `null` clears the name; absent is refused. The outer
    /// `Option` is "was the key sent", the inner one is its value.
    #[serde(default, deserialize_with = "sent")]
    display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "sent")]
    hostname: Option<Option<String>>,
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
/// Both keys are required and either may be `null`: the operation is
/// replacement, and a body that names one field would silently clear the
/// other (proposal 003 section 1).
pub(super) fn replace_profile(
    identity_id: Id,
    bytes: &[u8],
) -> Result<ReplaceProfile, ServiceError> {
    let parsed: ProfileBody = body(bytes)?;
    let name = |field: &str, value: Option<Option<String>>| match value {
        None => Err(missing_field(field)),
        Some(value) => Ok(value
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())),
    };
    Ok(ReplaceProfile {
        display_name: name("display_name", parsed.display_name)?,
        hostname: name("hostname", parsed.hostname)?,
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
struct VerifyBody {
    kind: Option<String>,
    issuer: Option<String>,
    subject: Option<String>,
    ledger_id: Option<String>,
    from: Option<String>,
}

/// `POST /api/verify`.
pub(super) fn verify(bytes: &[u8]) -> Result<VerifyRequest, ServiceError> {
    let parsed: VerifyBody = body(bytes)?;
    let kind_raw = required("kind", parsed.kind.as_ref())?;
    let kind = match kind_raw {
        "trust" => VerifyKind::Trust,
        "ledger" => VerifyKind::Ledger,
        _ => {
            return Err(ServiceError::schema(
                "unknown_enum_value",
                "kind must be one of trust, ledger",
            )
            .with_detail("field", "kind")
            .with_detail("value", kind_raw));
        }
    };
    let from = match parsed.from.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(id_field(IdKind::Endpoint, "from", raw)?),
    };
    match kind {
        VerifyKind::Trust => Ok(VerifyRequest::Trust {
            issuer: id_field(
                IdKind::Identity,
                "issuer",
                required("issuer", parsed.issuer.as_ref())?,
            )?,
            subject: id_field(
                IdKind::Identity,
                "subject",
                required("subject", parsed.subject.as_ref())?,
            )?,
            from,
        }),
        VerifyKind::Ledger => Ok(VerifyRequest::Ledger {
            ledger_id: id_field(
                IdKind::Ledger,
                "ledger_id",
                required("ledger_id", parsed.ledger_id.as_ref())?,
            )?,
            from,
        }),
    }
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
        IdKind, MAX_EVENT_LIMIT, Query, create_identity, event_page, fork_query, id, ledger_page,
        limit, verify, witnesses,
    };
    use crate::api::service::VerifyRequest;
    use serde_json::json;

    const ALICE: &str = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
    const BOB: &str = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";

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
        let bytes = json!({"witnesses": []}).to_string();
        let error = witnesses(bytes.as_bytes()).expect_err("empty");
        assert_eq!(error.reason(), "witnesses_out_of_range");
    }

    #[test]
    fn verify_ledger_names_its_ledger_in_ledger_id() {
        let bytes = json!({"kind": "ledger", "ledger_id": ALICE, "from": null}).to_string();
        let request = verify(bytes.as_bytes()).expect("a ledger request");
        assert!(matches!(request, VerifyRequest::Ledger { .. }));

        let bytes = json!({"kind": "ledger"}).to_string();
        let error = verify(bytes.as_bytes()).expect_err("no ledger named");
        assert_eq!(error.reason(), "missing_field");
    }

    #[test]
    fn verify_trust_needs_an_issuer_and_a_subject() {
        let bytes = json!({"kind": "trust", "issuer": ALICE, "subject": BOB, "from": null});
        let request = verify(bytes.to_string().as_bytes()).expect("a trust request");
        assert!(matches!(request, VerifyRequest::Trust { .. }));

        let bytes = json!({"kind": "trust", "issuer": ALICE});
        let error = verify(bytes.to_string().as_bytes()).expect_err("no subject");
        assert_eq!(error.reason(), "missing_field");
    }
}

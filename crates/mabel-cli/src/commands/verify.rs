//! `mabel verify ledger|trust`, against what this home holds.
//!
//! Both reports name their source, head and fetch time, and neither claims
//! anything beyond the one ledger it read (flag R, proposal 001 section 6). The
//! source of a local verification is this node's own endpoint id: no witness is
//! asked, since fetching from a peer is ticket 011.
//!
//! `verify trust` exits 0 for `trusted: true` and for `trusted: false`, and for
//! a subject no source holds. `verify ledger` on a chain that breaks part way
//! exits 20 with the report inside `details`, because partial validity is a
//! failure, not a result (section 3.6).

use mabel_core::IdentityId;
use mabel_node::api::documents::{
    Id, LedgerReport, RevokedAttestation, SUBJECT_CONTROL_SENTENCE, SubjectResolution, TrustReport,
    VERIFIED_MEANS_SENTENCE, VerifyKind,
};
use mabel_node::now_ms;
use serde_json::{Map, Value};

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::render::{Outcome, rfc3339_utc};

/// The sentence a subject no source holds carries (`contracts/cli/
/// verify-trust.json`).
const UNRESOLVED_SUBJECT_NOTE: &str = "subject: unresolved (not held by any queried source)";

/// `mabel verify ledger <alias|id>`.
pub fn ledger(ctx: &Context, name: &str) -> Result<Outcome> {
    let ledger = ctx.resolve(name)?;
    let source = ctx.source()?;
    require_held(ctx, ledger, &source)?;
    let loaded = ctx.load(ledger)?;
    let fetched_at_ms = now_ms();
    let valid_to_seq = loaded.valid_to_seq();

    let mut report = LedgerReport {
        kind: VerifyKind::Ledger,
        ledger_id: ids::identity(ledger),
        declared_kind: loaded.declared_kind(),
        valid: loaded.violation.is_none(),
        valid_to_seq,
        failed_at_seq: loaded.violation.as_ref().map(|violation| violation.seq),
        event_count: loaded.event_count,
        source: source.clone(),
        sources_queried: vec![source.clone()],
        head_seq: loaded.head_seq,
        head_event: ids::event(loaded.head_event),
        fetched_at_ms,
        statement: String::new(),
        verified_means: VERIFIED_MEANS_SENTENCE.to_owned(),
    };

    let Some(violation) = &loaded.violation else {
        report.statement = as_of(valid_to_seq, &report.ledger_id, &source, fetched_at_ms);
        let text = format!("{}\n{VERIFIED_MEANS_SENTENCE}", report.statement);
        return Outcome::new(&report, text);
    };

    report.statement = format!(
        "valid to seq {valid_to_seq} of {}, fetched from {source} at {}; failed at seq {}: {}",
        report.ledger_id,
        rfc3339_utc(fetched_at_ms),
        violation.seq,
        violation.reason
    );
    Err(
        CliError::ledger(violation.code(), loaded.failure_message(violation))
            .with_details(details(&report, loaded.failed_event.map(ids::event))),
    )
}

/// `mabel verify trust --issuer <alias|id> --subject <alias|id>`.
pub fn trust(ctx: &Context, issuer: &str, subject: &str) -> Result<Outcome> {
    let issuer = ctx.resolve(issuer)?;
    let subject = ctx.resolve(subject)?;
    let source = ctx.source()?;
    require_held(ctx, issuer, &source)?;
    let loaded = ctx.load(issuer)?;
    loaded.require_valid()?;
    let fetched_at_ms = now_ms();
    let head_seq = loaded.head_seq;

    let entries: Vec<_> = loaded
        .trust()
        .into_iter()
        .filter(|entry| entry.subject == ids::identity(subject))
        .collect();
    let standing = entries.iter().find(|entry| !entry.revoked);
    let revoked: Vec<RevokedAttestation> = entries
        .iter()
        .filter(|entry| entry.revoked)
        .filter_map(|entry| {
            Some(RevokedAttestation {
                attestation_event: entry.attestation_event.clone(),
                attestation_seq: entry.attestation_seq,
                revocation_event: entry.revocation_event.clone()?,
                revocation_seq: entry.revocation_seq?,
            })
        })
        .collect();

    let resolved = ctx.store(subject).head()?.is_some();
    let issuer_id = ids::identity(issuer);
    let statement = format!(
        "{}{}",
        as_of(head_seq, &issuer_id, &source, fetched_at_ms),
        revocation_clause(head_seq, &revoked)
    );
    let report = TrustReport {
        kind: VerifyKind::Trust,
        trusted: standing.is_some(),
        issuer: issuer_id,
        subject: ids::identity(subject),
        subject_resolution: if resolved {
            SubjectResolution::Resolved
        } else {
            SubjectResolution::Unresolved
        },
        subject_note: (!resolved).then(|| UNRESOLVED_SUBJECT_NOTE.to_owned()),
        attestation_event: standing.map(|entry| entry.attestation_event.clone()),
        attestation_seq: standing.map(|entry| entry.attestation_seq),
        revoked_count: revoked.len() as u64,
        revoked_attestations: revoked,
        source: source.clone(),
        sources_queried: vec![source],
        head_seq,
        head_event: ids::event(loaded.head_event),
        fetched_at_ms,
        statement,
        subject_control: SUBJECT_CONTROL_SENTENCE.to_owned(),
        verified_means: VERIFIED_MEANS_SENTENCE.to_owned(),
    };

    let mut text = format!("trusted: {}\n{}", report.trusted, report.statement);
    // Proposal 002 section 5: the text names who signed, so a delegate's
    // signature is not read as the subject's.
    if let Some(entry) = standing
        && let Some(attestation) = loaded.state.attestation(&parse(&entry.attestation_event))
    {
        text.push_str(&format!(
            "\nsigned by principal {}",
            attestation.signing_principal
        ));
    }
    if let Some(note) = &report.subject_note {
        text.push_str(&format!("\n{note}"));
    }
    text.push_str(&format!(
        "\n{SUBJECT_CONTROL_SENTENCE}\n{VERIFIED_MEANS_SENTENCE}"
    ));
    Outcome::new(&report, text)
}

/// `valid as of seq N of <ledger>, fetched from <source> at <RFC 3339>`.
fn as_of(seq: u64, ledger: &Id, source: &Id, fetched_at_ms: u64) -> String {
    format!(
        "valid as of seq {seq} of {ledger}, fetched from {source} at {}",
        rfc3339_utc(fetched_at_ms)
    )
}

/// The revocation clause of a trust statement, which never says "unrevoked".
fn revocation_clause(head_seq: u64, revoked: &[RevokedAttestation]) -> String {
    if revoked.is_empty() {
        return format!("; no revocation up to seq {head_seq}");
    }
    revoked
        .iter()
        .map(|entry| {
            format!(
                "; attestation {} revoked at seq {}",
                entry.attestation_event, entry.revocation_seq
            )
        })
        .collect()
}

/// Code 30 when this home holds no events for the ledger: the local store is
/// the only source a local verification has.
fn require_held(ctx: &Context, ledger: IdentityId, source: &Id) -> Result<()> {
    if ctx.store(ledger).head()?.is_some() {
        return Ok(());
    }
    Err(CliError::network(
        "no_source_available",
        format!("no source answered for {ledger}"),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("sources_queried", [source.as_str()]))
}

/// The report as the `details` of a code-20 envelope: every report field
/// except the pitfall-8 sentence, plus the event that failed
/// (`contracts/cli/verify-ledger.json`, the partial-validity case).
fn details(report: &LedgerReport, failed_event: Option<Id>) -> Map<String, Value> {
    let mut details = match serde_json::to_value(report) {
        Ok(Value::Object(fields)) => fields,
        _ => Map::new(),
    };
    details.remove("verified_means");
    details.insert(
        "failed_event".to_owned(),
        failed_event.map_or(Value::Null, |id| Value::String(id.as_str().to_owned())),
    );
    details
}

/// Reads back an id the report already rendered.
fn parse(id: &Id) -> mabel_core::EventId {
    id.as_str().parse().expect("a rendered event id parses")
}

//! `mabel verify ledger|trust`, against a peer or against what this home
//! holds.
//!
//! Where a report is read from is decided before anything is dialled: a
//! `--from` pins one source, a ledger that names witnesses is verified against
//! every one of them in parallel, and a ledger that names none is read from
//! this home, whose source is this node's own endpoint id. Both reports name
//! their source, head and fetch time, and neither claims anything beyond the
//! one chain it read (flag R, proposal 001 section 6).
//!
//! `verify trust` exits 0 for `trusted: true` and for `trusted: false`, and
//! for a subject no source holds. `verify ledger` on a chain that breaks part
//! way exits 20 with the report inside `details`, because partial validity is
//! a failure, not a result (section 3.6).

use mabel_core::IdentityId;
use mabel_node::api::documents::{
    Id, LedgerReport, RevokedAttestation, SUBJECT_CONTROL_SENTENCE, SigningPrincipal,
    SubjectResolution, TrustReport, UNRESOLVED_SUBJECT_NOTE, VERIFIED_MEANS_SENTENCE, VerifyKind,
};
use mabel_node::now_ms;
use mabel_node::wallet::{Sources, Verifier, WalletCore, WalletSync, as_of, revocation_clause};
use serde_json::{Map, Value};

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::on_network;
use crate::render::{Outcome, rfc3339_utc};

/// `mabel verify ledger <alias|id> [--from <endpoint id>] [--peer <ticket>]`.
///
/// # Errors
///
/// Returns code 20 for a chain that does not verify and for equivocation
/// between two sources, and code 30 when no source answered.
pub fn ledger(
    ctx: &Context,
    name: &str,
    from: Option<&str>,
    tickets: &[String],
) -> Result<Outcome> {
    let ledger = ctx.resolve(name)?;
    match plan(ctx, ledger, from, tickets)? {
        Sources::Local(_) => local_ledger(ctx, ledger),
        remote => {
            let report = on_network(
                ctx,
                tickets,
                |core: WalletCore, sync: WalletSync| async move {
                    let verified = Verifier::new(&core, Some(&sync))
                        .verify(ledger, remote)
                        .await?;
                    Ok(mabel_node::wallet::ledger_report(&verified))
                },
            )?;
            render_ledger(&report)
        }
    }
}

/// `mabel verify trust --issuer <alias|id> --subject <alias|id> [--from
/// <endpoint id>] [--peer <ticket>]`.
///
/// # Errors
///
/// As [`ledger`]. A subject no source holds is not a failure: the report says
/// `unresolved` and the command exits 0 (proposal 001 section 3.7).
pub fn trust(
    ctx: &Context,
    issuer: &str,
    subject: &str,
    from: Option<&str>,
    tickets: &[String],
) -> Result<Outcome> {
    let issuer = ctx.resolve(issuer)?;
    let subject = ctx.resolve(subject)?;
    match plan(ctx, issuer, from, tickets)? {
        Sources::Local(_) => local_trust(ctx, issuer, subject),
        remote => {
            let report = on_network(
                ctx,
                tickets,
                |core: WalletCore, sync: WalletSync| async move {
                    let verifier = Verifier::new(&core, Some(&sync));
                    let verified = verifier.verify(issuer, remote).await?;
                    let resolution = verifier.resolve(subject, &verified).await;
                    Ok(mabel_node::wallet::trust_report(
                        &verified, subject, resolution,
                    ))
                },
            )?;
            render_trust(&report)
        }
    }
}

/// Which sources answer for this ledger, decided before an endpoint is bound.
///
/// A home that holds nothing knows no witness for a ledger it has never seen,
/// so the endpoints of the `--peer` tickets stand in as the sources to ask.
/// This is the fresh-verifier case of proposal 001 section 11: the only thing
/// such a home was told is where to look. It changes no answer, since every
/// candidate a peer serves is still folded from nothing and its ledger id is
/// still required to equal the one that was asked for.
fn plan(
    ctx: &Context,
    ledger: IdentityId,
    from: Option<&str>,
    tickets: &[String],
) -> Result<Sources> {
    let from = from.map(ids::parse_endpoint).transpose()?;
    let core = WalletCore::new(ctx.home().clone());
    match mabel_node::wallet::sources(&core, ledger, from) {
        Ok(sources) => Ok(sources),
        Err(error) if error.reason() == "no_source_available" && !tickets.is_empty() => {
            let peers: Vec<_> = crate::network::parse_peers(tickets)?
                .into_iter()
                .map(|peer| peer.id)
                .collect();
            Ok(Sources::Witnesses(peers))
        }
        Err(error) => Err(error.into()),
    }
}

/// `mabel verify ledger` against this home's own copy.
fn local_ledger(ctx: &Context, ledger: IdentityId) -> Result<Outcome> {
    let source = ctx.source()?;
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
        return render_ledger(&report);
    };

    report.statement = format!(
        "valid to seq {valid_to_seq} of {}, fetched from {source} at {}; failed at seq {}: {}",
        ids::shown(&report.ledger_id),
        rfc3339_utc(fetched_at_ms),
        violation.seq,
        violation.reason
    );
    Err(
        CliError::ledger(violation.code(), loaded.failure_message(violation))
            .with_details(details(&report, loaded.failed_event.map(ids::event))),
    )
}

/// `mabel verify trust` against this home's own copy.
fn local_trust(ctx: &Context, issuer: IdentityId, subject: IdentityId) -> Result<Outcome> {
    let source = ctx.source()?;
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
    // Proposal 002 section 5: the report names who signed, so a delegate's
    // signature is not read as the subject's.
    let signing_principal = standing.and_then(|entry| {
        let event = ids::parse_event(entry.attestation_event.as_str()).ok()?;
        let attestation = loaded.state.attestation(&event)?;
        Some(SigningPrincipal {
            identity: ids::identity(attestation.signing_principal.identity),
            key: ids::key(&attestation.signing_principal.key),
        })
    });
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
        signing_principal,
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
    render_trust(&report)
}

/// The text and the document of a ledger report that verified.
fn render_ledger(report: &LedgerReport) -> Result<Outcome> {
    let text = format!("{}\n{VERIFIED_MEANS_SENTENCE}", report.statement);
    Outcome::new(report, text)
}

/// The text and the document of a trust report.
fn render_trust(report: &TrustReport) -> Result<Outcome> {
    let mut text = format!("trusted: {}\n{}", report.trusted, report.statement);
    if let Some(principal) = &report.signing_principal {
        // The identity carries the prefix and the key does not: they render
        // alike, and only one of them is a mabel id.
        text.push_str(&format!(
            "\nsigned by principal {} ({})",
            ids::shown(&principal.identity),
            principal.key
        ));
    }
    if let Some(note) = &report.subject_note {
        text.push_str(&format!("\n{note}"));
    }
    text.push_str(&format!(
        "\n{SUBJECT_CONTROL_SENTENCE}\n{VERIFIED_MEANS_SENTENCE}"
    ));
    Outcome::new(report, text)
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

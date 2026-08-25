//! `mabel trust add|revoke|list`.
//!
//! The text output never claims an attestation is "unrevoked": it says how far
//! the ledger was read, which is all a reader of one ledger knows (flag R,
//! proposal 001 section 6). `trust add` prints the flag-L sentence, because
//! nothing here proves the subject controls the identity.

use mabel_core::sign::{build_trust_attestation, build_trust_revocation};
use mabel_node::api::documents::SUBJECT_CONTROL_SENTENCE;

use crate::append::{append, ensure_fresh};
use crate::cli::AppendOptions;
use crate::context::Context;
use crate::documents::{AddedTrust, RevokedTrust, TrustList};
use crate::error::{CliError, Result};
use crate::ids;
use crate::render::Outcome;

/// `mabel trust add --issuer <alias|id> --subject <alias|id>`.
pub fn add(ctx: &Context, issuer: &str, subject: &str, options: &AppendOptions) -> Result<Outcome> {
    let issuer = ctx.resolve_local_hinted(issuer, "--issuer")?;
    let subject = ctx.resolve(subject)?;
    ensure_fresh(ctx, issuer, options)?;
    let mut loaded = ctx.load(issuer)?;
    let appended = append(ctx, issuer, &mut loaded, |signer, at, timestamp_ms| {
        build_trust_attestation(signer, at, subject, timestamp_ms)
    })?;

    let document = AddedTrust {
        issuer: ids::identity(issuer),
        subject: ids::identity(subject),
        attestation_event: ids::event(appended.event_id),
        attestation_seq: appended.seq,
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
        pushed: false,
    };
    let text = format!(
        "attested {} at seq {} of {}\n{SUBJECT_CONTROL_SENTENCE}",
        ids::shown(subject),
        appended.seq,
        ids::shown(issuer)
    );
    Outcome::new(&document, text)
}

/// `mabel trust revoke --issuer <alias|id> --attestation <event id>`.
pub fn revoke(
    ctx: &Context,
    issuer: &str,
    attestation: &str,
    options: &AppendOptions,
) -> Result<Outcome> {
    let issuer = ctx.resolve_local_hinted(issuer, "--issuer")?;
    let target = ids::parse_event(attestation)?;
    ensure_fresh(ctx, issuer, options)?;
    let mut loaded = ctx.load(issuer)?;
    let attestation_seq = loaded.seq_of.get(&target).copied();
    let subject = loaded.state.attestation(&target).map(|held| held.subject);
    let appended = append(ctx, issuer, &mut loaded, |signer, at, timestamp_ms| {
        build_trust_revocation(signer, at, target, timestamp_ms)
    })?;

    let (Some(subject), Some(attestation_seq)) = (subject, attestation_seq) else {
        // The fold rejects a target it does not hold, so this is unreachable.
        return Err(CliError::internal(
            "attestation_not_folded",
            format!(
                "attestation {target} is not in ledger {}",
                ids::shown(issuer)
            ),
        ));
    };
    let document = RevokedTrust {
        issuer: ids::identity(issuer),
        subject: ids::identity(subject),
        attestation_event: ids::event(target),
        attestation_seq,
        revocation_event: ids::event(appended.event_id),
        revocation_seq: appended.seq,
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
        pushed: false,
    };
    // `target` is the attestation's event id and stays bare; the issuer and the
    // subject are identities and carry the prefix.
    let text = format!(
        "revoked attestation {target} at seq {} of {}\nit named {} at seq {attestation_seq}",
        appended.seq,
        ids::shown(issuer),
        ids::shown(subject)
    );
    Outcome::new(&document, text)
}

/// `mabel trust list --issuer <alias|id>`.
pub fn list(ctx: &Context, issuer: &str) -> Result<Outcome> {
    let issuer = ctx.resolve(issuer)?;
    let loaded = ctx.load(issuer)?;
    let entries = loaded.trust();
    let text = if entries.is_empty() {
        format!(
            "{} has issued no attestations up to seq {}",
            ids::shown(issuer),
            loaded.head_seq
        )
    } else {
        entries
            .iter()
            .map(
                |entry| match (&entry.revocation_event, entry.revocation_seq) {
                    (Some(event), Some(seq)) => format!(
                        "{} at seq {} names {}; revoked at seq {seq} by {event}",
                        entry.attestation_event,
                        entry.attestation_seq,
                        ids::shown(&entry.subject)
                    ),
                    _ => format!(
                        "{} at seq {} names {}; no revocation up to seq {}",
                        entry.attestation_event,
                        entry.attestation_seq,
                        ids::shown(&entry.subject),
                        loaded.head_seq
                    ),
                },
            )
            .collect::<Vec<String>>()
            .join("\n")
    };
    let document = TrustList {
        issuer: ids::identity(issuer),
        head_seq: loaded.head_seq,
        head_event: ids::event(loaded.head_event),
        trust: entries,
    };
    Outcome::new(&document, text)
}

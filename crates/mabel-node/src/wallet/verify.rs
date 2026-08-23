//! Verifying a ledger against several sources, and the two reports that come
//! out (proposal 001 sections 3.7 and 6).
//!
//! With no pinned source every configured witness is asked in parallel and
//! every candidate is verified independently from nothing. A longer candidate
//! wins only if it extends the shorter one event id for event id; two valid
//! candidates that diverge at a sequence are equivocation, reported with both
//! source endpoints and both event ids there.
//!
//! Every report names its source, head sequence, head event and fetch time,
//! and claims nothing about the world beyond the chain it read (flag R).

use iroh::EndpointId;
use mabel_core::{IdentityId, LedgerId};
use tokio::task::JoinSet;

use crate::api::documents::{
    Id, LedgerReport, RevokedAttestation, SUBJECT_CONTROL_SENTENCE, SigningPrincipal,
    SubjectResolution, TrustReport, UNRESOLVED_SUBJECT_NOTE, VERIFIED_MEANS_SENTENCE, VerifyKind,
};
use crate::api::error::ServiceError;
use crate::rfc3339_utc;
use crate::wallet::core::WalletCore;
use crate::wallet::error::{Divergent, equivocation, no_source_available};
use crate::wallet::ids;
use crate::wallet::ledger::LoadedLedger;
use crate::wallet::sync::WalletSync;

/// One verified candidate and where it came from.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The endpoint that served it, or this node for a local read.
    pub source: EndpointId,
    /// The chain, folded.
    pub loaded: LoadedLedger,
}

/// The candidate that won, and everything that was asked.
#[derive(Debug, Clone)]
pub struct Verified {
    /// The winning candidate.
    pub candidate: Candidate,
    /// Every source that was asked, in the order they were named.
    pub sources_queried: Vec<EndpointId>,
    /// When the answer was taken.
    pub fetched_at_ms: u64,
}

impl Verified {
    /// The ledger that was verified.
    #[must_use]
    pub fn ledger(&self) -> LedgerId {
        self.candidate.loaded.ledger
    }

    /// The source as a document spells it.
    #[must_use]
    pub fn source(&self) -> Id {
        ids::key(&self.candidate.source)
    }

    /// Every source as a document spells them.
    #[must_use]
    pub fn sources(&self) -> Vec<Id> {
        self.sources_queried.iter().map(ids::key).collect()
    }
}

/// Where a verification may read a ledger from.
#[derive(Debug, Clone)]
pub enum Sources {
    /// One endpoint, from `--from`.
    Pinned(EndpointId),
    /// Every configured witness, asked in parallel.
    Witnesses(Vec<EndpointId>),
    /// This home's own copy, which is the fallback when a ledger names no
    /// witness.
    Local(EndpointId),
}

/// Where a ledger is read from: the pinned source, else the ledger's
/// witnesses, else this home's own copy.
///
/// A caller decides this before it binds an Iroh endpoint, since a ledger that
/// names no witness needs none.
///
/// # Errors
///
/// Returns code 30 when nothing can answer and code 10 for a malformed
/// `node.json`.
pub fn sources(
    core: &WalletCore,
    ledger: LedgerId,
    from: Option<EndpointId>,
) -> Result<Sources, ServiceError> {
    if let Some(from) = from {
        return Ok(Sources::Pinned(from));
    }
    let witnesses = core.witnesses_of(ledger)?;
    if !witnesses.is_empty() {
        return Ok(Sources::Witnesses(witnesses));
    }
    let here = core.endpoint_id()?;
    if core.holds(ledger)? {
        return Ok(Sources::Local(here));
    }
    Err(no_source_available(ledger, &[here]))
}

/// Collects and compares candidates, then renders the reports.
#[derive(Debug)]
pub struct Verifier<'a> {
    core: &'a WalletCore,
    sync: Option<&'a WalletSync>,
}

impl<'a> Verifier<'a> {
    /// A verifier that may reach the network.
    #[must_use]
    pub fn new(core: &'a WalletCore, sync: Option<&'a WalletSync>) -> Self {
        Self { core, sync }
    }

    /// Where a ledger is read from.
    ///
    /// See [`sources`], which this delegates to so a caller can plan before it
    /// binds an endpoint.
    ///
    /// # Errors
    ///
    /// As [`sources`].
    pub fn sources(
        &self,
        ledger: LedgerId,
        from: Option<EndpointId>,
    ) -> Result<Sources, ServiceError> {
        sources(self.core, ledger, from)
    }

    /// Verifies `ledger` against `sources` and returns the candidate that won.
    ///
    /// # Errors
    ///
    /// Returns code 20 for equivocation or a candidate that does not verify,
    /// and code 30 when no source answered.
    pub async fn verify(
        &self,
        ledger: LedgerId,
        sources: Sources,
    ) -> Result<Verified, ServiceError> {
        let fetched_at_ms = crate::now_ms();
        let (queried, candidates) = match sources {
            Sources::Local(here) => {
                let loaded = self.core.load(ledger)?;
                self.core.require_valid(&loaded)?;
                (
                    vec![here],
                    vec![Candidate {
                        source: here,
                        loaded,
                    }],
                )
            }
            Sources::Pinned(from) => {
                let sync = self.require_sync()?;
                let candidate = sync.candidate(from, ledger).await?;
                (
                    vec![from],
                    candidate
                        .into_iter()
                        .map(|loaded| Candidate {
                            source: from,
                            loaded,
                        })
                        .collect(),
                )
            }
            Sources::Witnesses(witnesses) => {
                let sync = self.require_sync()?;
                (witnesses.clone(), gather(sync, ledger, &witnesses).await?)
            }
        };
        if candidates.is_empty() {
            return Err(no_source_available(ledger, &queried));
        }
        let candidate = winner(ledger, candidates)?;
        Ok(Verified {
            candidate,
            sources_queried: queried,
            fetched_at_ms,
        })
    }

    /// Verifies `ledger` and renders the ledger report.
    ///
    /// # Errors
    ///
    /// As [`Verifier::verify`], plus code 20 with the report inside `details`
    /// when the chain breaks part way.
    pub async fn ledger_report(
        &self,
        ledger: LedgerId,
        from: Option<EndpointId>,
    ) -> Result<LedgerReport, ServiceError> {
        let sources = self.sources(ledger, from)?;
        let verified = self.verify(ledger, sources).await?;
        Ok(ledger_report(&verified))
    }

    /// Verifies an issuer's ledger, resolves the subject best-effort and
    /// renders the trust report.
    ///
    /// # Errors
    ///
    /// As [`Verifier::verify`]. A subject no source holds is not a failure:
    /// the report says `unresolved` and the call succeeds (proposal 001
    /// section 3.7).
    pub async fn trust_report(
        &self,
        issuer: IdentityId,
        subject: IdentityId,
        from: Option<EndpointId>,
    ) -> Result<TrustReport, ServiceError> {
        let sources = self.sources(issuer, from)?;
        let verified = self.verify(issuer, sources).await?;
        let resolution = self.resolve(subject, &verified).await;
        Ok(trust_report(&verified, subject, resolution))
    }

    /// Whether any queried source, or this home, holds the subject's own
    /// ledger and serves a chain whose id equals the requested one.
    ///
    /// Best effort: a subject nobody holds is reported, not failed (proposal
    /// 001 section 3.7).
    pub async fn resolve(&self, subject: IdentityId, verified: &Verified) -> SubjectResolution {
        if self.resolves_locally(subject) {
            return SubjectResolution::Resolved;
        }
        let Some(sync) = self.sync else {
            return SubjectResolution::Unresolved;
        };
        for source in &verified.sources_queried {
            if matches!(sync.candidate(*source, subject).await, Ok(Some(_))) {
                return SubjectResolution::Resolved;
            }
        }
        SubjectResolution::Unresolved
    }

    /// Whether this home's own copy of the subject's ledger resolves it.
    ///
    /// A directory of events is not resolution: the chain must fold with no
    /// violation and be the ledger that was asked for, the same bar a served
    /// candidate passes in [`WalletSync::candidate`]. A local copy that a
    /// tampered event broke falls through to the network instead of
    /// answering `resolved` (proposal 001 section 3.7).
    fn resolves_locally(&self, subject: IdentityId) -> bool {
        let Ok(loaded) = self.core.load(subject) else {
            return false;
        };
        loaded.violation.is_none() && loaded.state.ledger() == Some(subject)
    }

    fn require_sync(&self) -> Result<&'a WalletSync, ServiceError> {
        self.sync.ok_or_else(|| {
            ServiceError::network(
                "no_network",
                "this command has no Iroh endpoint, so it cannot reach a source",
            )
        })
    }
}

/// Asks every witness in parallel and keeps the ones that answered.
///
/// A witness that cannot be reached, or that does not hold the ledger, drops
/// out of the comparison; a witness that serves a chain which does not verify
/// fails the whole verification, because a verifier that quietly ignored a bad
/// candidate would report less than it knows.
async fn gather(
    sync: &WalletSync,
    ledger: LedgerId,
    witnesses: &[EndpointId],
) -> Result<Vec<Candidate>, ServiceError> {
    let mut tasks = JoinSet::new();
    for (index, witness) in witnesses.iter().enumerate() {
        let sync = sync.clone();
        let witness = *witness;
        tasks.spawn(async move { (index, witness, sync.candidate(witness, ledger).await) });
    }
    let mut found: Vec<(usize, Candidate)> = Vec::new();
    let mut invalid: Option<ServiceError> = None;
    while let Some(joined) = tasks.join_next().await {
        let Ok((index, source, result)) = joined else {
            continue;
        };
        match result {
            Ok(Some(loaded)) => found.push((index, Candidate { source, loaded })),
            // An unreachable source is not an error while another can answer.
            Ok(None) => {}
            Err(error) if error.code() == 20 => invalid = Some(error),
            Err(_) => {}
        }
    }
    if let Some(error) = invalid {
        return Err(error);
    }
    found.sort_by_key(|(index, _)| *index);
    Ok(found.into_iter().map(|(_, candidate)| candidate).collect())
}

/// The candidate that extends every other one.
///
/// # Errors
///
/// Returns code 30 when nothing answered and code 20 when two candidates
/// diverge.
fn winner(ledger: LedgerId, candidates: Vec<Candidate>) -> Result<Candidate, ServiceError> {
    let mut best: Option<Candidate> = None;
    for candidate in candidates {
        let Some(current) = best else {
            best = Some(candidate);
            continue;
        };
        match divergence(&current, &candidate) {
            Some((at_seq, first, second)) => {
                return Err(equivocation(ledger, at_seq, &first, &second));
            }
            None => {
                best = Some(
                    if candidate.loaded.event_count() > current.loaded.event_count() {
                        candidate
                    } else {
                        current
                    },
                );
            }
        }
    }
    best.ok_or_else(|| no_source_available(ledger, &[]))
}

/// The first sequence where two candidates hold different events, with the
/// event id each holds there.
fn divergence(left: &Candidate, right: &Candidate) -> Option<(u64, Divergent, Divergent)> {
    for (seq, (one, other)) in left
        .loaded
        .events
        .iter()
        .zip(right.loaded.events.iter())
        .enumerate()
    {
        if one == other {
            continue;
        }
        let at = seq as u64;
        let event_of = |candidate: &Candidate| {
            candidate
                .loaded
                .event_ids
                .get(seq)
                .copied()
                .flatten()
                .map_or_else(|| ids::bytes(&[0u8; 32]), ids::event)
        };
        return Some((
            at,
            Divergent {
                source: left.source,
                event: event_of(left),
            },
            Divergent {
                source: right.source,
                event: event_of(right),
            },
        ));
    }
    None
}

/// `valid as of seq N of <ledger>, fetched from <source> at <RFC 3339>`.
#[must_use]
pub fn as_of(seq: u64, ledger: &Id, source: &Id, fetched_at_ms: u64) -> String {
    format!(
        "valid as of seq {seq} of {ledger}, fetched from {source} at {}",
        rfc3339_utc(fetched_at_ms)
    )
}

/// The revocation clause of a trust statement, which never says "unrevoked"
/// (flag R, proposal 001 section 6).
#[must_use]
pub fn revocation_clause(head_seq: u64, revoked: &[RevokedAttestation]) -> String {
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

/// The ledger report for a candidate that verified to its head.
#[must_use]
pub fn ledger_report(verified: &Verified) -> LedgerReport {
    let loaded = &verified.candidate.loaded;
    let ledger_id = ids::identity(loaded.ledger);
    let source = verified.source();
    let valid_to_seq = loaded.valid_to_seq();
    LedgerReport {
        kind: VerifyKind::Ledger,
        declared_kind: loaded.declared_kind(),
        valid: loaded.violation.is_none(),
        valid_to_seq,
        failed_at_seq: loaded.violation.as_ref().map(|violation| violation.seq),
        event_count: loaded.event_count(),
        statement: as_of(valid_to_seq, &ledger_id, &source, verified.fetched_at_ms),
        ledger_id,
        source,
        sources_queried: verified.sources(),
        head_seq: loaded.head_seq,
        head_event: ids::event(loaded.head_event),
        fetched_at_ms: verified.fetched_at_ms,
        verified_means: VERIFIED_MEANS_SENTENCE.to_owned(),
    }
}

/// The trust report for a verified issuer ledger.
#[must_use]
pub fn trust_report(
    verified: &Verified,
    subject: IdentityId,
    resolution: SubjectResolution,
) -> TrustReport {
    let loaded = &verified.candidate.loaded;
    let source = verified.source();
    let issuer_id = ids::identity(loaded.ledger);
    let subject_id = ids::identity(subject);
    let entries: Vec<_> = loaded
        .trust()
        .into_iter()
        .filter(|entry| entry.subject == subject_id)
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

    // A standing attestation has no revocation, so its statement keeps the
    // plain clause; the revoked history stays in `revoked_attestations`.
    // Naming an old revocation beside `trusted: true` would read as if it
    // applied to the standing claim (flag R: say what was verified).
    let clause = if standing.is_some() {
        format!("; no revocation up to seq {}", loaded.head_seq)
    } else {
        revocation_clause(loaded.head_seq, &revoked)
    };
    let statement = format!(
        "{}{}",
        as_of(loaded.head_seq, &issuer_id, &source, verified.fetched_at_ms),
        clause
    );
    let unresolved = resolution == SubjectResolution::Unresolved;
    TrustReport {
        kind: VerifyKind::Trust,
        trusted: standing.is_some(),
        issuer: issuer_id,
        subject: subject_id,
        subject_resolution: resolution,
        subject_note: unresolved.then(|| UNRESOLVED_SUBJECT_NOTE.to_owned()),
        signing_principal: standing.and_then(|entry| signing_principal(loaded, entry)),
        attestation_event: standing.map(|entry| entry.attestation_event.clone()),
        attestation_seq: standing.map(|entry| entry.attestation_seq),
        revoked_count: revoked.len() as u64,
        revoked_attestations: revoked,
        source,
        sources_queried: verified.sources(),
        head_seq: loaded.head_seq,
        head_event: ids::event(loaded.head_event),
        fetched_at_ms: verified.fetched_at_ms,
        statement,
        subject_control: SUBJECT_CONTROL_SENTENCE.to_owned(),
        verified_means: VERIFIED_MEANS_SENTENCE.to_owned(),
    }
}

/// Who signed the attestation this report answers with (proposal 002
/// section 5).
#[must_use]
pub fn signing_principal(
    loaded: &LoadedLedger,
    entry: &crate::api::documents::TrustEntry,
) -> Option<SigningPrincipal> {
    let event = entry.attestation_event.as_str().parse().ok()?;
    let attestation = loaded.state.attestation(&event)?;
    Some(SigningPrincipal {
        identity: ids::identity(attestation.signing_principal.identity),
        key: ids::key(&attestation.signing_principal.key),
    })
}

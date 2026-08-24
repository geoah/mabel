//! `mabel sync push|fetch`, the two commands that cross the network
//! (proposal 001 section 9).
//!
//! A push reports one row per witness, so a witness that did not answer is
//! visible rather than fatal; the command fails with code 30 only when no
//! witness accepted anything. A fetch verifies what the source served from
//! nothing before a byte of it is stored, and requires the chain's ledger id
//! to equal the one that was asked for, which is what makes an untrusted
//! source safe to fetch from (proposal 001 section 3.7).

use mabel_node::api::documents::{Binding, PushStatus};
use mabel_node::graph::{Resolution, plan_sources};
use mabel_node::wallet::{Fetched, WalletCore, WalletSync};

use crate::context::Context;
use crate::documents::{FetchedLedger, PushedLedger};
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::on_network;
use crate::render::Outcome;

/// `mabel sync push --identity <alias|id> [--to <endpoint id>] [--peer
/// <ticket>]`.
///
/// # Errors
///
/// Returns code 2 when the ledger names no witness and none is pinned, code 30
/// when no witness accepted the push, and code 20 when one rejected it.
pub fn push(
    ctx: &Context,
    identity: &str,
    to: Option<&str>,
    tickets: &[String],
) -> Result<Outcome> {
    // A push signs, so the subject is local and a link's hints have nothing to
    // reach: they are dropped with a warning naming the flag (proposal 006
    // section 7).
    let identity = ctx.resolve_local_hinted(identity, "--identity")?;
    // Which witnesses to push to is decided before an endpoint is bound: a
    // ledger that names none has nothing to dial.
    // `--to` is an endpoint a person named for this push, so it is dialled and
    // never written to `peers.json` (proposal 006 section 5.3).
    let (witnesses, caller) = match to.map(ids::parse_endpoint).transpose()? {
        Some(endpoint) => (vec![endpoint], vec![endpoint]),
        None => (
            WalletCore::new(ctx.home().clone()).witnesses_of(identity)?,
            Vec::new(),
        ),
    };
    if witnesses.is_empty() {
        return Err(CliError::usage(
            "no_witness_configured",
            format!("no endpoint is configured to push {identity} to"),
        )
        .with_detail("ledger_id", identity.to_string()));
    }
    let pushed = on_network(
        ctx,
        tickets,
        |core: WalletCore, sync: WalletSync| async move {
            Ok(sync.push_from(&core, identity, &witnesses, &caller).await?)
        },
    )?;

    let accepted = pushed
        .results
        .iter()
        .filter(|result| result.status == PushStatus::Accepted)
        .count();
    let text = std::iter::once(format!(
        "{} at seq {}, {accepted} of {} witnesses accepted",
        pushed.ledger_id,
        pushed.head_seq,
        pushed.results.len()
    ))
    .chain(pushed.results.iter().map(|result| {
        let status = match result.status {
            PushStatus::Accepted => format!("accepted, stored {}", result.stored),
            PushStatus::Rejected => format!(
                "rejected {} at seq {}",
                result.reject_code.as_deref().unwrap_or("unknown"),
                result.at_seq.unwrap_or_default()
            ),
            PushStatus::Unreachable => "unreachable".to_owned(),
        };
        match &result.message {
            Some(message) => format!("{} {status}: {message}", result.endpoint),
            None => format!("{} {status}", result.endpoint),
        }
    }))
    .collect::<Vec<String>>()
    .join("\n");

    // A hinted endpoint is one no witness identity's ledger confirms, which is
    // a warning and never a refusal: the first push to a new witness always
    // happens before this home holds that witness's ledger (proposal 006
    // section 4.2).
    for result in &pushed.results {
        if result.status == PushStatus::Accepted && result.binding == Binding::Hinted {
            eprintln!(
                "warning: nobody's ledger confirms that {} answers for a witness of {identity}",
                result.endpoint
            );
        }
    }

    if accepted == 0 {
        return Err(CliError::network(
            "all_witnesses_failed",
            format!("no configured witness accepted the push for {identity}"),
        )
        .with_detail("ledger_id", identity.to_string())
        .with_detail("results", &pushed.results));
    }
    Outcome::new(
        &PushedLedger {
            identity_id: ids::identity(identity),
            pushed,
        },
        text,
    )
}

/// `mabel sync fetch <ledger id> [--from <endpoint id>] [--from-witness
/// <mabel id>] [--peer <ticket>]`.
///
/// `--from` is a plain `CallerHint`: the endpoint a human named for this fetch,
/// asked whether or not this home has heard of it (proposal 006 section 5,
/// source 2). `--from-witness` names a witness identity instead and is resolved
/// to endpoints through section 5.1. With neither, the link's hints are asked,
/// and then every source of section 5 in order.
///
/// # Errors
///
/// Returns code 2 with reason `conflicting_source` when both keys are given and
/// `unresolvable_witness` when `--from-witness` names a witness this home can
/// reach no endpoint for, code 30 when no source could be reached or none holds
/// the ledger, code 20 when what one served does not verify, and code 50 when
/// this home already holds a different event at some position.
pub fn fetch(
    ctx: &Context,
    ledger_id: &str,
    from: Option<&str>,
    from_witness: Option<&str>,
    tickets: &[String],
) -> Result<Outcome> {
    if from.is_some() && from_witness.is_some() {
        return Err(CliError::usage(
            "conflicting_source",
            "--from names an endpoint and --from-witness names an identity: give one",
        )
        .with_detail("parameter", "--from-witness"));
    }
    // The subject of a fetch is the thing being fetched, which is the one row
    // of the matrix where a link's hints apply (proposal 006 section 7).
    let (ledger, hints) = ctx.resolve_hinted(ledger_id)?;
    let core = WalletCore::new(ctx.home().clone());
    let sources = match (from, from_witness) {
        (Some(from), _) => {
            if !hints.is_empty() {
                eprintln!(
                    "warning: ignoring the {} the link names: --from says which peer to ask",
                    if hints.len() == 1 {
                        "endpoint"
                    } else {
                        "endpoints"
                    }
                );
            }
            vec![ids::parse_endpoint(from)?]
        }
        (None, Some(witness)) => {
            let witness = ctx.resolve(witness)?;
            let resolution = Resolution::for_operation().with_caller_hints(hints);
            let endpoints = resolution.witness_endpoints(&core, witness)?;
            if endpoints.is_empty() {
                return Err(CliError::usage(
                    "unresolvable_witness",
                    format!("no endpoint is known for the witness {witness}"),
                )
                .with_detail("identity_id", witness.to_string()));
            }
            endpoints
        }
        (None, None) => {
            // Every source of section 5, the link's hints first as source 2.
            let resolution = Resolution::for_operation().with_caller_hints(hints);
            let planned = plan_sources(&core, ledger, &[], &resolution)?;
            let sources: Vec<iroh_base::EndpointId> = planned
                .iter()
                .filter_map(|planned| planned.endpoint)
                .collect();
            if sources.is_empty() {
                return Err(CliError::usage(
                    "missing_argument",
                    "sync fetch needs --from <endpoint id>, --from-witness <mabel id>, a link \
                     that names one, or a configured default witness",
                ));
            }
            sources
        }
    };
    let fetched: Fetched = on_network(
        ctx,
        tickets,
        |core: WalletCore, sync: WalletSync| async move {
            // The hints are tried in the order the link named them, and only a
            // source that could not be reached moves on to the next: a chain that
            // does not verify is the answer, not a reason to ask elsewhere.
            let mut last: Option<CliError> = None;
            for source in sources {
                match sync.fetch(&core, ledger, source).await {
                    Ok(fetched) => return Ok(fetched),
                    Err(error) => {
                        let error = CliError::from(error);
                        if error.exit_code() != 30 {
                            return Err(error);
                        }
                        last = Some(error);
                    }
                }
            }
            Err(last.expect("at least one source was tried"))
        },
    )?;

    let document = FetchedLedger {
        ledger_id: ids::identity(ledger),
        source: ids::key(&fetched.source),
        event_count: fetched.event_count,
        stored: fetched.stored,
        head_seq: fetched.head_seq,
        head_event: ids::event(fetched.head_event),
        fetched_at_ms: fetched.fetched_at_ms,
        controlled_by: fetched.controlled_by.map(ids::identity),
    };
    let mut text = format!(
        "fetched {} events of {} from {}, stored {}\nhead seq {}, verified from nothing",
        document.event_count,
        document.ledger_id,
        document.source,
        document.stored,
        document.head_seq
    );
    // Whether this home may append to what it just stored is the one thing a
    // fetch decides beyond the bytes (ticket 031).
    match &document.controlled_by {
        Some(controller) => text.push_str(&format!(
            "\nthis home may append to it, signing as {controller}"
        )),
        None => text.push_str("\nstored read-only: no identity here controls it"),
    }
    Outcome::new(&document, text)
}

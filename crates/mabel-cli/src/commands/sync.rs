//! `mabel sync push|fetch`, the two commands that cross the network
//! (proposal 001 section 9).
//!
//! A push reports one row per witness, so a witness that did not answer is
//! visible rather than fatal; the command fails with code 30 only when no
//! witness accepted anything. A fetch verifies what the source served from
//! nothing before a byte of it is stored, and requires the chain's ledger id
//! to equal the one that was asked for, which is what makes an untrusted
//! source safe to fetch from (proposal 001 section 3.7).

use mabel_node::api::documents::PushStatus;
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
    let identity = ctx.resolve_local(identity)?;
    // Which witnesses to push to is decided before an endpoint is bound: a
    // ledger that names none has nothing to dial.
    let witnesses = match to.map(ids::parse_endpoint).transpose()? {
        Some(endpoint) => vec![endpoint],
        None => WalletCore::new(ctx.home().clone()).witnesses_of(identity)?,
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
            Ok(sync.push(&core, identity, &witnesses).await?)
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

/// `mabel sync fetch <ledger id> --from <endpoint id> [--peer <ticket>]`.
///
/// # Errors
///
/// Returns code 30 when the source cannot be reached or does not hold the
/// ledger, code 20 when what it served does not verify, and code 50 when this
/// home already holds a different event at some position.
pub fn fetch(ctx: &Context, ledger_id: &str, from: &str, tickets: &[String]) -> Result<Outcome> {
    let ledger = ctx.resolve(ledger_id)?;
    let from = ids::parse_endpoint(from)?;
    let fetched: Fetched = on_network(
        ctx,
        tickets,
        |core: WalletCore, sync: WalletSync| async move { Ok(sync.fetch(&core, ledger, from).await?) },
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

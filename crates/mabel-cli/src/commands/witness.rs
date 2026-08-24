//! `mabel witness add` and `mabel witness set-default`.
//!
//! Two different sets, and only one of them is signed. A `WitnessSet` replaces
//! the whole set a ledger records, so adding one witness means signing the set
//! the ledger already holds plus the new one, and every entry is an identity id
//! (proposal 006 section 1). `node.json.witnesses` is this node's own
//! configuration: the endpoints it queries for any ledger, third in the
//! crawler's source order (proposal 003 section 3).

use mabel_core::sign::build_witness_set;

use crate::append::{append, ensure_fresh};
use crate::cli::AppendOptions;
use crate::context::Context;
use crate::documents::{AddedWitness, DefaultWitnesses};
use crate::error::Result;
use crate::ids;
use crate::render::Outcome;

/// `mabel witness add --identity <alias|id> --witness <alias|id>`.
///
/// The witness is named by identity id: a witness that moves machines keeps the
/// same id, so this event stands whatever endpoint answers for it.
pub fn add(
    ctx: &Context,
    identity: &str,
    witness: &str,
    options: &AppendOptions,
) -> Result<Outcome> {
    let identity = ctx.resolve_local(identity)?;
    let witness = ctx.resolve(witness)?;
    ensure_fresh(ctx, identity, options)?;
    let mut loaded = ctx.load(identity)?;

    let mut witnesses = loaded.state.witness_identities().to_vec();
    if !witnesses.contains(&witness) {
        witnesses.push(witness);
    }
    let appended = append(ctx, identity, &mut loaded, |signer, at, timestamp_ms| {
        build_witness_set(signer, at, &witnesses, timestamp_ms)
    })?;

    let document = AddedWitness {
        identity_id: ids::identity(identity),
        witness: ids::identity(witness),
        witnesses: loaded.witnesses(),
        event_id: ids::event(appended.event_id),
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
    };
    let text = format!(
        "{} witnesses {identity} as of seq {}\nthe set now holds {} witnesses",
        document.witness,
        appended.seq,
        document.witnesses.len()
    );
    Outcome::new(&document, text)
}

/// `mabel witness set-default <endpoint id>...`.
///
/// Writes `node.json.witnesses`, replacing whatever it held. Nothing is
/// signed and no ledger changes: this is the node's configuration, and a
/// running node reads the file at startup, so a restart is what makes an edit
/// take effect.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_endpoint_id` for an id that does not
/// parse, code 10 for a malformed `node.json`, and code 1 if the file cannot
/// be written.
pub fn set_default(ctx: &Context, endpoints: &[String]) -> Result<Outcome> {
    let mut witnesses = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let endpoint = ids::parse_endpoint(endpoint)?;
        if !witnesses.contains(&endpoint) {
            witnesses.push(endpoint);
        }
    }

    let mut config = ctx.home().config()?;
    config.witnesses.clone_from(&witnesses);
    ctx.home().write_config(&config)?;

    let document = DefaultWitnesses {
        witnesses: witnesses.iter().map(ids::key).collect(),
    };
    let mut text = format!(
        "node.json now names {} default {}",
        document.witnesses.len(),
        if document.witnesses.len() == 1 {
            "witness"
        } else {
            "witnesses"
        }
    );
    for witness in &document.witnesses {
        text.push('\n');
        text.push_str(witness.as_str());
    }
    text.push_str("\na running node reads node.json at startup; restart it to apply");
    Outcome::new(&document, text)
}

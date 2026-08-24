//! `mabel witness add` and `mabel witness set-default`.
//!
//! Two different sets, and only one of them is signed. A `WitnessSet` replaces
//! the whole set a ledger records, so adding one witness means signing the set
//! the ledger already holds plus the new one, and every entry is an identity id
//! (proposal 006 section 1). `node.json.witnesses` is this node's own
//! configuration: the endpoints it queries for any ledger, third in the
//! crawler's source order (proposal 003 section 3).

use mabel_core::sign::build_witness_set;
use mabel_node::WitnessEntry;
use mabel_node::graph::Resolution;
use mabel_node::wallet::WalletCore;

use crate::append::{append, ensure_fresh_hinted};
use crate::cli::AppendOptions;
use crate::context::Context;
use crate::documents::{AddedWitness, DefaultWitness, DefaultWitnesses};
use crate::error::{CliError, Result};
use crate::ids;
use crate::render::Outcome;

/// `mabel witness add --identity <alias|id> --witness <alias|id|link>
/// [--endpoints <endpoint,...>]`.
///
/// The witness is named by identity id: a witness that moves machines keeps the
/// same id, so this event stands whatever endpoint answers for it.
///
/// `--endpoints` names the machines to try while this command resolves witness
/// identities, the one on `--witness` included. They are this call's
/// `CallerHint`s (proposal 006 section 5, source 2), which is what makes the
/// freshness query of section 5 reach a witness no local source names an
/// endpoint for. They are used for this call and nothing else: neither
/// `node.json` nor `peers.json` is written, the same rule a link's hints follow
/// (section 5.3). A link on `--witness` carries the same kind of hint, so the
/// flag and a link that names endpoints are refused together.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_endpoint_id` for an endpoint that does
/// not parse and `conflicting_source` when both `--endpoints` and a link on
/// `--witness` name machines, and the errors of the append discipline.
pub fn add(
    ctx: &Context,
    identity: &str,
    witness: &str,
    endpoints: &[String],
    options: &AppendOptions,
) -> Result<Outcome> {
    let identity = ctx.resolve_local_hinted(identity, "--identity")?;
    let (witness, linked) = ctx.resolve_hinted(witness)?;
    let mut given = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let endpoint = ids::parse_endpoint(endpoint)?;
        if !given.contains(&endpoint) {
            given.push(endpoint);
        }
    }
    if !given.is_empty() && !linked.is_empty() {
        return Err(CliError::usage(
            "conflicting_source",
            "--endpoints names machines and the link on --witness names others: give one",
        )
        .with_detail("parameter", "--endpoints"));
    }
    let hints = if given.is_empty() { linked } else { given };
    ensure_fresh_hinted(ctx, identity, options, &hints)?;
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

/// `mabel witness set-default --witness <mabel-id> [--endpoints <endpoint,...>]`.
///
/// Writes `node.json.witnesses`, replacing whatever it held: one witness
/// identity and the raw endpoints that reach it (proposal 006 section 5.4).
/// Nothing is signed and no ledger changes: this is the node's configuration,
/// and a running node reads the file at startup, so a restart is what makes an
/// edit take effect.
///
/// A configured witness with no reachable endpoint is a config entry that does
/// nothing, so the command refuses one: `unresolvable_witness` when neither the
/// command line, a local copy of the witness, `peers.json` nor the entry already
/// on disk names an endpoint for it.
///
/// # Errors
///
/// Returns code 2 with reason `malformed_endpoint_id` for an id that does not
/// parse and `unresolvable_witness` for a witness this home can reach no
/// endpoint for, code 10 for a malformed `node.json`, and code 1 if the file
/// cannot be written.
pub fn set_default(ctx: &Context, witness: &str, endpoints: &[String]) -> Result<Outcome> {
    let witness = ctx.resolve(witness)?;
    let mut given = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let endpoint = ids::parse_endpoint(endpoint)?;
        if !given.contains(&endpoint) {
            given.push(endpoint);
        }
    }

    // What resolution would find for this witness before the file is touched: a
    // local copy's advertisement, a `peers.json` hint or the entry already
    // recorded. Any of them is enough, so an operator who is only correcting a
    // typo does not have to repeat the endpoints.
    let core = WalletCore::new(ctx.home().clone());
    let resolution = Resolution::for_operation().with_caller_hints(given.clone());
    let reachable = resolution.witness_endpoints(&core, witness)?;
    if reachable.is_empty() {
        return Err(CliError::usage(
            "unresolvable_witness",
            format!(
                "no endpoint is known for the witness {witness}: pass --endpoints, or fetch its \
                 ledger first"
            ),
        )
        .with_detail("witness", witness.to_string())
        .with_detail("endpoints_tried", Vec::<String>::new()));
    }

    let mut config = ctx.home().config()?;
    // This identity's entry is replaced, not added to, and any other configured
    // witness is left alone: a fleet is several identities, and setting one
    // should not unconfigure the rest.
    let recorded = if given.is_empty() {
        config.witness_endpoints(witness).to_vec()
    } else {
        given
    };
    let entry = WitnessEntry::new(witness, recorded.clone());
    match config
        .witnesses
        .iter_mut()
        .find(|existing| existing.identity == witness)
    {
        Some(existing) => *existing = entry,
        None => config.witnesses.push(entry),
    }
    ctx.home().write_config(&config)?;

    let document = DefaultWitnesses {
        witnesses: config
            .witnesses
            .iter()
            .map(|entry| DefaultWitness {
                identity_id: ids::identity(entry.identity),
                endpoints: entry.endpoints.iter().map(ids::key).collect(),
            })
            .collect(),
    };
    let mut text = format!("node.json now names {witness} as a default witness");
    for endpoint in recorded.iter().map(ids::key) {
        text.push('\n');
        text.push_str(endpoint.as_str());
    }
    if recorded.is_empty() {
        text.push_str(&format!(
            "\nno endpoint is recorded beside it; resolution reaches it through {}",
            ids::key(&reachable[0])
        ));
    }
    text.push_str("\na running node reads node.json at startup; restart it to apply");
    Outcome::new(&document, text)
}

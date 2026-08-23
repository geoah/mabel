//! `mabel witness add`.
//!
//! A `WitnessConfig` replaces the whole set, so adding one endpoint means
//! signing the set the ledger already records plus the new one (proposal 001
//! section 3.4).

use mabel_core::sign::build_witness_config;

use crate::append::{append, ensure_fresh};
use crate::cli::AppendOptions;
use crate::context::Context;
use crate::documents::AddedWitness;
use crate::error::Result;
use crate::ids;
use crate::render::Outcome;

/// `mabel witness add --identity <alias|id> --endpoint <endpoint id>`.
pub fn add(
    ctx: &Context,
    identity: &str,
    endpoint: &str,
    options: &AppendOptions,
) -> Result<Outcome> {
    let identity = ctx.resolve_local(identity)?;
    let endpoint = ids::parse_endpoint(endpoint)?;
    ensure_fresh(ctx, identity, options)?;
    let mut loaded = ctx.load(identity)?;

    let mut witnesses = loaded.state.witnesses().to_vec();
    if !witnesses.contains(&endpoint) {
        witnesses.push(endpoint);
    }
    let appended = append(ctx, identity, &mut loaded, |signer, at, timestamp_ms| {
        build_witness_config(signer, at, &witnesses, timestamp_ms)
    })?;

    let document = AddedWitness {
        identity_id: ids::identity(identity),
        endpoint: ids::key(&endpoint),
        witnesses: loaded.witnesses(),
        event_id: ids::event(appended.event_id),
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
    };
    let text = format!(
        "{} witnesses {identity} as of seq {}\nthe set now holds {} endpoints",
        document.endpoint,
        appended.seq,
        document.witnesses.len()
    );
    Outcome::new(&document, text)
}

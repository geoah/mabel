//! `mabel graph sync|status` and `mabel lookup`.
//!
//! Synchronizing is manual (decision 016): a sync tells each contacted witness
//! which identities this wallet cares about, so nothing here runs on a timer.
//! The caps of proposal 003 section 3 bound the run, and `truncated_by` says
//! which one stopped it.
//!
//! A lookup answers relative to one local root. `degrees: null` means no path
//! was found **within the caps of this crawl**, which the output states as
//! "no path in this crawl", never as "no relationship".

use mabel_core::IdentityId;
use mabel_node::api::documents::{GraphStatus, GraphView, Lookup};
use mabel_node::graph::{CrawlOptions, Generation, GraphStore, NetLedgerFetcher, crawl};
use mabel_node::now_ms;
use mabel_node::wallet::{Names, WalletCore, default_root, graph_status, lookup_document};

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::on_network;
use crate::render::Outcome;

/// `mabel graph sync [--depth N] [--peer <ticket>]`.
pub fn sync(ctx: &Context, depth: Option<u32>, peers: &[String]) -> Result<Outcome> {
    let roots = ctx.home().identities()?;
    if roots.is_empty() {
        return Err(CliError::usage(
            "no_local_identity",
            "this home holds no identity to crawl from",
        ));
    }
    let generation = on_network(ctx, peers, |core, sync| async move {
        let fetcher = NetLedgerFetcher::new(core, sync);
        let mut options = CrawlOptions::new();
        if let Some(depth) = depth {
            options = options.with_depth(depth);
        }
        Ok(crawl(&roots, &options, &fetcher).await)
    })?;
    GraphStore::in_home(ctx.home()).publish(&generation)?;

    let core = WalletCore::new(ctx.home().clone());
    let graph = graph_status(
        &Names::new(&core, Some(&generation)),
        &generation.summary,
        now_ms(),
    );
    let text = summary_text(&graph);
    Outcome::new(&GraphView { graph: Some(graph) }, text)
}

/// `mabel graph status`.
pub fn status(ctx: &Context) -> Result<Outcome> {
    let generation = current(ctx)?;
    let core = WalletCore::new(ctx.home().clone());
    let graph = generation.as_ref().map(|generation| {
        graph_status(
            &Names::new(&core, Some(generation)),
            &generation.summary,
            now_ms(),
        )
    });
    let text = graph.as_ref().map_or_else(
        || "no crawl has run in this home; run mabel graph sync".to_owned(),
        summary_text,
    );
    Outcome::new(&GraphView { graph }, text)
}

/// `mabel lookup <identity> [--from <alias|id>]`.
pub fn lookup(ctx: &Context, name: &str, from: Option<&str>) -> Result<Outcome> {
    let target = ctx.resolve(name)?;
    let core = WalletCore::new(ctx.home().clone());
    let from = match from {
        Some(from) => root(ctx, from)?,
        None => default_root(&core)?,
    };
    let generation = current(ctx)?;
    let document = lookup_document(&core, generation.as_ref(), from, target, now_ms())?;
    let text = lookup_text(&document);
    Outcome::new(&document, text)
}

/// The live generation, `None` when no crawl has run in this home.
fn current(ctx: &Context) -> Result<Option<Generation>> {
    Ok(GraphStore::in_home(ctx.home()).current_generation()?)
}

/// A `--from` that must name an identity this home holds.
fn root(ctx: &Context, name: &str) -> Result<IdentityId> {
    let from = ctx.resolve(name)?;
    if ctx.home().identity_dir(from).is_dir() {
        return Ok(from);
    }
    Err(CliError::usage(
        "unknown_from_identity",
        format!("no identity here is named {name}"),
    )
    .with_detail("parameter", "from")
    .with_detail("value", from.to_string()))
}

fn summary_text(graph: &GraphStatus) -> String {
    let mut text = format!(
        "crawl {} at depth {}: {} nodes, {} edges, {} fetches",
        graph.sync_id, graph.depth, graph.node_count, graph.edge_count, graph.fetch_count
    );
    match graph.truncated_by {
        Some(cap) => text.push_str(&format!(
            "\ntruncated by {}: the graph is what this crawl reached, not the whole graph",
            cap.as_str()
        )),
        None => text.push_str("\nno cap was reached"),
    }
    if graph.stale {
        text.push_str("\nthis crawl is over 24 hours old");
    }
    for identity in &graph.equivocations {
        text.push_str(&format!(
            "\ntwo sources disagree about {}",
            ids::shown(identity)
        ));
    }
    text
}

fn lookup_text(document: &Lookup) -> String {
    let target = &document.identity;
    let mut text = format!(
        "{} ({})\nfrom {} ({})",
        label(target.display_name.as_deref(), target.alias.as_deref()),
        ids::shown(&target.identity_id),
        label(
            document.from.display_name.as_deref(),
            document.from.alias.as_deref()
        ),
        ids::shown(&document.from.identity_id)
    );
    match document.degrees {
        Some(degrees) => text.push_str(&format!("\n{degrees} degrees in this crawl")),
        None => text.push_str("\nno path in this crawl, which is not the same as no relationship"),
    }
    for path in &document.paths {
        for hop in &path.hops {
            // The two identities carry the prefix; the attestation is an event
            // id and does not.
            text.push_str(&format!(
                "\n  {} trusts {} ({})",
                ids::shown(&hop.from.identity_id),
                ids::shown(&hop.to.identity_id),
                hop.attestation_event
            ));
            if hop.stale {
                text.push_str(" [stale]");
            }
            if hop.equivocation.is_some() {
                text.push_str(" [two sources disagree]");
            }
        }
    }
    text.push_str(&format!(
        "\n{} attestations out, {} in (best effort: who this crawl read)",
        document.trust.len(),
        document.reverse.entries.len()
    ));
    if document.graph_stale {
        text.push_str("\nthe crawl behind this answer is over 24 hours old or has never run");
    }
    if let Some(cap) = document.truncated_by {
        text.push_str(&format!("\nthe crawl was truncated by {}", cap.as_str()));
    }
    text
}

/// The one-line label a person reads, id always printed beside it.
fn label(display_name: Option<&str>, alias: Option<&str>) -> String {
    display_name
        .or(alias)
        .map_or_else(|| "(no name)".to_owned(), ToOwned::to_owned)
}

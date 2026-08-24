//! The breadth-first walk and its caps (proposal 003 section 3).
//!
//! Every local identity is a root at depth 0, ties are broken by ascending
//! identity id, and four caps bound the run: the depth it was given, 500
//! nodes, 300 fetches and 60 seconds, with at most 8 fetches in flight per
//! level. Caps first, completeness never: node count bounds the graph, the
//! clock bounds a slow crawl and the fetch count bounds a fast one. A
//! truncated crawl is deterministic, because the level is walked in ascending
//! id order and a cap cuts its tail.
//!
//! The walk writes nothing. It hands back a [`Generation`], and
//! [`crate::graph::GraphStore`] is what puts it on disk.

use std::collections::{BTreeMap, BTreeSet};
use std::future::poll_fn;
use std::task::Poll;
use std::time::Duration;

use iroh::EndpointId;
use mabel_core::IdentityId;

use crate::api::documents::Id;
use crate::graph::fetcher::{FetchFuture, FetchOutcome, LedgerFetcher};
use crate::graph::model::{
    DiscoveredVia, GraphEdge, GraphNode, GraphSummary, NodeStatus, RootDepth, TruncatedBy,
};
use crate::graph::resolve::Resolution;
use crate::graph::store::Generation;
use crate::now_ms;
use crate::wallet::ids;

/// Depth when a caller names none.
pub const DEFAULT_DEPTH: u32 = 2;

/// Shallowest crawl a caller may ask for.
pub const MIN_DEPTH: u32 = 1;

/// Deepest crawl a caller may ask for.
pub const MAX_DEPTH: u32 = 4;

/// Nodes one generation holds.
pub const MAX_NODES: usize = 500;

/// Ledgers one run reads.
pub const MAX_FETCHES: usize = 300;

/// Fetches running together within one level.
pub const IN_FLIGHT: usize = 8;

/// How long a whole run may take. Authoritative: the walk stops here whatever
/// the other caps allow.
pub const RUN_BUDGET: Duration = Duration::from_secs(60);

/// The caps and the clock one crawl runs under.
#[derive(Debug, Clone)]
pub struct CrawlOptions {
    /// Levels to walk, bounded to [`MIN_DEPTH`] through [`MAX_DEPTH`] by
    /// [`CrawlOptions::bounded_depth`] whatever is set here.
    pub depth: u32,
    /// Nodes the generation may hold.
    pub max_nodes: usize,
    /// Ledgers the run may read.
    pub max_fetches: usize,
    /// Fetches running together within one level.
    pub in_flight: usize,
    /// How long the whole run may take.
    pub budget: Duration,
    /// The wall-clock start, which names the generation and is what
    /// staleness counts from.
    pub started_at_ms: u64,
    /// The generation name, minted from `started_at_ms` when absent.
    pub sync_id: Option<String>,
    /// Endpoints the caller named for this run, which are source 2 (proposal
    /// 006 section 5).
    pub caller_hints: Vec<EndpointId>,
}

impl CrawlOptions {
    /// The proposal 003 caps, starting now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            depth: DEFAULT_DEPTH,
            max_nodes: MAX_NODES,
            max_fetches: MAX_FETCHES,
            in_flight: IN_FLIGHT,
            budget: RUN_BUDGET,
            started_at_ms: now_ms(),
            sync_id: None,
            caller_hints: Vec::new(),
        }
    }

    /// The same options at `depth`, bounded to 1 through 4.
    #[must_use]
    pub const fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    /// The depth the run uses: what was asked for, held inside 1 through 4.
    ///
    /// A caller that asks for 0 gets 1 and a caller that asks for 9 gets 4,
    /// because a wallet that crawled to depth 9 would be a spider.
    #[must_use]
    pub const fn bounded_depth(&self) -> u32 {
        if self.depth < MIN_DEPTH {
            MIN_DEPTH
        } else if self.depth > MAX_DEPTH {
            MAX_DEPTH
        } else {
            self.depth
        }
    }
}

impl Default for CrawlOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Mints a generation name: the start timestamp, zero-padded so names sort by
/// age, and a random suffix so two crawls in one millisecond do not collide.
///
/// Falls back to a counter-free suffix of zeroes if the system has no
/// randomness, which costs a collision at worst and never fails a sync.
#[must_use]
pub fn mint_sync_id(started_at_ms: u64) -> String {
    let mut bytes = [0u8; 5];
    if getrandom::fill(&mut bytes).is_err() {
        tracing::warn!("no system randomness for the sync id suffix");
    }
    let suffix = data_encoding::BASE32_NOPAD
        .encode(&bytes)
        .to_ascii_lowercase();
    format!("{started_at_ms:013}-{suffix}")
}

/// Crawls outward from `roots` and returns the generation it saw.
///
/// Never fails: an unreachable ledger, a chain that does not verify and two
/// sources that disagree are all recorded on the node and the walk carries
/// on. What a caller checks is `summary.truncated` and the per-node status.
///
/// One [`Resolution`] carries the run's deadline, its 16-endpoint dial budget
/// and its visited-identity set, so every fetch of this run shares them
/// (proposal 006 section 5.2).
pub async fn crawl(
    roots: &[IdentityId],
    options: &CrawlOptions,
    fetcher: &dyn LedgerFetcher,
) -> Generation {
    let resolution =
        Resolution::with_budget(options.budget).with_caller_hints(options.caller_hints.clone());
    crawl_with(roots, options, fetcher, &resolution).await
}

/// [`crawl`] over a resolution the caller already holds, which is what a route
/// that resolved a link's endpoint hints has.
pub async fn crawl_with(
    roots: &[IdentityId],
    options: &CrawlOptions,
    fetcher: &dyn LedgerFetcher,
    resolution: &Resolution,
) -> Generation {
    let depth_cap = options.bounded_depth();
    let root_set: BTreeSet<IdentityId> = roots.iter().copied().collect();

    let mut outcomes: BTreeMap<IdentityId, (u32, FetchOutcome)> = BTreeMap::new();
    let mut provenance: BTreeMap<IdentityId, DiscoveredVia> = BTreeMap::new();
    let mut hints: BTreeMap<IdentityId, Vec<EndpointId>> = BTreeMap::new();
    let mut frontier: Vec<IdentityId> = root_set.iter().copied().collect();
    let mut fetch_count = 0usize;
    let mut hard_stop: Option<TruncatedBy> = None;
    let mut depth_cut = false;

    'levels: for depth in 0..=depth_cap {
        if frontier.is_empty() {
            break;
        }
        let mut level = std::mem::take(&mut frontier);
        // The level is already ascending, so a cap keeps the lowest ids and
        // two runs over one graph cut at the same place.
        if outcomes.len() + level.len() > options.max_nodes {
            level.truncate(options.max_nodes.saturating_sub(outcomes.len()));
            hard_stop.get_or_insert(TruncatedBy::Nodes);
        }
        if fetch_count + level.len() > options.max_fetches {
            level.truncate(options.max_fetches.saturating_sub(fetch_count));
            hard_stop.get_or_insert(TruncatedBy::Fetches);
        }
        let stop_after_level = hard_stop.is_some();

        for batch in level.chunks(options.in_flight.max(1)) {
            if resolution.expired() {
                hard_stop.get_or_insert(TruncatedBy::Time);
                break 'levels;
            }
            let futures: Vec<FetchFuture<'_>> = batch
                .iter()
                .map(|ledger| {
                    let learned = hints.get(ledger).cloned().unwrap_or_default();
                    fetcher.fetch_candidate(*ledger, learned, resolution)
                })
                .collect();
            fetch_count += futures.len();
            for outcome in join_all(futures).await {
                outcomes.insert(outcome.ledger, (depth, outcome));
            }
        }

        // The next level is every subject this one attests to that has not
        // been read yet, ascending.
        let mut next: BTreeSet<IdentityId> = BTreeSet::new();
        for ledger in &level {
            let Some((_, outcome)) = outcomes.get(ledger) else {
                continue;
            };
            let source = outcome
                .source
                .as_ref()
                .and_then(crate::graph::model::FetchSource::endpoint)
                .and_then(|endpoint| ids::parse_endpoint(endpoint).ok());
            let Some(summary) = outcome.summary.as_ref() else {
                continue;
            };
            for edge in &summary.trust {
                if outcomes.contains_key(&edge.subject) {
                    continue;
                }
                if depth >= depth_cap {
                    depth_cut = true;
                    continue;
                }
                provenance
                    .entry(edge.subject)
                    .or_insert_with(|| DiscoveredVia {
                        identity: ids::identity(*ledger),
                        attestation_event: ids::event(edge.attestation_event),
                    });
                if let Some(source) = source {
                    // Where the attesting ledger was served is a plausible
                    // place to find the ledger it names: a hint, verified
                    // like every other source and authorizing nothing.
                    let learned = hints.entry(edge.subject).or_default();
                    if !learned.contains(&source) {
                        learned.push(source);
                    }
                }
                next.insert(edge.subject);
            }
        }
        if stop_after_level {
            break;
        }
        frontier = next.into_iter().collect();
    }

    let nodes = build_nodes(&outcomes, &provenance, &root_set);
    let summary = build_summary(options, depth_cap, &root_set, &nodes, fetch_count, {
        match hard_stop {
            Some(reason) => Some(reason),
            None if depth_cut => Some(TruncatedBy::Depth),
            None => None,
        }
    });
    Generation { summary, nodes }
}

/// Turns the outcomes into node documents and fills in root provenance.
fn build_nodes(
    outcomes: &BTreeMap<IdentityId, (u32, FetchOutcome)>,
    provenance: &BTreeMap<IdentityId, DiscoveredVia>,
    roots: &BTreeSet<IdentityId>,
) -> BTreeMap<Id, GraphNode> {
    let mut nodes: BTreeMap<Id, GraphNode> = BTreeMap::new();
    for (ledger, (depth, outcome)) in outcomes {
        let summary = outcome.summary.as_ref();
        let edges: Vec<GraphEdge> = summary
            .map(|summary| {
                summary
                    .trust
                    .iter()
                    .map(|edge| GraphEdge {
                        subject: ids::identity(edge.subject),
                        attestation_event: ids::event(edge.attestation_event),
                        seq: edge.seq,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let identity_id = ids::identity(*ledger);
        nodes.insert(
            identity_id.clone(),
            GraphNode {
                identity_id,
                declared_kind: summary.map(|summary| summary.declared_kind),
                display_name: summary.and_then(|summary| summary.display_name.clone()),
                hostname: summary.and_then(|summary| summary.hostname.clone()),
                email: summary.and_then(|summary| summary.email.clone()),
                head_seq: summary.map(|summary| summary.head_seq),
                head_event: summary.map(|summary| ids::event(summary.head_event)),
                depth: *depth,
                roots: Vec::new(),
                discovered_via: provenance.get(ledger).cloned(),
                source: outcome.source.clone(),
                fetched_at_ms: matches!(outcome.status, NodeStatus::Ok | NodeStatus::Equivocation)
                    .then_some(outcome.fetched_at_ms),
                status: outcome.status,
                equivocation: outcome.equivocation.clone(),
                detail: outcome.detail.clone(),
                edges,
            },
        );
    }
    fill_roots(&mut nodes, roots);
    nodes
}

/// Records, on every node, which local roots reach it and at what distance.
///
/// One breadth-first pass per root over the edges the generation stored, so
/// the answer is about the graph on disk rather than about the order the
/// crawl happened to visit things in.
fn fill_roots(nodes: &mut BTreeMap<Id, GraphNode>, roots: &BTreeSet<IdentityId>) {
    let mut reach: BTreeMap<Id, Vec<RootDepth>> = BTreeMap::new();
    for root in roots {
        let root_id = ids::identity(*root);
        if !nodes.contains_key(&root_id) {
            continue;
        }
        for (identity, depth) in distances(nodes, &root_id) {
            reach.entry(identity).or_default().push(RootDepth {
                root: root_id.clone(),
                depth,
            });
        }
    }
    for (identity, mut found) in reach {
        found.sort_by(|left, right| left.root.cmp(&right.root));
        if let Some(node) = nodes.get_mut(&identity) {
            node.roots = found;
        }
    }
}

/// Edges from `from` to every node it reaches, breadth-first.
fn distances(nodes: &BTreeMap<Id, GraphNode>, from: &Id) -> BTreeMap<Id, u32> {
    let mut seen: BTreeMap<Id, u32> = BTreeMap::new();
    seen.insert(from.clone(), 0);
    let mut frontier = vec![from.clone()];
    let mut depth = 0;
    while !frontier.is_empty() {
        depth += 1;
        let mut next = Vec::new();
        for identity in frontier {
            let Some(node) = nodes.get(&identity) else {
                continue;
            };
            for edge in &node.edges {
                if !nodes.contains_key(&edge.subject) || seen.contains_key(&edge.subject) {
                    continue;
                }
                seen.insert(edge.subject.clone(), depth);
                next.push(edge.subject.clone());
            }
        }
        frontier = next;
    }
    seen
}

fn build_summary(
    options: &CrawlOptions,
    depth: u32,
    roots: &BTreeSet<IdentityId>,
    nodes: &BTreeMap<Id, GraphNode>,
    fetch_count: usize,
    truncated_by: Option<TruncatedBy>,
) -> GraphSummary {
    GraphSummary {
        sync_id: options
            .sync_id
            .clone()
            .unwrap_or_else(|| mint_sync_id(options.started_at_ms)),
        last_sync_ms: options.started_at_ms,
        depth,
        roots: roots.iter().map(|root| ids::identity(*root)).collect(),
        node_count: nodes.len() as u64,
        edge_count: nodes.values().map(|node| node.edges.len() as u64).sum(),
        fetch_count: fetch_count as u64,
        truncated: truncated_by.is_some(),
        truncated_by,
        equivocations: nodes
            .values()
            .filter(|node| node.equivocation.is_some())
            .map(|node| node.identity_id.clone())
            .collect(),
    }
}

/// Drives every future in `futures` together and returns their outcomes in
/// the order they were given.
///
/// A hand-rolled join keeps the crawler free of a futures dependency, and a
/// batch is at most [`IN_FLIGHT`] wide, so polling all of them on every wake
/// costs nothing worth measuring.
async fn join_all(futures: Vec<FetchFuture<'_>>) -> Vec<FetchOutcome> {
    let mut pending: Vec<Option<FetchFuture<'_>>> = futures.into_iter().map(Some).collect();
    let mut done: Vec<Option<FetchOutcome>> = pending.iter().map(|_| None).collect();
    poll_fn(move |context| {
        let mut ready = true;
        for (slot, future) in done.iter_mut().zip(pending.iter_mut()) {
            let Some(running) = future else {
                continue;
            };
            match running.as_mut().poll(context) {
                Poll::Ready(outcome) => {
                    *slot = Some(outcome);
                    *future = None;
                }
                Poll::Pending => ready = false,
            }
        }
        if !ready {
            return Poll::Pending;
        }
        Poll::Ready(
            done.iter_mut()
                .filter_map(std::option::Option::take)
                .collect(),
        )
    })
    .await
}

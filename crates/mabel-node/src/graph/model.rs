//! What a crawl writes down: one document per node, one summary per
//! generation (proposal 003 section 3).
//!
//! Every id is rendered the way `contracts/README.md` renders ids, so the
//! files a crawl leaves under `graph/` and the documents the lookup route
//! serves are the same shapes. Nothing here decides anything; the types are
//! records of what one crawl saw, each carrying when it saw it.

use serde::{Deserialize, Serialize};

use crate::api::documents::{DeclaredKind, Id};

/// How long a fetch or a whole crawl stays fresh: 24 hours (decision 016).
pub const STALE_AFTER_MS: u64 = 24 * 60 * 60 * 1000;

/// Whether `at_ms` is more than [`STALE_AFTER_MS`] behind `now_ms`.
///
/// A timestamp in the future is fresh, not stale: a clock that ran backwards
/// is not evidence that the data aged.
#[must_use]
pub const fn is_stale(at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(at_ms) > STALE_AFTER_MS
}

/// Which class of the dial budget a source spends from (proposal 006 section
/// 5.2).
///
/// Sources 5, 6 and 7 share one class because a chain full of witnesses cannot
/// be allowed to starve source 4, and they stay three separate sources because
/// their provenance differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceClass {
    /// A local read, which dials nothing.
    Local,
    /// Source 2.
    CallerHint,
    /// Source 3.
    PeerHint,
    /// Source 4, the class with reserved slots.
    NodeWitness,
    /// Sources 5, 6 and 7 together.
    ChainNamed,
    /// Source 8.
    Dns,
}

impl SourceClass {
    /// Endpoints of this class one operation may dial (proposal 006 section
    /// 5.2), and [`usize::MAX`] for the class that dials nothing.
    #[must_use]
    pub const fn cap(self) -> usize {
        match self {
            Self::Local => usize::MAX,
            Self::CallerHint | Self::PeerHint | Self::Dns => 4,
            Self::NodeWitness | Self::ChainNamed => 8,
        }
    }

    /// Slots of the 16 that only this class may take.
    ///
    /// Four for [`SourceClass::NodeWitness`] and none for anything else: a
    /// ledger naming 16 witnesses would otherwise spend the whole budget before
    /// the node's own configured witnesses, which are the endpoints most likely
    /// to answer, get a single dial.
    #[must_use]
    pub const fn reserved(self) -> usize {
        match self {
            Self::NodeWitness => 4,
            _ => 0,
        }
    }
}

/// Where a copy of a ledger came from, in the order proposal 006 section 5
/// queries them: cheapest first, then most authoritative, then most leaky.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FetchSource {
    /// Source 1: a copy already under `ledgers/` in this node home.
    Local,
    /// Source 2: an endpoint supplied with this request, from a `mabel://`
    /// link, a `--peer` ticket or `--from`. A human just named it.
    ///
    /// Never written to `peers.json`: an endpoint that arrived in a link or on
    /// a command line served the operation it came with and nothing more
    /// (proposal 006 section 5.3).
    CallerHint {
        /// The endpoint that was asked.
        endpoint: Id,
    },
    /// Source 3: an endpoint `peers.json` records for this ledger, or one the
    /// crawl learned while walking.
    PeerHint {
        /// The endpoint that was asked.
        endpoint: Id,
    },
    /// Source 4: an endpoint of a witness identity `node.json` configures,
    /// resolved by proposal 006 section 5.1.
    NodeWitness {
        /// The witness identity the endpoint answers for.
        witness: Id,
        /// The endpoint that was asked.
        endpoint: Id,
    },
    /// Source 5: an endpoint the ledger's own tag-18 `EndpointAdvertisement`
    /// names, reachable only once another source produced a copy.
    LedgerEndpoint {
        /// The endpoint that was asked.
        endpoint: Id,
    },
    /// Source 6: an endpoint of an identity the ledger's tag-19 `WitnessSet`
    /// names, resolved by proposal 006 section 5.1. Also needs a copy.
    WitnessIdentity {
        /// The witness identity the endpoint answers for.
        witness: Id,
        /// The endpoint that was asked.
        endpoint: Id,
    },
    /// Source 7: an endpoint in the ledger's retired tag-11 `WitnessConfig`.
    ///
    /// A list of raw endpoints written before proposal 006 existed, under a
    /// field that never promised an identity. An endpoint reached this way is
    /// never merged into a tag-18 advertisement, never establishes a binding
    /// under section 4.2 and never reports as `verified`. It counts against the
    /// chain-named budget so that a chain full of legacy hints cannot starve
    /// source 4.
    LegacyWitnessHint {
        /// The endpoint that was asked.
        endpoint: Id,
    },
    /// Source 8: an endpoint a hostname's `mabel-endpoints=` records name,
    /// queried only when sources 1 to 7 produced no reachable copy.
    DnsEndpoint {
        /// The hostname that was queried.
        hostname: String,
        /// The endpoint that was asked.
        endpoint: Id,
    },
}

impl FetchSource {
    /// The endpoint asked, absent for a local read.
    #[must_use]
    pub const fn endpoint(&self) -> Option<&Id> {
        match self {
            Self::Local => None,
            Self::CallerHint { endpoint }
            | Self::PeerHint { endpoint }
            | Self::NodeWitness { endpoint, .. }
            | Self::LedgerEndpoint { endpoint }
            | Self::WitnessIdentity { endpoint, .. }
            | Self::LegacyWitnessHint { endpoint }
            | Self::DnsEndpoint { endpoint, .. } => Some(endpoint),
        }
    }

    /// The witness identity this source answers for, absent for the five
    /// sources that name no identity.
    #[must_use]
    pub const fn witness(&self) -> Option<&Id> {
        match self {
            Self::NodeWitness { witness, .. } | Self::WitnessIdentity { witness, .. } => {
                Some(witness)
            }
            _ => None,
        }
    }

    /// Its position in the source order, 1 through 8.
    #[must_use]
    pub const fn order(&self) -> u8 {
        match self {
            Self::Local => 1,
            Self::CallerHint { .. } => 2,
            Self::PeerHint { .. } => 3,
            Self::NodeWitness { .. } => 4,
            Self::LedgerEndpoint { .. } => 5,
            Self::WitnessIdentity { .. } => 6,
            Self::LegacyWitnessHint { .. } => 7,
            Self::DnsEndpoint { .. } => 8,
        }
    }

    /// The budget class this source spends from.
    #[must_use]
    pub const fn class(&self) -> SourceClass {
        match self {
            Self::Local => SourceClass::Local,
            Self::CallerHint { .. } => SourceClass::CallerHint,
            Self::PeerHint { .. } => SourceClass::PeerHint,
            Self::NodeWitness { .. } => SourceClass::NodeWitness,
            Self::LedgerEndpoint { .. }
            | Self::WitnessIdentity { .. }
            | Self::LegacyWitnessHint { .. } => SourceClass::ChainNamed,
            Self::DnsEndpoint { .. } => SourceClass::Dns,
        }
    }

    /// Whether a copy this source served may establish an endpoint binding
    /// (proposal 006 section 4.2).
    ///
    /// False for source 7 alone. The tag-11 list never promised an identity, so
    /// an endpoint reached through it stays `hinted` however clean the chain it
    /// served folds.
    #[must_use]
    pub const fn may_bind(&self) -> bool {
        !matches!(self, Self::LegacyWitnessHint { .. })
    }

    /// Whether this endpoint may be written back to `peers.json` (proposal 006
    /// section 5.3).
    ///
    /// False for source 2 alone: writing a caller's endpoint back would let
    /// anyone whose link reaches a paste into the search box install a durable
    /// dial target for an identity they do not control.
    #[must_use]
    pub const fn may_record_hint(&self) -> bool {
        !matches!(self, Self::Local | Self::CallerHint { .. })
    }
}

/// What happened when the crawl tried to read one ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// A source served a chain that folded with no violation.
    Ok,
    /// No source answered.
    Unreachable,
    /// A source answered with a chain that does not verify, and none served
    /// one that does.
    Invalid,
    /// Two sources served valid chains that diverge at a sequence. The node
    /// carries the first copy in source order so the walk can continue, and
    /// [`GraphNode::equivocation`] names both branches; nothing here picks a
    /// branch as the true one (proposal 001 section 3.7).
    Equivocation,
}

/// One side of an equivocation: the source, and the event it holds where the
/// two chains disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquivocationBranch {
    /// The source that served this branch.
    pub source: FetchSource,
    /// The event it holds at the divergent sequence.
    pub event: Id,
}

/// Two valid chains for one ledger that disagree at a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equivocation {
    /// The first sequence where the two chains hold different events.
    pub at_seq: u64,
    /// Both branches, in source order. Always two entries.
    pub branches: Vec<EquivocationBranch>,
}

/// One outgoing trust attestation, as the signing ledger recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphEdge {
    /// The identity the attestation names.
    pub subject: Id,
    /// The `TrustAttestation` event.
    pub attestation_event: Id,
    /// Its position in the signing ledger.
    pub seq: u64,
}

/// One local root and how far this node is from it, over the edges this
/// generation stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootDepth {
    /// The local identity the crawl started from.
    pub root: Id,
    /// Edges from that root to this node.
    pub depth: u32,
}

/// The attestation that put a node in the frontier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveredVia {
    /// The ledger whose attestation named this node.
    pub identity: Id,
    /// That attestation's event.
    pub attestation_event: Id,
}

/// One identity as one crawl saw it: `graph/generations/<sync_id>/nodes/<identity_id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphNode {
    /// The identity this file describes.
    pub identity_id: Id,
    /// What its inception declares, absent when no source served a chain.
    pub declared_kind: Option<DeclaredKind>,
    /// The name its profile publishes, absent when it carries none.
    pub display_name: Option<String>,
    /// The hostname its profile claims, unverified here (proposal 003
    /// section 2 does the DNS check).
    pub hostname: Option<String>,
    /// The email its profile publishes, absent when it carries none. Defaulted
    /// on read, so a generation written before proposal 005 still parses.
    #[serde(default)]
    pub email: Option<String>,
    /// The last position of the chain that was read.
    pub head_seq: Option<u64>,
    /// The event at that position.
    pub head_event: Option<Id>,
    /// Edges from the nearest local root.
    pub depth: u32,
    /// Every local root that reaches this node in this generation, ascending
    /// by root id.
    pub roots: Vec<RootDepth>,
    /// The attestation that named it, absent on a root.
    pub discovered_via: Option<DiscoveredVia>,
    /// The source that served the copy this node was folded from.
    pub source: Option<FetchSource>,
    /// When that source answered.
    pub fetched_at_ms: Option<u64>,
    /// How the read ended.
    pub status: NodeStatus,
    /// Both branches when two sources diverged.
    pub equivocation: Option<Equivocation>,
    /// One sentence about a failed read, for a person looking at the file.
    pub detail: Option<String>,
    /// The attestations this ledger currently makes, ascending by position.
    /// A revoked attestation is not an edge.
    pub edges: Vec<GraphEdge>,
}

impl GraphNode {
    /// Whether this node was fetched more than 24 hours before `now_ms`.
    ///
    /// A node no source served has no fetch time and is stale.
    #[must_use]
    pub const fn stale(&self, now_ms: u64) -> bool {
        match self.fetched_at_ms {
            Some(fetched_at_ms) => is_stale(fetched_at_ms, now_ms),
            None => true,
        }
    }

    /// The edge to `subject`, if this node attests to it.
    #[must_use]
    pub fn edge_to(&self, subject: &Id) -> Option<&GraphEdge> {
        self.edges.iter().find(|edge| &edge.subject == subject)
    }
}

/// Why a crawl stopped short of the graph it could have walked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncatedBy {
    /// Attestations pointed past the depth the run was given.
    Depth,
    /// The node cap was reached.
    Nodes,
    /// The fetch cap was reached.
    Fetches,
    /// The whole-run clock ran out.
    Time,
}

impl TruncatedBy {
    /// The JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Depth => "depth",
            Self::Nodes => "nodes",
            Self::Fetches => "fetches",
            Self::Time => "time",
        }
    }
}

/// `graph/generations/<sync_id>/summary.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphSummary {
    /// The generation this summary belongs to: the start timestamp and a
    /// random suffix.
    pub sync_id: String,
    /// When the crawl started, which is what staleness counts from.
    pub last_sync_ms: u64,
    /// The depth the run used, after the 1 to 4 bound.
    pub depth: u32,
    /// The local identities the crawl started from, ascending.
    pub roots: Vec<Id>,
    /// Nodes in the generation.
    pub node_count: u64,
    /// Edges over all nodes.
    pub edge_count: u64,
    /// Ledgers the run asked a fetcher for.
    pub fetch_count: u64,
    /// Whether a cap stopped the walk.
    pub truncated: bool,
    /// Which cap, absent when nothing was cut.
    pub truncated_by: Option<TruncatedBy>,
    /// Every identity whose sources disagreed, ascending.
    pub equivocations: Vec<Id>,
}

impl GraphSummary {
    /// Whether the crawl ran more than 24 hours before `now_ms`.
    #[must_use]
    pub const fn stale(&self, now_ms: u64) -> bool {
        is_stale(self.last_sync_ms, now_ms)
    }
}

/// One step of a path: the ledger that attested, the identity it named, and
/// the attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathHop {
    /// The ledger that signed the attestation.
    pub from: Id,
    /// The identity it names.
    pub to: Id,
    /// The attestation event.
    pub attestation_event: Id,
}

/// One path from a root to a target, shortest in this generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphPath {
    /// The hops in order, from the root outward.
    pub hops: Vec<PathHop>,
}

impl GraphPath {
    /// Edges in the path, which is the degrees of separation it reports.
    #[must_use]
    pub const fn degrees(&self) -> usize {
        self.hops.len()
    }
}

/// One identity in this crawl that attests to the subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverseEdge {
    /// The ledger that signed the attestation.
    pub identity: Id,
    /// The attestation event.
    pub attestation_event: Id,
    /// Its position in that ledger.
    pub seq: u64,
}

/// Who, in this crawl, trusts one identity.
///
/// Always labelled: the answer is who this node happened to fetch, never who
/// trusts the subject in the world (proposal 003 section 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverseEdges {
    /// Always `true`.
    pub best_effort: bool,
    /// The attesting identities, ascending by id.
    pub entries: Vec<ReverseEdge>,
}

impl ReverseEdges {
    /// The labelled answer for `entries`.
    #[must_use]
    pub const fn new(entries: Vec<ReverseEdge>) -> Self {
        Self {
            best_effort: true,
            entries,
        }
    }
}

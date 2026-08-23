//! `graph/`: one directory per crawl and a pointer at the live one
//! (proposal 003 section 3).
//!
//! ```text
//! graph/current.json                                    sync_id of the live generation
//! graph/generations/<sync_id>/summary.json              counts, caps hit, roots
//! graph/generations/<sync_id>/nodes/<identity_id>.json  one identity as the crawl saw it
//! ```
//!
//! A sync writes a whole generation, then replaces `graph/current.json` with
//! one rename. A reader resolves the pointer once and reads only the
//! generation it names, so no lookup ever sees half a crawl. Older
//! generations are caches and are collected down to the last two.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::documents::Id;
use crate::atomic::{DATA_MODE, create_dir, sync_dir, write_atomic};
use crate::error::{Result, StorageError, io_at, json_at};
use crate::graph::model::{GraphNode, GraphPath, GraphSummary, PathHop, ReverseEdge, ReverseEdges};
use crate::home::NodeHome;

/// Directory under the node home that holds every generation.
pub const GRAPH_DIR: &str = "graph";

/// Name of the pointer file.
pub const CURRENT_FILE: &str = "current.json";

/// Directory holding one subdirectory per generation.
pub const GENERATIONS_DIR: &str = "generations";

/// Directory of node files inside a generation.
pub const NODES_DIR: &str = "nodes";

/// Name of the per-generation summary.
pub const SUMMARY_FILE: &str = "summary.json";

/// How many generations survive a sync, the live one included.
pub const KEPT_GENERATIONS: usize = 2;

/// How many shortest paths a lookup renders (proposal 003 section 3).
pub const MAX_PATHS: usize = 3;

/// `graph/current.json`: which generation is live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentPointer {
    /// The generation readers resolve to.
    pub sync_id: String,
    /// When the pointer was swapped.
    pub written_at_ms: u64,
}

/// One crawl, in memory: its summary and every node it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    /// The counts and caps of the run.
    pub summary: GraphSummary,
    /// Every node, by identity id.
    pub nodes: BTreeMap<Id, GraphNode>,
}

impl Generation {
    /// The node for `identity`, if this crawl reached it.
    #[must_use]
    pub fn node(&self, identity: &Id) -> Option<&GraphNode> {
        self.nodes.get(identity)
    }

    /// Whether the crawl ran more than 24 hours before `now_ms`.
    #[must_use]
    pub const fn stale(&self, now_ms: u64) -> bool {
        self.summary.stale(now_ms)
    }

    /// Who, in this crawl, attests to `identity`.
    ///
    /// Computed by scanning the generation, which is trivial at 500 nodes,
    /// and always labelled best-effort: it answers who this crawl happened to
    /// read, never who trusts the identity in the world.
    #[must_use]
    pub fn reverse_edges(&self, identity: &Id) -> ReverseEdges {
        let mut entries: Vec<ReverseEdge> = Vec::new();
        for node in self.nodes.values() {
            for edge in node.edges.iter().filter(|edge| &edge.subject == identity) {
                entries.push(ReverseEdge {
                    identity: node.identity_id.clone(),
                    attestation_event: edge.attestation_event.clone(),
                    seq: edge.seq,
                });
            }
        }
        entries.sort_by(|left, right| left.identity.cmp(&right.identity));
        ReverseEdges::new(entries)
    }

    /// Up to [`MAX_PATHS`] shortest paths from `from` to `to`, over the edges
    /// this generation stored.
    ///
    /// Empty means no path was found **within the caps of this crawl**, which
    /// is not the same statement as "no relationship" and must never be
    /// rendered as one.
    #[must_use]
    pub fn paths(&self, from: &Id, to: &Id) -> Vec<GraphPath> {
        self.paths_up_to(from, to, MAX_PATHS)
    }

    /// [`Generation::paths`] with the count of paths chosen by the caller.
    #[must_use]
    pub fn paths_up_to(&self, from: &Id, to: &Id, limit: usize) -> Vec<GraphPath> {
        if limit == 0 || !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
            return Vec::new();
        }
        if from == to {
            return vec![GraphPath { hops: Vec::new() }];
        }
        let remaining = self.distances_to(to);
        let Some(distance) = remaining.get(from).copied() else {
            return Vec::new();
        };
        let mut found = Vec::new();
        self.walk(
            from,
            distance,
            &remaining,
            &mut Vec::new(),
            &mut found,
            limit,
        );
        found
    }

    /// Edges on the shortest path from `from` to `to`, or `None` when this
    /// crawl found none.
    #[must_use]
    pub fn degrees(&self, from: &Id, to: &Id) -> Option<usize> {
        if from == to {
            return self.nodes.contains_key(from).then_some(0);
        }
        self.distances_from(from)
            .remove(to)
            .map(|distance| distance as usize)
    }

    /// Breadth-first distance from `from` to every node it reaches.
    fn distances_from(&self, from: &Id) -> BTreeMap<Id, u32> {
        let mut seen: BTreeMap<Id, u32> = BTreeMap::new();
        seen.insert(from.clone(), 0);
        let mut frontier = vec![from.clone()];
        let mut depth = 0;
        while !frontier.is_empty() {
            depth += 1;
            let mut next = Vec::new();
            for identity in frontier {
                let Some(node) = self.nodes.get(&identity) else {
                    continue;
                };
                for edge in &node.edges {
                    if !self.nodes.contains_key(&edge.subject) || seen.contains_key(&edge.subject) {
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

    /// Breadth-first distance to `to` from every node that reaches it, over
    /// the edges read backwards.
    fn distances_to(&self, to: &Id) -> BTreeMap<Id, u32> {
        let mut incoming: BTreeMap<&Id, Vec<&Id>> = BTreeMap::new();
        for node in self.nodes.values() {
            for edge in &node.edges {
                if self.nodes.contains_key(&edge.subject) {
                    incoming
                        .entry(&edge.subject)
                        .or_default()
                        .push(&node.identity_id);
                }
            }
        }
        let mut seen: BTreeMap<Id, u32> = BTreeMap::new();
        seen.insert(to.clone(), 0);
        let mut frontier = vec![to.clone()];
        let mut depth = 0;
        while !frontier.is_empty() {
            depth += 1;
            let mut next = Vec::new();
            for identity in frontier {
                let Some(sources) = incoming.get(&identity) else {
                    continue;
                };
                for source in sources {
                    if seen.contains_key(*source) {
                        continue;
                    }
                    seen.insert((*source).clone(), depth);
                    next.push((*source).clone());
                }
            }
            frontier = next;
        }
        seen
    }

    /// Collects shortest paths by walking forward, only ever taking an edge
    /// that reduces the distance left to the target, in ascending subject
    /// order so two readers of one generation list the same paths.
    fn walk(
        &self,
        at: &Id,
        distance: u32,
        remaining: &BTreeMap<Id, u32>,
        hops: &mut Vec<PathHop>,
        found: &mut Vec<GraphPath>,
        limit: usize,
    ) {
        if found.len() >= limit {
            return;
        }
        if distance == 0 {
            found.push(GraphPath { hops: hops.clone() });
            return;
        }
        let Some(node) = self.nodes.get(at) else {
            return;
        };
        let mut edges: Vec<_> = node.edges.iter().collect();
        edges.sort_by(|left, right| left.subject.cmp(&right.subject));
        for edge in edges {
            if found.len() >= limit {
                return;
            }
            if remaining.get(&edge.subject) != Some(&(distance - 1)) {
                continue;
            }
            hops.push(PathHop {
                from: at.clone(),
                to: edge.subject.clone(),
                attestation_event: edge.attestation_event.clone(),
            });
            self.walk(&edge.subject, distance - 1, remaining, hops, found, limit);
            hops.pop();
        }
    }
}

/// `graph/` in one node home.
#[derive(Debug, Clone)]
pub struct GraphStore {
    root: PathBuf,
}

impl GraphStore {
    /// The store at `root`, which is the `graph/` directory itself.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The store in `home`.
    #[must_use]
    pub fn in_home(home: &NodeHome) -> Self {
        Self::new(home.root().join(GRAPH_DIR))
    }

    /// `graph/`.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `graph/current.json`.
    #[must_use]
    pub fn current_path(&self) -> PathBuf {
        self.root.join(CURRENT_FILE)
    }

    /// `graph/generations/<sync_id>/`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] for a `sync_id` outside `[a-z0-9-]`,
    /// which is what stops a pointer from naming a path elsewhere on disk.
    pub fn generation_dir(&self, sync_id: &str) -> Result<PathBuf> {
        check_sync_id(sync_id)?;
        Ok(self.root.join(GENERATIONS_DIR).join(sync_id))
    }

    /// Writes a whole generation, without touching the pointer.
    ///
    /// A reader keeps seeing the previous generation until
    /// [`GraphStore::set_current`] renames the pointer over it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if a write fails.
    pub fn write_generation(&self, generation: &Generation) -> Result<()> {
        let dir = self.generation_dir(&generation.summary.sync_id)?;
        let nodes = dir.join(NODES_DIR);
        create_dir(&nodes)?;
        for (identity, node) in &generation.nodes {
            write_json(&nodes.join(format!("{identity}.json")), node)?;
        }
        sync_dir(&nodes)?;
        write_json(&dir.join(SUMMARY_FILE), &generation.summary)?;
        sync_dir(&dir)
    }

    /// Points `graph/current.json` at `sync_id` with one rename.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the write fails and
    /// [`StorageError::Json`] for a malformed `sync_id`.
    pub fn set_current(&self, sync_id: &str, written_at_ms: u64) -> Result<()> {
        check_sync_id(sync_id)?;
        create_dir(&self.root)?;
        write_json(
            &self.current_path(),
            &CurrentPointer {
                sync_id: sync_id.to_owned(),
                written_at_ms,
            },
        )
    }

    /// Writes `generation`, swaps the pointer to it and collects everything
    /// but the last [`KEPT_GENERATIONS`].
    ///
    /// # Errors
    ///
    /// Returns the errors of [`GraphStore::write_generation`] and
    /// [`GraphStore::set_current`]. A generation that cannot be collected is
    /// logged, not failed: the live pointer is already correct.
    pub fn publish(&self, generation: &Generation) -> Result<()> {
        self.write_generation(generation)?;
        self.set_current(&generation.summary.sync_id, generation.summary.last_sync_ms)?;
        self.collect(&generation.summary.sync_id);
        Ok(())
    }

    /// The pointer, or `None` when no crawl has run in this home.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] for a malformed pointer.
    pub fn current(&self) -> Result<Option<CurrentPointer>> {
        let path = self.current_path();
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(json_at(&path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_at(&path)(error)),
        }
    }

    /// The generation the pointer names.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] for a malformed pointer, summary or
    /// node file.
    pub fn current_generation(&self) -> Result<Option<Generation>> {
        let Some(pointer) = self.current()? else {
            return Ok(None);
        };
        self.generation(&pointer.sync_id)
    }

    /// One generation by name, `None` when it has been collected.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] for a malformed summary or node file.
    pub fn generation(&self, sync_id: &str) -> Result<Option<Generation>> {
        let dir = self.generation_dir(sync_id)?;
        let summary_path = dir.join(SUMMARY_FILE);
        let summary: GraphSummary = match fs::read(&summary_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(json_at(&summary_path))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_at(&summary_path)(error)),
        };
        let nodes_dir = dir.join(NODES_DIR);
        let mut nodes = BTreeMap::new();
        let entries = match fs::read_dir(&nodes_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(Generation { summary, nodes }));
            }
            Err(error) => return Err(io_at(&nodes_dir)(error)),
        };
        for entry in entries {
            let path = entry.map_err(io_at(&nodes_dir))?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_at(&path))?;
            let node: GraphNode = serde_json::from_slice(&bytes).map_err(json_at(&path))?;
            nodes.insert(node.identity_id.clone(), node);
        }
        Ok(Some(Generation { summary, nodes }))
    }

    /// Every generation on disk, oldest first.
    ///
    /// Names begin with a zero-padded start timestamp, so the byte order is
    /// the age order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if `graph/generations/` cannot be listed.
    pub fn generation_ids(&self) -> Result<Vec<String>> {
        let dir = self.root.join(GENERATIONS_DIR);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_at(&dir)(error)),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(io_at(&dir))?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if check_sync_id(&name).is_ok() {
                ids.push(name);
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Deletes every generation but the newest [`KEPT_GENERATIONS`] and the
    /// one `keep` names.
    fn collect(&self, keep: &str) {
        let Ok(ids) = self.generation_ids() else {
            return;
        };
        let surviving = ids.len().saturating_sub(KEPT_GENERATIONS);
        for sync_id in ids.iter().take(surviving) {
            if sync_id == keep {
                continue;
            }
            let Ok(dir) = self.generation_dir(sync_id) else {
                continue;
            };
            if let Err(error) = fs::remove_dir_all(&dir) {
                tracing::warn!(sync_id, %error, "could not collect an old graph generation");
            }
        }
    }
}

/// Writes one JSON document, pretty-printed with a trailing newline, through
/// a temp file and a rename.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(json_at(path))?;
    bytes.push(b'\n');
    write_atomic(path, &bytes, DATA_MODE)
}

/// A generation name is a path element and nothing else.
fn check_sync_id(sync_id: &str) -> Result<()> {
    let allowed = !sync_id.is_empty()
        && sync_id.len() <= 64
        && sync_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if allowed {
        return Ok(());
    }
    Err(StorageError::Json {
        path: PathBuf::from(sync_id),
        message: "a sync id is one or more of [a-z0-9-], at most 64 characters".to_owned(),
    })
}

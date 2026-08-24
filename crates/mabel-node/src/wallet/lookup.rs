//! Naming a foreign identity, and answering "how do I know this identity"
//! from the live graph generation (proposal 003 sections 3 and 4).
//!
//! One object renders every foreign identity, [`ResolvedIdentity`], and one
//! place builds it, so no surface can forget that a name is a claim: the id
//! travels beside the name, and `provenance` says which of the three sources
//! the label came from.
//!
//! Two rules run through the lookup. A path is the shortest **in this crawl**,
//! so `degrees: null` means no path was found within the caps, never "no
//! relationship". And the reverse list is who this crawl happened to read,
//! which is why it is labelled `best_effort` every time.
//!
//! [`known_identities`] answers the other direction, "who does this home know",
//! from the same resolver and the same generation, so one name means one thing
//! on both routes.

use std::collections::BTreeSet;

use mabel_core::IdentityId;

use crate::api::documents::{
    GraphStatus, Id, KnownIdentity, Lookup, LookupHop, LookupPath, LookupReverse,
    LookupReverseEdge, LookupTrust, Provenance, ResolvedIdentity, VerificationStatus,
};
use crate::api::error::ServiceError;
use crate::graph::{Generation, GraphNode, GraphSummary, MAX_PATHS};
use crate::wallet::core::WalletCore;
use crate::wallet::error::storage_error;
use crate::wallet::ids;

/// Resolves identity ids to the rows every surface renders.
///
/// The order is proposal 003 section 4: the profile display name, then the
/// local alias or contact nickname, then the id alone. A ledger this home
/// holds answers from its own copy; anything else answers from the crawl.
#[derive(Debug, Clone, Copy)]
pub struct Names<'a> {
    core: &'a WalletCore,
    generation: Option<&'a Generation>,
}

impl<'a> Names<'a> {
    /// A resolver over one home and, when a crawl has run, one generation.
    #[must_use]
    pub const fn new(core: &'a WalletCore, generation: Option<&'a Generation>) -> Self {
        Self { core, generation }
    }

    /// The row for one identity.
    ///
    /// Never fails: a home that cannot be read costs a label, not an answer,
    /// so every unreadable source falls back to the id.
    #[must_use]
    pub fn resolve(&self, identity: IdentityId) -> ResolvedIdentity {
        let identity_id = ids::identity(identity);
        let local = self
            .core
            .holds(identity)
            .unwrap_or(false)
            .then(|| self.core.load(identity).ok())
            .flatten();
        let (display_name, hostname, email) = match local.as_ref() {
            // A ledger this home holds is its own authority, profile or not.
            Some(loaded) => match loaded.profile() {
                Some(profile) => (profile.display_name, profile.hostname, profile.email),
                None => (None, None, None),
            },
            None => match self.node(&identity_id) {
                Some(node) => (
                    node.display_name.clone(),
                    node.hostname.clone(),
                    node.email.clone(),
                ),
                None => (None, None, None),
            },
        };
        let alias = self
            .core
            .home()
            .identity_meta(identity)
            .ok()
            .map(|meta| meta.alias)
            .or_else(|| {
                self.core
                    .contact(identity)
                    .ok()
                    .flatten()
                    .and_then(|contact| contact.nickname)
            });
        let provenance = if display_name.is_some() {
            Provenance::Profile
        } else if alias.is_some() {
            Provenance::Alias
        } else {
            Provenance::None
        };
        ResolvedIdentity {
            identity_id,
            display_name,
            email,
            alias,
            verification_status: self.status(identity, hostname.as_deref()),
            hostname,
            provenance,
        }
    }

    /// The row for an id that has already been parsed once.
    #[must_use]
    pub fn resolve_id(&self, identity_id: &Id) -> ResolvedIdentity {
        match ids::parse_identity(identity_id) {
            Ok(identity) => self.resolve(identity),
            Err(_) => ResolvedIdentity::bare(identity_id.clone()),
        }
    }

    /// The cached verdict on a claimed hostname, cache-only.
    fn status(&self, identity: IdentityId, hostname: Option<&str>) -> VerificationStatus {
        let Some(hostname) = hostname else {
            return VerificationStatus::Unclaimed;
        };
        self.core
            .verification_store()
            .read_bound(identity, hostname)
            .ok()
            .flatten()
            .map_or(VerificationStatus::Unverified, |entry| entry.status)
    }

    fn node(&self, identity_id: &Id) -> Option<&'a GraphNode> {
        self.generation?.node(identity_id)
    }
}

/// The graph object both graph routes return.
#[must_use]
pub fn graph_status(names: &Names<'_>, summary: &GraphSummary, now_ms: u64) -> GraphStatus {
    GraphStatus {
        sync_id: summary.sync_id.clone(),
        last_sync_ms: summary.last_sync_ms,
        depth: summary.depth,
        roots: summary
            .roots
            .iter()
            .map(|root| names.resolve_id(root))
            .collect(),
        node_count: summary.node_count,
        edge_count: summary.edge_count,
        fetch_count: summary.fetch_count,
        truncated: summary.truncated,
        truncated_by: summary.truncated_by,
        equivocations: summary.equivocations.clone(),
        stale: summary.stale(now_ms),
    }
}

/// The answer to `GET /api/lookup/{identity_id}?from=`.
///
/// A generation of `None` is a home where no crawl has run: the answer is the
/// same shape with `degrees: null`, an empty path list and `graph_stale:
/// true`, because "I have not looked" and "I looked and found nothing" are
/// both answers and neither is a 404.
///
/// # Errors
///
/// Returns the storage errors of reading the home while resolving names.
pub fn lookup_document(
    core: &WalletCore,
    generation: Option<&Generation>,
    from: IdentityId,
    target: IdentityId,
    now_ms: u64,
) -> Result<Lookup, ServiceError> {
    let names = Names::new(core, generation);
    let from_id = ids::identity(from);
    let target_id = ids::identity(target);
    let node = generation.and_then(|generation| generation.node(&target_id));

    let paths = generation
        .map(|generation| generation.paths_up_to(&from_id, &target_id, MAX_PATHS))
        .unwrap_or_default();
    let degrees = generation
        .and_then(|generation| generation.degrees(&from_id, &target_id))
        .map(|degrees| degrees as u64);

    let mut hops = Vec::with_capacity(paths.len());
    for path in &paths {
        hops.push(LookupPath {
            hops: path
                .hops
                .iter()
                .map(|hop| {
                    let reached = generation.and_then(|generation| generation.node(&hop.to));
                    LookupHop {
                        from: names.resolve_id(&hop.from),
                        to: names.resolve_id(&hop.to),
                        attestation_event: hop.attestation_event.clone(),
                        fetched_at_ms: reached.and_then(|node| node.fetched_at_ms),
                        stale: reached.is_none_or(|node| node.stale(now_ms)),
                        equivocation: reached.and_then(|node| node.equivocation.clone()),
                    }
                })
                .collect(),
        });
    }

    let trust = node.map_or_else(Vec::new, |node| {
        node.edges
            .iter()
            .map(|edge| LookupTrust {
                subject: names.resolve_id(&edge.subject),
                attestation_event: edge.attestation_event.clone(),
                seq: edge.seq,
            })
            .collect()
    });
    let reverse = generation.map_or_else(
        || LookupReverse {
            best_effort: true,
            entries: Vec::new(),
        },
        |generation| {
            let edges = generation.reverse_edges(&target_id);
            LookupReverse {
                best_effort: edges.best_effort,
                entries: edges
                    .entries
                    .into_iter()
                    .map(|edge| LookupReverseEdge {
                        identity: names.resolve_id(&edge.identity),
                        attestation_event: edge.attestation_event,
                        seq: edge.seq,
                    })
                    .collect(),
            }
        },
    );

    Ok(Lookup {
        identity: names.resolve(target),
        from: names.resolve(from),
        degrees,
        paths: hops,
        trust,
        reverse,
        equivocation: node.and_then(|node| node.equivocation.clone()),
        fetched_at_ms: node.and_then(|node| node.fetched_at_ms),
        stale: node.is_none_or(|node| node.stale(now_ms)),
        sync_id: generation.map(|generation| generation.summary.sync_id.clone()),
        last_sync_ms: generation.map(|generation| generation.summary.last_sync_ms),
        graph_stale: generation.is_none_or(|generation| generation.stale(now_ms)),
        graph_truncated: generation.is_some_and(|generation| generation.summary.truncated),
        truncated_by: generation.and_then(|generation| generation.summary.truncated_by),
    })
}

/// The answer to `GET /api/identities/known`: every identity this home has a
/// local record of and does not control, by ascending id.
///
/// Three local sources merge into one row set, and a row may come from any
/// one of them alone: a ledger under `ledgers/` this home did not root, a node
/// of the stored crawl generation, and an id with nothing but a note under
/// `contacts/`. Nothing here opens a socket or queries DNS, so a row says what
/// this home already knew.
///
/// Every identity under `identities/` is left out. That is every identity this
/// wallet can sign for, and those are the rows `GET /api/identities` serves.
///
/// # Errors
///
/// Returns the storage errors of listing the home and folding the ledgers it
/// stores.
pub fn known_identities(
    core: &WalletCore,
    generation: Option<&Generation>,
) -> Result<Vec<KnownIdentity>, ServiceError> {
    let home = core.home();
    let local: BTreeSet<IdentityId> = home
        .identities()
        .map_err(storage_error)?
        .into_iter()
        .collect();
    let trusted = trusted_subjects(core, &local)?;

    // A `BTreeSet` merges the three sources: one row per identity, whichever
    // sources named it.
    let mut known: BTreeSet<IdentityId> = BTreeSet::new();
    known.extend(home.ledgers().map_err(storage_error)?);
    if let Some(generation) = generation {
        known.extend(
            generation
                .nodes
                .keys()
                .filter_map(|identity| ids::parse_identity(identity).ok()),
        );
    }
    known.extend(core.contact_store().identities().map_err(storage_error)?);

    let names = Names::new(core, generation);
    let mut rows = Vec::new();
    for identity in known {
        if local.contains(&identity) {
            continue;
        }
        let identity_id = ids::identity(identity);
        let resolved = names.resolve(identity);
        let stored = core.holds(identity)?;
        let loaded = stored.then(|| core.load(identity)).transpose()?;
        rows.push(KnownIdentity {
            identity_id: resolved.identity_id,
            display_name: resolved.display_name,
            alias: resolved.alias,
            email: resolved.email,
            hostname: resolved.hostname,
            verification_status: resolved.verification_status,
            declared_kind: loaded.as_ref().map(|loaded| loaded.declared_kind()),
            stored,
            trusted: trusted.contains(&identity),
            // The depth the crawl recorded is the edge count from the root
            // nearest this node, which is what "how far is this from me" means
            // over one generation.
            degrees: generation
                .and_then(|generation| generation.node(&identity_id))
                .map(|node| u64::from(node.depth)),
            head_seq: loaded.map(|loaded| loaded.head_seq),
        });
    }
    // By the rendered id, not by the 32 bytes behind it: the base32 alphabet
    // puts its digits before its letters, so the two orders differ, and the
    // one a client can reproduce from the document is this one.
    rows.sort_by(|left, right| left.identity_id.cmp(&right.identity_id));
    Ok(rows)
}

/// Every identity a ledger in this home currently attests to.
///
/// A revoked attestation is not trust (proposal 001 section 3.4), so the fold's
/// own `revoked_by` is what decides each entry.
fn trusted_subjects(
    core: &WalletCore,
    local: &BTreeSet<IdentityId>,
) -> Result<BTreeSet<IdentityId>, ServiceError> {
    let mut subjects = BTreeSet::new();
    for identity in local {
        // An identity whose ledger never landed attests to nothing, and must
        // not fail the whole listing.
        if !core.holds(*identity)? {
            continue;
        }
        for attestation in core.load(*identity)?.state.trust().values() {
            if !attestation.is_revoked() {
                subjects.insert(attestation.subject);
            }
        }
    }
    Ok(subjects)
}

/// The local identity a lookup defaults to when the caller names none: the
/// lowest identity id in this home.
///
/// Proposal 003 section 3 defaults it to the identity selected in the wallet,
/// which is a browser fact the node does not hold. A client that cares sends
/// `from`.
///
/// # Errors
///
/// Returns code 2 with reason `no_local_identity` on a home holding none.
pub fn default_root(core: &WalletCore) -> Result<IdentityId, ServiceError> {
    core.home()
        .identities()
        .map_err(storage_error)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            ServiceError::usage(
                "no_local_identity",
                "this home holds no identity to look up from",
            )
        })
}

//! One dial budget and one visited set per top-level operation (proposal 006
//! sections 5.1 and 5.2).
//!
//! A top-level operation is one `sync push`, one fetch, one route call or one
//! crawl run. Across witness resolution, the fetches of the target ledger and
//! any DNS lookup it dials at most [`MAX_DIALS`] distinct endpoints, counted
//! once per endpoint id after dedupe, so an endpoint three sources name costs
//! one slot. It shares one deadline: the crawl's [`RUN_BUDGET`] when the
//! operation is a crawl, and [`RESOLVE_BUDGET`] otherwise.
//!
//! Witness resolution is the base operation: resolving the endpoints of witness
//! identity `X` runs the source list of section 5 with sources 4 and 6 removed,
//! so a witness's endpoints are never found by resolving that witness's own
//! witnesses. The visited set terminates the two cases the rules otherwise
//! allow: a ledger naming itself in its own `WitnessSet`, and the same witness
//! named both in `node.json` and in a chain, which would otherwise resolve
//! twice and dial the same endpoints twice.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::Duration;

use iroh::EndpointId;
use mabel_core::IdentityId;

use crate::api::error::ServiceError;
use crate::graph::crawl::RUN_BUDGET;
use crate::graph::model::SourceClass;
use crate::wallet::WalletCore;

/// Distinct endpoints one top-level operation may dial (proposal 006 section
/// 5.2).
pub const MAX_DIALS: usize = 16;

/// How long a top-level operation that is not a crawl may spend resolving and
/// fetching: 20 seconds.
///
/// Two rounds of 8 in flight at the 5-second
/// [`crate::graph::PER_FETCH_TIMEOUT`], plus slack for a DNS lookup.
pub const RESOLVE_BUDGET: Duration = Duration::from_secs(20);

/// Which endpoints one operation has dialled, and what each class has spent.
#[derive(Debug, Default)]
pub struct DialBudget {
    dialled: BTreeSet<EndpointId>,
    spent: BTreeMap<SourceClass, usize>,
}

impl DialBudget {
    /// An unspent budget.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `endpoint` may be dialled as `class`, charging a slot when this
    /// operation has not named it before.
    ///
    /// An endpoint already admitted costs nothing and is always allowed again:
    /// the budget counts endpoints, not mentions.
    pub fn admit(&mut self, class: SourceClass, endpoint: EndpointId) -> bool {
        if class == SourceClass::Local {
            return true;
        }
        if self.dialled.contains(&endpoint) {
            return true;
        }
        let spent = self.spent(class);
        if spent >= class.cap() {
            return false;
        }
        if self.dialled.len() >= self.ceiling(class) {
            return false;
        }
        self.dialled.insert(endpoint);
        *self.spent.entry(class).or_default() += 1;
        true
    }

    /// The most endpoints `class` may see dialled in total, which is
    /// [`MAX_DIALS`] less the reservation no other class may take.
    fn ceiling(&self, class: SourceClass) -> usize {
        let reserved = SourceClass::NodeWitness.reserved();
        if class == SourceClass::NodeWitness {
            return MAX_DIALS;
        }
        MAX_DIALS - reserved.saturating_sub(self.spent(SourceClass::NodeWitness))
    }

    /// Slots `class` has spent.
    #[must_use]
    pub fn spent(&self, class: SourceClass) -> usize {
        self.spent.get(&class).copied().unwrap_or_default()
    }

    /// Distinct endpoints this operation has admitted.
    #[must_use]
    pub fn dialled(&self) -> usize {
        self.dialled.len()
    }

    /// Whether every slot is gone.
    #[must_use]
    pub fn exhausted(&self) -> bool {
        self.dialled.len() >= MAX_DIALS
    }
}

/// The state one top-level operation carries: the dial budget, the witnesses it
/// has resolved, and the ones it is resolving.
#[derive(Debug, Default)]
struct ResolutionState {
    dials: DialBudget,
    /// Endpoints per witness identity, so an identity is resolved once.
    resolved: BTreeMap<IdentityId, Vec<EndpointId>>,
    /// Identities being resolved right now, which is what stops a ledger
    /// naming itself in its own `WitnessSet`.
    resolving: BTreeSet<IdentityId>,
    /// How many resolutions actually ran, which a test asserts on.
    resolutions: usize,
}

/// One dial budget, one deadline and one visited set, shared by every fetch of
/// one top-level operation.
#[derive(Debug)]
pub struct Resolution {
    started: tokio::time::Instant,
    budget: Duration,
    caller_hints: Vec<EndpointId>,
    state: Mutex<ResolutionState>,
}

impl Resolution {
    /// A resolution for one fetch, push or route call: [`RESOLVE_BUDGET`].
    #[must_use]
    pub fn for_operation() -> Self {
        Self::with_budget(RESOLVE_BUDGET)
    }

    /// A resolution for one crawl run: the crawl's [`RUN_BUDGET`].
    #[must_use]
    pub fn for_crawl() -> Self {
        Self::with_budget(RUN_BUDGET)
    }

    /// A resolution under `budget`, which a test shortens.
    #[must_use]
    pub fn with_budget(budget: Duration) -> Self {
        Self {
            started: tokio::time::Instant::now(),
            budget,
            caller_hints: Vec::new(),
            state: Mutex::new(ResolutionState::default()),
        }
    }

    /// The same resolution carrying the endpoints the caller named: a
    /// `mabel://` link's hints, a `--peer` ticket or `--from`.
    ///
    /// They are source 2 for the target and for every witness this operation
    /// resolves, because a ticket is an address this home was handed and not a
    /// statement about one ledger.
    #[must_use]
    pub fn with_caller_hints(mut self, endpoints: Vec<EndpointId>) -> Self {
        self.caller_hints = endpoints;
        self
    }

    /// The endpoints the caller named for this operation.
    #[must_use]
    pub fn caller_hints(&self) -> &[EndpointId] {
        &self.caller_hints
    }

    /// Whether the shared deadline has passed.
    #[must_use]
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= self.budget
    }

    /// What is left of the shared deadline.
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.budget.saturating_sub(self.started.elapsed())
    }

    /// Whether `endpoint` may be dialled as `class`.
    pub fn admit(&self, class: SourceClass, endpoint: EndpointId) -> bool {
        self.lock().dials.admit(class, endpoint)
    }

    /// Distinct endpoints this operation has admitted.
    #[must_use]
    pub fn dialled(&self) -> usize {
        self.lock().dials.dialled()
    }

    /// Slots `class` has spent.
    #[must_use]
    pub fn spent(&self, class: SourceClass) -> usize {
        self.lock().dials.spent(class)
    }

    /// Whether every dial slot is gone.
    #[must_use]
    pub fn exhausted(&self) -> bool {
        self.lock().dials.exhausted()
    }

    /// Witness resolutions this operation actually ran.
    #[must_use]
    pub fn resolutions(&self) -> usize {
        self.lock().resolutions
    }

    /// The endpoints of witness identity `witness`, resolved once per
    /// operation (proposal 006 section 5.1).
    ///
    /// The list is the source list of section 5 with sources 4 and 6 removed:
    /// the caller's endpoints, `peers.json`, the tag-18 advertisement of a local
    /// copy of `witness`, that copy's retired tag-11 list, and the bootstrap
    /// endpoints `node.json` records beside the id. Nothing here dials: a
    /// witness's endpoints are read from what this home already holds, which is
    /// why the bootstrap rules of section 5.4 exist.
    ///
    /// An identity already being resolved yields nothing, which terminates a
    /// ledger naming itself in its own `WitnessSet`. An identity already
    /// resolved yields the same list again without resolving it twice.
    ///
    /// # Errors
    ///
    /// Returns the errors of reading `peers.json` and `node.json`.
    pub fn witness_endpoints(
        &self,
        core: &WalletCore,
        witness: IdentityId,
    ) -> Result<Vec<EndpointId>, ServiceError> {
        {
            let mut state = self.lock();
            if let Some(endpoints) = state.resolved.get(&witness) {
                return Ok(endpoints.clone());
            }
            if !state.resolving.insert(witness) {
                return Ok(Vec::new());
            }
            state.resolutions += 1;
        }
        let resolved = self.resolve_witness(core, witness);
        let mut state = self.lock();
        state.resolving.remove(&witness);
        match resolved {
            Ok(endpoints) => {
                state.resolved.insert(witness, endpoints.clone());
                Ok(endpoints)
            }
            Err(error) => Err(error),
        }
    }

    fn resolve_witness(
        &self,
        core: &WalletCore,
        witness: IdentityId,
    ) -> Result<Vec<EndpointId>, ServiceError> {
        let mut endpoints: Vec<EndpointId> = Vec::new();
        let push = |endpoint: EndpointId, endpoints: &mut Vec<EndpointId>| {
            if !endpoints.contains(&endpoint) {
                endpoints.push(endpoint);
            }
        };
        // Source 2 for the witness itself.
        for endpoint in &self.caller_hints {
            push(*endpoint, &mut endpoints);
        }
        // Source 3.
        let peers = core.home().peers().map_err(crate::wallet::storage_error)?;
        for endpoint in peers.hints(witness) {
            push(endpoint, &mut endpoints);
        }
        // Sources 5 and 7 off a local copy of the witness: what its own chain
        // advertises, then what its retired tag-11 list holds.
        if let Ok(loaded) = core.load(witness)
            && !loaded.is_empty()
            && loaded.violation.is_none()
            && loaded.state.ledger() == Some(witness)
        {
            for endpoint in loaded.state.endpoints() {
                push(*endpoint, &mut endpoints);
            }
            for endpoint in loaded.state.witness_endpoints() {
                push(*endpoint, &mut endpoints);
            }
        }
        // The bootstrap endpoints `node.json` records beside the id, which is
        // what makes a configured witness reachable before anything is fetched.
        for endpoint in core.config()?.witness_endpoints(witness) {
            push(*endpoint, &mut endpoints);
        }
        Ok(endpoints)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ResolutionState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self::for_operation()
    }
}

#[cfg(test)]
mod tests {
    use super::{DialBudget, MAX_DIALS, Resolution};
    use crate::graph::model::SourceClass;

    fn endpoint(seed: u8) -> iroh::EndpointId {
        iroh_base::SecretKey::from_bytes(&[seed; 32]).public()
    }

    /// An endpoint three sources name costs one slot (proposal 006 section
    /// 5.2).
    #[test]
    fn an_endpoint_three_sources_name_costs_one_slot() {
        let mut budget = DialBudget::new();
        assert!(budget.admit(SourceClass::CallerHint, endpoint(1)));
        assert!(budget.admit(SourceClass::PeerHint, endpoint(1)));
        assert!(budget.admit(SourceClass::ChainNamed, endpoint(1)));
        assert_eq!(budget.dialled(), 1);
        assert_eq!(budget.spent(SourceClass::CallerHint), 1);
        assert_eq!(budget.spent(SourceClass::PeerHint), 0);
    }

    #[test]
    fn every_class_stops_at_its_cap() {
        for class in [
            SourceClass::CallerHint,
            SourceClass::PeerHint,
            SourceClass::Dns,
            SourceClass::ChainNamed,
        ] {
            let mut budget = DialBudget::new();
            for seed in 0..class.cap() {
                assert!(
                    budget.admit(class, endpoint(seed as u8)),
                    "{class:?} slot {seed}"
                );
            }
            assert!(
                !budget.admit(class, endpoint(200)),
                "{class:?} over its cap"
            );
        }
    }

    /// Four of the sixteen belong to `node.json.witnesses` and no other class
    /// may take them.
    #[test]
    fn four_slots_stay_reserved_for_node_witnesses() {
        let mut budget = DialBudget::new();
        let mut seed = 0u8;
        for class in [
            SourceClass::CallerHint,
            SourceClass::PeerHint,
            SourceClass::ChainNamed,
        ] {
            for _ in 0..class.cap() {
                seed += 1;
                budget.admit(class, endpoint(seed));
            }
        }
        assert_eq!(budget.dialled(), 12, "the reservation holds four back");
        assert!(!budget.admit(SourceClass::ChainNamed, endpoint(100)));
        assert!(!budget.admit(SourceClass::PeerHint, endpoint(101)));
        for slot in 0..4 {
            assert!(
                budget.admit(SourceClass::NodeWitness, endpoint(150 + slot)),
                "reserved slot {slot}"
            );
        }
        assert_eq!(budget.dialled(), MAX_DIALS);
        assert!(budget.exhausted());
        assert!(!budget.admit(SourceClass::NodeWitness, endpoint(200)));
    }

    #[test]
    fn a_local_read_costs_nothing() {
        let mut budget = DialBudget::new();
        for seed in 0..40 {
            assert!(budget.admit(SourceClass::Local, endpoint(seed)));
        }
        assert_eq!(budget.dialled(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_shared_deadline_expires() {
        let resolution = Resolution::for_operation();
        assert!(!resolution.expired());
        tokio::time::advance(super::RESOLVE_BUDGET).await;
        assert!(resolution.expired());
        assert_eq!(resolution.remaining(), std::time::Duration::ZERO);
    }
}

//! The node HTTP surface, over the same core the CLI drives (proposal 006
//! section 8).
//!
//! One service answers every route on every node. It reads two things over one
//! home: [`WalletCore`], which folds the ledgers and owns every append rule,
//! and [`LedgerStorage`], the one store, which holds the index the sync server
//! answers from, the fork records and the caps. Nothing here is gated on what
//! the home holds.
//!
//! Every method turns the validated request into one call on [`WalletCore`],
//! [`LedgerStorage`] or [`WalletSync`] and renders the document the fixtures
//! under `contracts/http/` freeze. Blocking file work runs under
//! `spawn_blocking`; the network work is already async.
//!
//! Verification is not here. Proposal 004 removed `POST /api/verify` with the
//! verify tab, so [`crate::wallet::Verifier`] is a CLI concern and this
//! surface never renders a verification report.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};

use axum::http::StatusCode;
use iroh_base::EndpointId;
use mabel_core::{IdentityId, LedgerId};

use crate::api::documents::{
    Accepted, Admitted, Appended, Binding, ContactView, CreatedIdentity, DeclaredKind,
    FetchedLedger, ForkList, GraphSynced, GraphView, Id, Identity, IdentityKeys, Invited,
    KnownIdentityList, LedgerPage, Lookup, MembershipView, NodeDocument, ProfileReplaced, Pushed,
    Relay, Removed, ResolveInputKind, ResolveStatus, Resolved, Revoked, VerificationChecked,
    WitnessEndpoint, WitnessEntry, WitnessForRow, WitnessHoldings, WitnessLedgerEntry, WitnessList,
};
use crate::api::error::ServiceError;
use crate::api::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, FetchIdentity,
    ForkQuery, Invite, LookupRequest, NodeService, PageRequest, PushRequest, RemoveMembership,
    ReplaceProfile, ResolveInput, ServiceFuture, SetContact,
};
use crate::bindings;
use crate::config::RelayMode;
use crate::events::fork_document;
use crate::graph::{
    CrawlOptions, GraphStore, LedgerFetcher, NetLedgerFetcher, Resolution, SourceClass, crawl,
    plan_sources,
};
use crate::now_ms;
use crate::storage::LedgerStorage;
use crate::verification::{
    HickoryResolver, ResolveFuture, Resolver, TxtRecord, endpoints_at_label, mabel_claim,
    query_name, verify_hostname,
};
use crate::wallet::core::{AppendLock, WalletCore, no_local_signer, verification_document};
use crate::wallet::error::{no_source_available, storage_error};
use crate::wallet::ids;
use crate::wallet::lookup::{Names, default_root, graph_status, known_identities, lookup_document};
use crate::wallet::sync::WalletSync;

/// The version `GET /api/node` reports.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The node API over one home, one store and one Iroh endpoint.
pub struct NodeApiService {
    core: Arc<WalletCore>,
    storage: Arc<LedgerStorage>,
    sync: WalletSync,
    http_bind: SocketAddr,
    relay: Relay,
    /// The DNS resolver the hostname check queries. Injectable, so no test
    /// reaches the public internet (proposal 003 section 2).
    resolver: Arc<dyn Resolver>,
    /// The crawl's reader, built over `core` and `sync` unless a test
    /// installed one.
    fetcher: Option<Arc<dyn LedgerFetcher>>,
    /// Identities with a background re-check already running, so the
    /// single-identity GET starts at most one per identity.
    refreshing: Arc<StdMutex<HashSet<IdentityId>>>,
}

impl std::fmt::Debug for NodeApiService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeApiService")
            .field("http_bind", &self.http_bind)
            .field("relay", &self.relay)
            .finish_non_exhaustive()
    }
}

impl NodeApiService {
    /// A service over `core` and `storage`, dialling peers through `sync`.
    ///
    /// The resolver is built from the system configuration; a machine with
    /// none gets one that answers every query `unavailable`, which the
    /// verifier reads as `unreachable`. A hostname check is advisory and must
    /// never stop a wallet from starting.
    #[must_use]
    pub fn new(
        core: Arc<WalletCore>,
        storage: Arc<LedgerStorage>,
        sync: WalletSync,
        http_bind: SocketAddr,
        relay: RelayMode,
    ) -> Self {
        let resolver: Arc<dyn Resolver> = match HickoryResolver::system() {
            Ok(resolver) => Arc::new(resolver),
            Err(error) => {
                tracing::warn!(%error, "no system DNS resolver: hostname checks will not answer");
                Arc::new(UnavailableResolver)
            }
        };
        Self {
            core,
            storage,
            sync,
            http_bind,
            relay: match relay {
                RelayMode::N0 => Relay::N0,
                RelayMode::Disabled => Relay::Disabled,
            },
            resolver,
            fetcher: None,
            refreshing: Arc::new(StdMutex::new(HashSet::new())),
        }
    }

    /// The same service, answering hostname checks from `resolver`.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Arc<dyn Resolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// The same service, crawling through `fetcher` instead of the network.
    #[must_use]
    pub fn with_fetcher(mut self, fetcher: Arc<dyn LedgerFetcher>) -> Self {
        self.fetcher = Some(fetcher);
        self
    }

    /// Runs blocking core work off the reactor.
    fn blocking<T, F>(&self, work: F) -> ServiceFuture<'_, T>
    where
        T: Send + 'static,
        F: FnOnce(&WalletCore) -> Result<T, ServiceError> + Send + 'static,
    {
        let core = self.core.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || work(&core)).await {
                Ok(result) => result,
                Err(error) => Err(ServiceError::state(
                    "storage_unavailable",
                    format!("the storage task did not finish: {error}"),
                )
                .with_status(StatusCode::INTERNAL_SERVER_ERROR)),
            }
        })
    }
}

impl NodeService for NodeApiService {
    /// What this node holds and witnesses for, never a role (proposal 006
    /// section 8).
    ///
    /// The counts come from the one store, so they are the numbers the sync
    /// server enforces its caps against, and each `witness_for` entry carries
    /// the advertisement invariant beside it (section 4.1).
    fn node(&self) -> ServiceFuture<'_, NodeDocument> {
        let http_bind = self.http_bind;
        let relay = self.relay;
        let storage = self.storage.clone();
        self.blocking(move |core| {
            let config = core.config()?;
            let totals = storage.totals();
            Ok(NodeDocument {
                endpoint_id: ids::key(&core.endpoint_id()?),
                http_bind,
                relay,
                witnesses: config.witness_bootstrap().iter().map(ids::key).collect(),
                witness_for: storage
                    .witness_for_entries()
                    .iter()
                    .map(|entry| WitnessForRow {
                        identity: ids::identity(entry.identity),
                        advertised: entry.advertised(),
                        reason: entry.gap.map(|gap| gap.reason().to_owned()),
                    })
                    .collect(),
                storage_capacity: storage.caps().storage_capacity,
                storage_used: totals.storage_used,
                identity_count: core.identities()?.len() as u64,
                ledger_count: totals.ledger_count,
                fork_count: totals.fork_count,
                version: VERSION.to_owned(),
            })
        })
    }

    fn identities(&self) -> ServiceFuture<'_, Vec<Identity>> {
        self.blocking(WalletCore::identities)
    }

    /// Answers from the home and the stored crawl generation, cache-only: a
    /// list route never fans out into one DNS query or one fetch per row.
    ///
    /// One page at a time: a home may hold up to ten thousand ledgers, and only
    /// the ids on the page are folded (proposal 006 section 8).
    fn known_identities(&self, page: PageRequest) -> ServiceFuture<'_, KnownIdentityList> {
        self.blocking(move |core| {
            let generation = GraphStore::in_home(core.home())
                .current_generation()
                .map_err(storage_error)?;
            let found = known_identities(core, generation.as_ref(), page)?;
            Ok(KnownIdentityList {
                offset: page.offset,
                limit: page.limit,
                more: found.more,
                identities: found.rows,
            })
        })
    }

    fn create_identity(&self, request: CreateIdentity) -> ServiceFuture<'_, CreatedIdentity> {
        self.blocking(move |core| {
            let founder = request
                .founder
                .as_ref()
                .map(ids::parse_identity)
                .transpose()?;
            core.create_identity(
                &request.alias,
                request.declared_kind,
                founder,
                request.display_name.as_deref(),
                request.email.as_deref(),
            )
        })
    }

    /// Answers from the cache immediately and starts at most one background
    /// re-check when the entry is stale (proposal 003 section 2).
    ///
    /// Resolver trouble never fails this route: the document already carries
    /// what the cache knows, and the refresh is a side effect.
    fn identity(&self, identity_id: Id) -> ServiceFuture<'_, Identity> {
        Box::pin(async move {
            let identity = ids::parse_identity(&identity_id)?;
            let core = self.core.clone();
            let document = spawn(move || core.identity(identity)).await?;
            if document.verification.stale
                && let Some(hostname) = document.verification.hostname.clone()
            {
                self.refresh_in_background(identity, hostname);
            }
            Ok(document)
        })
    }

    fn replace_profile(&self, request: ReplaceProfile) -> ServiceFuture<'_, ProfileReplaced> {
        Box::pin(async move {
            let identity = ids::parse_identity(&request.identity_id)?;
            let lock = self.core.append_lock(identity).await;
            self.fresh(identity, &lock).await?;
            let core = self.core.clone();
            spawn(move || {
                core.replace_profile(
                    &lock,
                    identity,
                    request.display_name.as_deref(),
                    request.hostname.as_deref(),
                    request.email.as_deref(),
                )
            })
            .await
        })
    }

    fn check_verification(&self, identity_id: Id) -> ServiceFuture<'_, VerificationChecked> {
        Box::pin(async move {
            let identity = ids::parse_identity(&identity_id)?;
            let core = self.core.clone();
            let hostname = spawn(move || {
                let profile = core.load(identity)?.profile();
                profile.and_then(|profile| profile.hostname).ok_or_else(|| {
                    ServiceError::policy(
                        "no_hostname_claimed",
                        format!("{identity} claims no hostname, so there is nothing to check"),
                    )
                    .with_detail("identity_id", identity.to_string())
                })
            })
            .await?;

            let outcome = verify_hostname(self.resolver.as_ref(), &hostname, identity).await;
            let core = self.core.clone();
            let verification = spawn(move || {
                let entry = core
                    .verification_store()
                    .record(identity, &outcome, now_ms())
                    .map_err(storage_error)?;
                Ok(verification_document(&entry, now_ms()))
            })
            .await?;
            Ok(VerificationChecked {
                identity_id,
                verification,
            })
        })
    }

    fn contact(&self, identity_id: Id) -> ServiceFuture<'_, ContactView> {
        self.blocking(move |core| {
            let identity = ids::parse_identity(&identity_id)?;
            Ok(ContactView {
                contact: core.contact(identity)?,
                identity_id,
            })
        })
    }

    fn set_contact(&self, request: SetContact) -> ServiceFuture<'_, ContactView> {
        self.blocking(move |core| {
            let identity = ids::parse_identity(&request.identity_id)?;
            Ok(ContactView {
                contact: core.set_contact(identity, request.nickname, request.note)?,
                identity_id: request.identity_id,
            })
        })
    }

    /// Fetches one ledger from a witness and stores it, exactly as `mabel sync
    /// fetch` does (proposal 004).
    ///
    /// `from` is a plain `CallerHint`: a human named that endpoint for this
    /// request, so it is asked whether or not this wallet has heard of it
    /// (proposal 006 section 5, source 2). `from_witness` names a witness
    /// identity instead and is resolved to endpoints through section 5.1.
    /// Naming both is `conflicting_source`.
    ///
    /// With neither, every known source is asked in the order of section 5
    /// until one serves a chain that verifies. A source that could not answer is
    /// skipped; anything else stops the walk, because a chain that does not
    /// verify is an answer about the ledger, not about the source.
    fn fetch_identity(&self, request: FetchIdentity) -> ServiceFuture<'_, FetchedLedger> {
        Box::pin(async move {
            let ledger = ids::parse_ledger(&request.identity_id)?;
            if request.from.is_some() && request.from_witness.is_some() {
                return Err(ServiceError::usage(
                    "conflicting_source",
                    "from names an endpoint and from_witness names an identity: give one",
                )
                .with_detail("parameter", "from_witness"));
            }
            let caller: Vec<EndpointId> = match &request.from {
                Some(from) => vec![ids::parse_endpoint(from)?],
                None => Vec::new(),
            };
            let from_witness = request
                .from_witness
                .as_ref()
                .map(ids::parse_identity)
                .transpose()?;
            let core = self.core.clone();
            let asked = spawn(move || {
                let resolution = Resolution::for_operation().with_caller_hints(caller);
                match from_witness {
                    Some(witness) => {
                        if known_endpoint(&core, witness)? {
                            return Err(endpoint_not_identity(&ids::identity(witness)));
                        }
                        let endpoints = resolution.witness_endpoints(&core, witness)?;
                        if endpoints.is_empty() {
                            return Err(unresolvable_witness(&ids::identity(witness), &[]));
                        }
                        Ok(endpoints.iter().map(ids::key).collect())
                    }
                    None => fetch_sources(&core, ledger, &resolution),
                }
            })
            .await?;
            if asked.is_empty() {
                return Err(ServiceError::usage(
                    "no_witness_configured",
                    format!("this wallet knows no witness to fetch {ledger} from"),
                )
                .with_detail("ledger_id", ledger.to_string()));
            }

            let mut refused: Option<ServiceError> = None;
            for endpoint_id in &asked {
                let endpoint = ids::parse_endpoint(endpoint_id)?;
                match self.sync.fetch(&self.core, ledger, endpoint).await {
                    Ok(fetched) => return Ok(fetched_document(&fetched)),
                    Err(error) if error.reason() == "peer_unreachable" => {
                        refused = Some(
                            endpoint_unreachable(
                                endpoint_id,
                                format!("{endpoint_id} did not answer for {ledger}"),
                                unreachable_detail(&error),
                            )
                            .with_detail("ledger_id", ledger.to_string()),
                        );
                    }
                    Err(error) if error.code() == 30 => refused = Some(error),
                    Err(error) => return Err(error),
                }
            }
            Err(refused.unwrap_or_else(|| {
                no_source_available(
                    ledger,
                    &asked
                        .iter()
                        .filter_map(|endpoint| ids::parse_endpoint(endpoint).ok())
                        .collect::<Vec<EndpointId>>(),
                )
            }))
        })
    }

    fn lookup(&self, request: LookupRequest) -> ServiceFuture<'_, Lookup> {
        self.blocking(move |core| {
            let target = ids::parse_identity(&request.identity_id)?;
            let from = match &request.from {
                Some(from) => {
                    let from = ids::parse_identity(from)?;
                    if !core.home().identity_dir(from).is_dir() {
                        return Err(ServiceError::usage(
                            "unknown_from_identity",
                            format!("no identity here is named {from}"),
                        )
                        .with_detail("parameter", "from")
                        .with_detail("value", from.to_string()));
                    }
                    from
                }
                None => default_root(core)?,
            };
            let generation = GraphStore::in_home(core.home())
                .current_generation()
                .map_err(storage_error)?;
            lookup_document(core, generation.as_ref(), from, target, now_ms())
        })
    }

    fn graph(&self) -> ServiceFuture<'_, GraphView> {
        self.blocking(move |core| {
            let generation = GraphStore::in_home(core.home())
                .current_generation()
                .map_err(storage_error)?;
            Ok(GraphView {
                graph: generation.as_ref().map(|generation| {
                    graph_status(
                        &Names::new(core, Some(generation)),
                        &generation.summary,
                        now_ms(),
                    )
                }),
            })
        })
    }

    /// Runs one crawl from every local identity and swaps the pointer.
    ///
    /// Manual only: nothing here runs on a timer, and the caps of proposal
    /// 003 section 3 bound the run whether or not the caller waits.
    fn sync_graph(&self) -> ServiceFuture<'_, GraphSynced> {
        Box::pin(async move {
            let core = self.core.clone();
            let roots = spawn(move || {
                let roots = core.home().identities().map_err(storage_error)?;
                if roots.is_empty() {
                    return Err(ServiceError::usage(
                        "no_local_identity",
                        "this home holds no identity to crawl from",
                    ));
                }
                Ok(roots)
            })
            .await?;

            let fetcher = self.fetcher.clone().unwrap_or_else(|| {
                Arc::new(
                    NetLedgerFetcher::new((*self.core).clone(), self.sync.clone())
                        // Source 8 needs a resolver, and this service already
                        // holds the one the hostname checks use.
                        .with_resolver(self.resolver.clone()),
                )
            });
            let generation = crawl(&roots, &CrawlOptions::new(), fetcher.as_ref()).await;

            let core = self.core.clone();
            spawn(move || {
                GraphStore::in_home(core.home())
                    .publish(&generation)
                    .map_err(storage_error)?;
                Ok(GraphSynced {
                    graph: graph_status(
                        &Names::new(&core, Some(&generation)),
                        &generation.summary,
                        now_ms(),
                    ),
                })
            })
            .await
        })
    }

    fn identity_ledger(
        &self,
        identity_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage> {
        self.blocking(move |core| core.identity_ledger(ids::parse_identity(&identity_id)?, page))
    }

    fn identity_keys(&self, identity_id: Id) -> ServiceFuture<'_, IdentityKeys> {
        self.blocking(move |core| core.identity_keys(ids::parse_identity(&identity_id)?))
    }

    /// Replaces the witness set of one ledger, after every named id has been
    /// shown to be an identity this home can resolve (proposal 006 section 8).
    fn set_witnesses(&self, identity_id: Id, witnesses: Vec<Id>) -> ServiceFuture<'_, Appended> {
        Box::pin(async move {
            let identity = ids::parse_identity(&identity_id)?;
            let mut named = Vec::with_capacity(witnesses.len());
            for witness in &witnesses {
                named.push(ids::parse_identity(witness)?);
            }
            for witness in &named {
                self.resolvable(*witness).await?;
            }
            let lock = self.core.append_lock(identity).await;
            self.fresh(identity, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.set_witnesses(&lock, identity, &named)).await
        })
    }

    fn set_endpoints(&self, identity_id: Id, endpoints: Vec<Id>) -> ServiceFuture<'_, Appended> {
        Box::pin(async move {
            let identity = ids::parse_identity(&identity_id)?;
            let mut named = Vec::with_capacity(endpoints.len());
            for endpoint in &endpoints {
                named.push(ids::parse_endpoint(endpoint)?);
            }
            let lock = self.core.append_lock(identity).await;
            self.fresh(identity, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.set_endpoints(&lock, identity, &named)).await
        })
    }

    fn memberships(&self, identity_id: Id) -> ServiceFuture<'_, MembershipView> {
        self.blocking(move |core| core.memberships(ids::parse_ledger(&identity_id)?))
    }

    fn invite(&self, request: Invite) -> ServiceFuture<'_, Invited> {
        Box::pin(async move {
            let ledger = ids::parse_ledger(&request.ledger_id)?;
            let by = ids::parse_identity(&request.by)?;
            let lock = self.core.append_lock(ledger).await;
            self.fresh(ledger, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.invite(&lock, ledger, by, request.role, &request.invitee_descriptor))
                .await
        })
    }

    /// Signing an acceptance appends nothing, so the append discipline does
    /// not apply: no ledger moves here.
    fn accept_invitation(&self, request: AcceptInvitation) -> ServiceFuture<'_, Accepted> {
        self.blocking(move |core| {
            core.accept_invitation(
                ids::parse_identity(&request.identity_id)?,
                &request.invitation_bundle,
            )
        })
    }

    fn admit_acceptance(&self, request: AdmitAcceptance) -> ServiceFuture<'_, Admitted> {
        Box::pin(async move {
            let ledger = ids::parse_ledger(&request.ledger_id)?;
            let by = ids::parse_identity(&request.by)?;
            let lock = self.core.append_lock(ledger).await;
            self.fresh(ledger, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.admit_acceptance(&lock, ledger, by, &request.acceptance)).await
        })
    }

    fn remove_membership(&self, request: RemoveMembership) -> ServiceFuture<'_, Removed> {
        Box::pin(async move {
            let ledger = ids::parse_ledger(&request.ledger_id)?;
            let by = ids::parse_identity(&request.by)?;
            let target = ids::parse_identity(&request.target)?;
            let lock = self.core.append_lock(ledger).await;
            self.fresh(ledger, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.remove_membership(&lock, ledger, by, target)).await
        })
    }

    fn add_trust(&self, request: AddTrust) -> ServiceFuture<'_, Appended> {
        Box::pin(async move {
            let issuer = ids::parse_identity(&request.issuer)?;
            let subject = ids::parse_identity(&request.subject)?;
            let lock = self.core.append_lock(issuer).await;
            self.fresh(issuer, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.add_trust(&lock, issuer, subject)).await
        })
    }

    fn revoke_trust(&self, event_id: Id, issuer: Id) -> ServiceFuture<'_, Revoked> {
        Box::pin(async move {
            let issuer = ids::parse_identity(&issuer)?;
            let attestation = ids::parse_event(&event_id)?;
            let lock = self.core.append_lock(issuer).await;
            self.fresh(issuer, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.revoke_trust(&lock, issuer, attestation)).await
        })
    }

    fn push(&self, request: PushRequest) -> ServiceFuture<'_, Pushed> {
        Box::pin(async move {
            let identity = ids::parse_identity(&request.identity_id)?;
            // `to` is an endpoint a caller named for this request, so it is
            // dialled and never written to `peers.json` (proposal 006 section
            // 5.3).
            let (witnesses, caller) = match &request.to {
                Some(to) => {
                    let to = vec![ids::parse_endpoint(to)?];
                    (to.clone(), to)
                }
                None => (self.witnesses_of(identity).await?, Vec::new()),
            };
            let pushed = self
                .sync
                .push_from(&self.core, identity, &witnesses, &caller)
                .await?;
            if pushed
                .results
                .iter()
                .all(|result| result.status != crate::api::documents::PushStatus::Accepted)
            {
                return Err(ServiceError::network(
                    "all_witnesses_failed",
                    format!("no configured witness accepted the push for {identity}"),
                )
                .with_detail("ledger_id", identity.to_string())
                .with_detail("results", &pushed.results));
            }
            Ok(pushed)
        })
    }

    /// One identity id, one hostname or one link, for navigation only.
    ///
    /// Nothing is written and nothing is read from the verification cache of
    /// proposal 003 section 2: a hostname typed into a search box is not a
    /// claim any ledger made, so it gets no cached verdict and leaves none.
    /// Only the label itself is queried, with no CNAME chain.
    ///
    /// An identity id and a link query nothing at all: both already name the
    /// ledger, so `status` is `null` and the endpoints are whatever the link
    /// carried. A hostname is row 1 of the applicability matrix (proposal 006
    /// section 6): the endpoints at the label belong to the identity this same
    /// response resolved to, and a response that resolved to none reports none.
    fn resolve(&self, input: ResolveInput) -> ServiceFuture<'_, Resolved> {
        Box::pin(async move {
            let hostname = match input {
                ResolveInput::Identity(identity_id) => {
                    return Ok(Resolved {
                        input_kind: ResolveInputKind::Identity,
                        identity_id: Some(identity_id),
                        hostname: None,
                        endpoints: Vec::new(),
                        status: None,
                    });
                }
                ResolveInput::Link {
                    identity_id,
                    endpoints,
                } => {
                    return Ok(Resolved {
                        input_kind: ResolveInputKind::Link,
                        identity_id: Some(identity_id),
                        hostname: None,
                        endpoints,
                        status: None,
                    });
                }
                ResolveInput::Hostname(hostname) => hostname,
            };

            let name = query_name(&hostname);
            let Ok(records) = self.resolver.lookup_txt(&name).await else {
                return Ok(Resolved {
                    input_kind: ResolveInputKind::Hostname,
                    identity_id: None,
                    hostname: Some(hostname),
                    endpoints: Vec::new(),
                    status: Some(ResolveStatus::Unreachable),
                });
            };
            let mut claims = 0usize;
            let mut identity_id = None;
            for record in &records {
                let value = record.value();
                let Some(claimed) = mabel_claim(&value) else {
                    continue;
                };
                claims += 1;
                if identity_id.is_none()
                    && let Ok(identity) = claimed.parse::<IdentityId>()
                {
                    identity_id = Some(ids::identity(identity));
                }
            }
            let status = if identity_id.is_some() {
                ResolveStatus::Resolved
            } else if claims == 0 {
                ResolveStatus::NoRecord
            } else {
                ResolveStatus::MismatchedRecords
            };
            // A label that resolved to no identity has no identity to offer
            // endpoints for, so its endpoints records are not read out.
            let endpoints = if identity_id.is_some() {
                endpoints_at_label(&records).iter().map(ids::key).collect()
            } else {
                Vec::new()
            };
            Ok(Resolved {
                input_kind: ResolveInputKind::Hostname,
                identity_id,
                hostname: Some(hostname),
                endpoints,
                status: Some(status),
            })
        })
    }

    fn witnesses(&self) -> ServiceFuture<'_, WitnessList> {
        self.blocking(|core| {
            Ok(WitnessList {
                witnesses: known_witnesses(core)?,
            })
        })
    }

    /// The fork records this home holds, for one ledger or for all of them.
    ///
    /// Every node answers this: a fork is a fact about a stored ledger, and a
    /// home that merely fetched a ledger can meet equivocation on it (proposal
    /// 006 section 8).
    fn forks(&self, query: ForkQuery) -> ServiceFuture<'_, ForkList> {
        let storage = self.storage.clone();
        self.blocking(move |_| {
            let ledger = match &query.ledger_id {
                Some(id) => Some(ids::parse_ledger(id)?),
                None => None,
            };
            let page = query.page;
            let found = storage.forks(ledger, page.offset as usize, page.limit as usize);
            let mut entries = Vec::with_capacity(found.items.len());
            for record in &found.items {
                entries.push(fork_document(record)?);
            }
            Ok(ForkList {
                offset: page.offset,
                limit: page.limit,
                more: found.more,
                entries,
            })
        })
    }

    /// Proxies one `List` request to a witness identity over the sync protocol.
    ///
    /// The identity is resolved to endpoints through proposal 006 section 5.1
    /// first, and each endpoint is asked in that order until one answers.
    /// Nothing is stored: this is what that witness holds right now, read live,
    /// and the ledgers it names are fetched only by the explicit fetch route
    /// (proposal 004).
    ///
    /// An id equal to an endpoint id this home knows is refused before any dial
    /// with a 404: both keys render as 52 base32 characters, so the mistake is
    /// worth naming rather than dialling nothing (proposal 006 section 8).
    fn witness_holdings(
        &self,
        identity_id: Id,
        page: PageRequest,
    ) -> ServiceFuture<'_, WitnessHoldings> {
        Box::pin(async move {
            let identity = ids::parse_identity(&identity_id)?;
            let asked = identity_id.clone();
            let core = self.core.clone();
            let endpoints = spawn(move || {
                if known_endpoint(&core, identity)? {
                    // A drill-in route answers 404 for it: the client asked
                    // for a page that is not there.
                    return Err(endpoint_not_identity(&asked).with_status(StatusCode::NOT_FOUND));
                }
                let endpoints = Resolution::for_operation().witness_endpoints(&core, identity)?;
                if endpoints.is_empty() {
                    return Err(unresolvable_witness(&asked, &[]));
                }
                Ok(endpoints)
            })
            .await?;

            let mut failures: Vec<String> = Vec::new();
            for endpoint in &endpoints {
                match self.sync.list(*endpoint, page.offset, page.limit).await {
                    Ok(served) => {
                        return Ok(WitnessHoldings {
                            identity_id,
                            endpoint_id: ids::key(endpoint),
                            offset: page.offset,
                            limit: page.limit,
                            more: served.more,
                            ledgers: served.items.iter().map(witness_ledger_entry).collect(),
                        });
                    }
                    Err(error) => failures.push(format!("{}: {error}", ids::key(endpoint))),
                }
            }
            Err(witness_unreachable(
                &identity_id,
                format!("no machine answering for {identity_id} served its ledger list"),
                &endpoints,
                failures.join("; "),
            ))
        })
    }
}

impl NodeApiService {
    /// Refuses a witness id this home cannot resolve to a known identity
    /// (proposal 006 section 8).
    ///
    /// Two refusals, in order. An id equal to an endpoint id this home knows is
    /// `endpoint_not_identity`, before any dial. Anything else must resolve: a
    /// local copy answers it outright, and an id with no copy is fetched once
    /// through the endpoints section 5.1 resolves it to, which is what
    /// `--endpoints` bootstraps. An id with no copy and no reachable endpoint is
    /// `unresolvable_witness`, naming what was dialled.
    async fn resolvable(&self, witness: IdentityId) -> Result<(), ServiceError> {
        let core = self.core.clone();
        let endpoints = spawn(move || {
            if known_endpoint(&core, witness)? {
                return Err(endpoint_not_identity(&ids::identity(witness)));
            }
            if core.holds(witness)? {
                return Ok(Vec::new());
            }
            Resolution::for_operation().witness_endpoints(&core, witness)
        })
        .await?;
        if endpoints.is_empty() {
            // Either the copy is already here, or nothing names a machine for
            // it; the first is the common case and answers with no dial.
            let core = self.core.clone();
            let held = spawn(move || core.holds(witness)).await?;
            return if held {
                Ok(())
            } else {
                Err(unresolvable_witness(&ids::identity(witness), &[]))
            };
        }
        for endpoint in &endpoints {
            if self
                .sync
                .fetch(&self.core, witness, *endpoint)
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(unresolvable_witness(&ids::identity(witness), &endpoints))
    }

    /// Starts one background hostname re-check, unless one is already running
    /// for this identity (proposal 003 section 2).
    ///
    /// The GET has already answered from the cache by the time this runs, so
    /// a resolver that never comes back costs nothing but a task.
    fn refresh_in_background(&self, identity: IdentityId, hostname: String) {
        {
            let mut refreshing = self
                .refreshing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !refreshing.insert(identity) {
                return;
            }
        }
        let core = self.core.clone();
        let resolver = self.resolver.clone();
        let refreshing = self.refreshing.clone();
        tokio::spawn(async move {
            let outcome = verify_hostname(resolver.as_ref(), &hostname, identity).await;
            let recorded = tokio::task::spawn_blocking(move || {
                core.verification_store()
                    .record(identity, &outcome, now_ms())
            })
            .await;
            match recorded {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    tracing::warn!(%identity, %error, "the hostname re-check could not be cached");
                }
                Err(error) => {
                    tracing::warn!(%identity, %error, "the hostname re-check task did not finish");
                }
            }
            refreshing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&identity);
        });
    }

    /// The witnesses one ledger is pushed to.
    async fn witnesses_of(&self, identity: IdentityId) -> Result<Vec<EndpointId>, ServiceError> {
        let core = self.core.clone();
        spawn(move || core.witnesses_of(identity)).await
    }

    /// The append preconditions: the ledger is here, this home holds a key that
    /// may append to it, and a ledger this home does not solely control is
    /// checked against its witnesses before anything is signed (proposal 001
    /// section 5, proposal 006 section 8).
    ///
    /// The signer check runs before any dial: a mutating route naming a ledger
    /// this home merely stores answers `no_local_signer` rather than spending
    /// ten seconds on a freshness query it cannot use.
    ///
    /// The caller holds `identity`'s append lock and keeps holding it through
    /// the append, so the head this leaves behind is the head that is signed
    /// on.
    async fn fresh(
        &self,
        identity: mabel_core::IdentityId,
        lock: &AppendLock,
    ) -> Result<(), ServiceError> {
        let core = self.core.clone();
        let shared = spawn(move || {
            let loaded = core.load(identity)?;
            if !core.home().can_sign_for(identity) {
                return Err(no_local_signer(identity));
            }
            Ok(!core.solely_controls(&loaded.state))
        })
        .await?;
        if !shared {
            return Ok(());
        }
        let witnesses = self.witnesses_of(identity).await?;
        if witnesses.is_empty() {
            return Ok(());
        }
        self.sync
            .ensure_fresh_locked(&self.core, identity, &witnesses, lock)
            .await?;
        Ok(())
    }
}

/// The resolver a machine with no system DNS configuration gets.
///
/// Every query answers [`ResolveError::Unavailable`], which the verifier
/// records as `unreachable`: a hostname check is advisory, so a wallet with no
/// resolver still starts, still serves and still says it could not look.
///
/// [`ResolveError::Unavailable`]: crate::verification::ResolveError::Unavailable
#[derive(Debug)]
struct UnavailableResolver;

impl Resolver for UnavailableResolver {
    fn lookup_txt<'a>(&'a self, name: &'a str) -> ResolveFuture<'a, Vec<TxtRecord>> {
        Box::pin(async move {
            Err(crate::verification::ResolveError::Failed {
                name: name.to_owned(),
                message: "this node has no system DNS resolver".to_owned(),
            })
        })
    }
}

/// Every witness identity this home knows, ascending, with the machines that
/// answer for it and the stored ledgers that name it (proposal 006 sections 1
/// and 8).
///
/// Two sources for the rows: the tag-19 `WitnessSet` of every ledger under
/// `ledgers/`, and the witness identities `node.json` configures. An identity
/// only `node.json` names has an empty `named_by`.
///
/// The machines come from resolution (section 5.1), which reads what this home
/// already holds and dials nothing, and each carries the binding of section 4.2:
/// `verified` when the identity's own chain advertises it, `hinted` otherwise.
fn known_witnesses(core: &WalletCore) -> Result<Vec<WitnessEntry>, ServiceError> {
    let mut named: BTreeMap<IdentityId, BTreeSet<Id>> = BTreeMap::new();
    for ledger in core.home().ledgers().map_err(storage_error)? {
        for witness in core.load(ledger)?.state.witness_identities() {
            named
                .entry(*witness)
                .or_default()
                .insert(ids::identity(ledger));
        }
    }
    let mut defaults: BTreeSet<IdentityId> = BTreeSet::new();
    for entry in &core.config()?.witnesses {
        named.entry(entry.identity).or_default();
        defaults.insert(entry.identity);
    }
    // One resolution for the whole list: it dials nothing, and an identity
    // named by two ledgers is resolved once (proposal 006 section 5.1).
    let resolution = Resolution::for_operation();
    let names = Names::new(core, None);
    let mut rows = Vec::new();
    for (identity, named_by) in named {
        let bindings = bindings::read(core.home(), identity).map_err(storage_error)?;
        let endpoints = resolution
            .witness_endpoints(core, identity)?
            .into_iter()
            .map(|endpoint| WitnessEndpoint {
                endpoint_id: ids::key(&endpoint),
                binding: bindings
                    .as_ref()
                    .map_or(Binding::Hinted, |bindings| bindings.binding(endpoint)),
            })
            .collect();
        rows.push(WitnessEntry {
            identity_id: ids::identity(identity),
            display_name: names.resolve(identity).display_name,
            endpoints,
            named_by: named_by.into_iter().collect(),
            is_node_default: defaults.contains(&identity),
            stored: core.holds(identity)?,
        });
    }
    // By the rendered id, which is the order a client can reproduce from the
    // document.
    rows.sort_by(|left, right| left.identity_id.cmp(&right.identity_id));
    Ok(rows)
}

/// Whether `identity` is an endpoint id this home knows, which is the one id a
/// witness surface refuses before dialling anything (proposal 006 section 8).
///
/// The endpoints this home knows are its own endpoint id, every endpoint a
/// stored ledger advertises or lists in a retired tag-11 `WitnessConfig`, every
/// bootstrap endpoint `node.json` records, and every `peers.json` hint. The two
/// key types are both 32 opaque bytes, so the comparison is on the bytes.
fn known_endpoint(core: &WalletCore, identity: IdentityId) -> Result<bool, ServiceError> {
    let wanted = EndpointId::from_bytes(identity.as_bytes()).ok();
    let Some(wanted) = wanted else {
        // 32 bytes that are not a curve point cannot be an endpoint id, so
        // nothing here can name it.
        return Ok(false);
    };
    if core.endpoint_id()? == wanted {
        return Ok(true);
    }
    for endpoint in core.config()?.witness_bootstrap() {
        if endpoint == wanted {
            return Ok(true);
        }
    }
    let peers = core.home().peers().map_err(storage_error)?;
    if peers
        .ledgers
        .values()
        .flatten()
        .any(|hint| hint.endpoint == wanted)
    {
        return Ok(true);
    }
    for ledger in core.home().ledgers().map_err(storage_error)? {
        let loaded = core.load(ledger)?;
        if loaded.state.endpoints().contains(&wanted)
            || loaded.state.witness_endpoints().contains(&wanted)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The endpoints a fetch of `ledger` may ask, in the source order of proposal
/// 006 section 5: the caller's endpoints, the `peers.json` hints for this
/// ledger, the endpoints of every witness identity `node.json` configures, then
/// every other witness endpoint this wallet knows.
///
/// The local copy is not a source here: a fetch is about getting the chain from
/// somewhere else. Every endpoint is charged to `resolution`, so one route call
/// dials at most 16.
fn fetch_sources(
    core: &WalletCore,
    ledger: LedgerId,
    resolution: &Resolution,
) -> Result<Vec<Id>, ServiceError> {
    let mut sources: Vec<Id> = Vec::new();
    for planned in plan_sources(core, ledger, &[], resolution)? {
        if let Some(endpoint) = planned.endpoint {
            push_source(&mut sources, ids::key(&endpoint));
        }
    }
    for witness in known_witnesses(core)? {
        for machine in witness.endpoints {
            let Ok(endpoint) = ids::parse_endpoint(&machine.endpoint_id) else {
                continue;
            };
            if resolution.admit(SourceClass::ChainNamed, endpoint) {
                push_source(&mut sources, machine.endpoint_id);
            }
        }
    }
    Ok(sources)
}

fn push_source(sources: &mut Vec<Id>, endpoint: Id) {
    if !sources.contains(&endpoint) {
        sources.push(endpoint);
    }
}

/// A witness identity whose machines could not be dialled or did not answer:
/// code 30, reason `witness_unreachable`, the identity and every endpoint tried
/// named in `details` (proposal 006 section 8).
fn witness_unreachable(
    identity_id: &Id,
    sentence: String,
    tried: &[EndpointId],
    detail: String,
) -> ServiceError {
    ServiceError::network("witness_unreachable", sentence)
        .with_detail("identity_id", identity_id.as_str())
        .with_detail(
            "endpoints_tried",
            tried.iter().map(ids::key).collect::<Vec<Id>>(),
        )
        .with_detail("error", detail)
}

/// An endpoint that did not answer a fetch: the one caller that names an
/// endpoint and no identity.
fn endpoint_unreachable(endpoint_id: &Id, sentence: String, detail: String) -> ServiceError {
    ServiceError::network("witness_unreachable", sentence)
        .with_detail("endpoint_id", endpoint_id.as_str())
        .with_detail("error", detail)
}

/// An id this home cannot resolve to a known identity: code 2, reason
/// `unresolvable_witness` (proposal 006 section 8).
fn unresolvable_witness(identity_id: &Id, tried: &[EndpointId]) -> ServiceError {
    ServiceError::usage(
        "unresolvable_witness",
        format!("this home knows no machine that answers for {identity_id}"),
    )
    .with_detail("witness", identity_id.as_str())
    .with_detail(
        "endpoints_tried",
        tried.iter().map(ids::key).collect::<Vec<Id>>(),
    )
}

/// An id that names a machine this home knows and not an identity: code 2,
/// reason `endpoint_not_identity`, refused before any dial (proposal 006
/// section 8).
///
/// The id is not ambiguous, it is wrong: an endpoint id and an identity id are
/// both 52 base32 characters, and nothing in the string says which it is.
fn endpoint_not_identity(identity_id: &Id) -> ServiceError {
    ServiceError::usage(
        "endpoint_not_identity",
        format!("{identity_id} is a machine this home knows, not a witness identity"),
    )
    .with_detail("value", identity_id.as_str())
}

/// The sentence a `peer_unreachable` failure carried, so respelling it as
/// `witness_unreachable` loses nothing.
fn unreachable_detail(error: &ServiceError) -> String {
    error
        .details()
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| error.message().to_owned(), ToOwned::to_owned)
}

/// One row of a witness's `List` answer as the proxy renders it.
fn witness_ledger_entry(summary: &mabel_net::store::LedgerSummary) -> WitnessLedgerEntry {
    WitnessLedgerEntry {
        ledger_id: ids::identity(summary.ledger),
        declared_kind: DeclaredKind::parse(mabel_core::declared_kind_name(summary.declared_kind))
            .unwrap_or(DeclaredKind::Person),
        head_seq: summary.head_seq,
        head_event: ids::event(summary.head_event),
        event_count: summary.event_count,
        fork_count: u64::from(summary.fork_count),
    }
}

/// What a fetch stored, in the shape `mabel sync fetch --json` prints.
fn fetched_document(fetched: &crate::wallet::sync::Fetched) -> FetchedLedger {
    FetchedLedger {
        ledger_id: ids::identity(fetched.ledger),
        source: ids::key(&fetched.source),
        event_count: fetched.event_count,
        stored: fetched.stored,
        head_seq: fetched.head_seq,
        head_event: ids::event(fetched.head_event),
        fetched_at_ms: fetched.fetched_at_ms,
        controlled_by: fetched.controlled_by.map(ids::identity),
    }
}

/// Runs one piece of blocking core work off the reactor.
async fn spawn<T, F>(work: F) -> Result<T, ServiceError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ServiceError> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => Err(ServiceError::state(
            "storage_unavailable",
            format!("the storage task did not finish: {error}"),
        )
        .with_status(StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

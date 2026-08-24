//! The wallet HTTP surface, over the same core the CLI drives.
//!
//! Every method turns the validated request into one call on [`WalletCore`]
//! or [`WalletSync`] and renders the document the fixtures under
//! `contracts/http/` freeze. Blocking file work runs under `spawn_blocking`;
//! the network work is already async.
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
    Accepted, Admitted, Appended, ContactView, CreatedIdentity, DeclaredKind, FetchedLedger,
    GraphSynced, GraphView, Id, Identity, IdentityKeys, Invited, LedgerPage, Lookup,
    MembershipView, ProfileReplaced, Pushed, Relay, Removed, ResolveStatus, Resolved, Revoked,
    Role, VerificationChecked, WalletNode, WitnessEntry, WitnessLedgerEntry, WitnessLedgers,
    WitnessList,
};
use crate::api::error::ServiceError;
use crate::api::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, FetchIdentity,
    Invite, LookupRequest, PageRequest, PushRequest, RemoveMembership, ReplaceProfile,
    ServiceFuture, SetContact, WalletService,
};
use crate::config::RelayMode;
use crate::graph::{
    CrawlOptions, GraphStore, LedgerFetcher, NetLedgerFetcher, crawl, plan_sources,
};
use crate::now_ms;
use crate::verification::{
    HickoryResolver, ResolveFuture, Resolver, TxtRecord, mabel_claim, query_name, verify_hostname,
};
use crate::wallet::core::{AppendLock, WalletCore, verification_document};
use crate::wallet::error::{no_source_available, storage_error};
use crate::wallet::ids;
use crate::wallet::lookup::{Names, default_root, graph_status, lookup_document};
use crate::wallet::sync::WalletSync;

/// The version `GET /api/node` reports.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The wallet API over one home and one Iroh endpoint.
pub struct WalletApiService {
    core: Arc<WalletCore>,
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

impl std::fmt::Debug for WalletApiService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WalletApiService")
            .field("http_bind", &self.http_bind)
            .field("relay", &self.relay)
            .finish_non_exhaustive()
    }
}

impl WalletApiService {
    /// A service over `core`, dialling peers through `sync`.
    ///
    /// The resolver is built from the system configuration; a machine with
    /// none gets one that answers every query `unavailable`, which the
    /// verifier reads as `unreachable`. A hostname check is advisory and must
    /// never stop a wallet from starting.
    #[must_use]
    pub fn new(
        core: Arc<WalletCore>,
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

impl WalletService for WalletApiService {
    fn node(&self) -> ServiceFuture<'_, WalletNode> {
        let http_bind = self.http_bind;
        let relay = self.relay;
        self.blocking(move |core| {
            let config = core.config()?;
            Ok(WalletNode {
                role: Role::Wallet,
                endpoint_id: ids::key(&core.endpoint_id()?),
                http_bind,
                relay,
                witnesses: config.witnesses.iter().map(ids::key).collect(),
                storage_capacity: config.storage_capacity,
                storage_used: core.storage_used()?,
                identity_count: core.identities()?.len() as u64,
                version: VERSION.to_owned(),
            })
        })
    }

    fn identities(&self) -> ServiceFuture<'_, Vec<Identity>> {
        self.blocking(WalletCore::identities)
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
    /// With no `from`, every known witness is asked in the crawler's source
    /// order until one serves a chain that verifies. A source that could not
    /// answer is skipped; anything else stops the walk, because a chain that
    /// does not verify is an answer about the ledger, not about the source.
    fn fetch_identity(&self, request: FetchIdentity) -> ServiceFuture<'_, FetchedLedger> {
        Box::pin(async move {
            let ledger = ids::parse_ledger(&request.identity_id)?;
            let core = self.core.clone();
            let known = spawn(move || fetch_sources(&core, ledger)).await?;
            let asked = match &request.from {
                Some(from) if !known.contains(from) => {
                    return Err(ServiceError::usage(
                        "unknown_witness",
                        format!("this wallet knows no witness {from}"),
                    )
                    .with_detail("endpoint_id", from.as_str()));
                }
                Some(from) => vec![from.clone()],
                None => known,
            };
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
                            witness_unreachable(
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
                Arc::new(NetLedgerFetcher::new(
                    (*self.core).clone(),
                    self.sync.clone(),
                ))
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

    fn set_witnesses(&self, identity_id: Id, witnesses: Vec<Id>) -> ServiceFuture<'_, Appended> {
        Box::pin(async move {
            let identity = ids::parse_identity(&identity_id)?;
            let mut endpoints = Vec::with_capacity(witnesses.len());
            for witness in &witnesses {
                endpoints.push(ids::parse_endpoint(witness)?);
            }
            let lock = self.core.append_lock(identity).await;
            self.fresh(identity, &lock).await?;
            let core = self.core.clone();
            spawn(move || core.set_witnesses(&lock, identity, &endpoints)).await
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
            let witnesses = match &request.to {
                Some(to) => vec![ids::parse_endpoint(to)?],
                None => self.witnesses_of(identity).await?,
            };
            let pushed = self.sync.push(&self.core, identity, &witnesses).await?;
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

    /// One TXT lookup of `_mabel.<hostname>.`, for navigation only.
    ///
    /// Nothing is written and nothing is read from the verification cache of
    /// proposal 003 section 2: a hostname typed into a search box is not a
    /// claim any ledger made, so it gets no cached verdict and leaves none.
    /// Only the label itself is queried, with no CNAME chain.
    fn resolve(&self, hostname: String) -> ServiceFuture<'_, Resolved> {
        Box::pin(async move {
            let name = query_name(&hostname);
            let Ok(records) = self.resolver.lookup_txt(&name).await else {
                return Ok(Resolved {
                    hostname,
                    identity_id: None,
                    status: ResolveStatus::Unreachable,
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
            Ok(Resolved {
                hostname,
                identity_id,
                status,
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

    /// Proxies one `List` request to a witness over the sync protocol.
    ///
    /// Nothing is stored: this is what that witness holds right now, read
    /// live, and the ledgers it names are fetched only by the explicit fetch
    /// route (proposal 004).
    fn witness_ledgers(
        &self,
        endpoint_id: Id,
        page: PageRequest,
    ) -> ServiceFuture<'_, WitnessLedgers> {
        Box::pin(async move {
            let endpoint = ids::parse_endpoint(&endpoint_id)?;
            let served = self
                .sync
                .list(endpoint, page.offset, page.limit)
                .await
                .map_err(|error| {
                    witness_unreachable(
                        &endpoint_id,
                        format!("{endpoint_id} did not answer the ledger list"),
                        error.to_string(),
                    )
                })?;
            Ok(WitnessLedgers {
                endpoint_id,
                offset: page.offset,
                limit: page.limit,
                more: served.more,
                ledgers: served.items.iter().map(witness_ledger_entry).collect(),
            })
        })
    }
}

impl WalletApiService {
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

    /// The append discipline: a ledger this wallet does not solely control is
    /// checked against its witnesses before anything is signed (proposal 001
    /// section 5).
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

/// Every witness endpoint this wallet knows, ascending, with the stored
/// ledgers that name it (proposal 004).
///
/// Two sources: the folded `WitnessConfig` of every ledger under `ledgers/`,
/// which is the same fold the identity document renders from, and the
/// node-wide defaults of `node.json`. An endpoint only `node.json` names has
/// an empty `named_by`.
fn known_witnesses(core: &WalletCore) -> Result<Vec<WitnessEntry>, ServiceError> {
    let mut named: BTreeMap<Id, BTreeSet<Id>> = BTreeMap::new();
    for ledger in core.home().ledgers().map_err(storage_error)? {
        for endpoint in core.load(ledger)?.witnesses() {
            named
                .entry(endpoint)
                .or_default()
                .insert(ids::identity(ledger));
        }
    }
    let mut defaults: BTreeSet<Id> = BTreeSet::new();
    for endpoint in core.config()?.witnesses {
        let endpoint = ids::key(&endpoint);
        named.entry(endpoint.clone()).or_default();
        defaults.insert(endpoint);
    }
    Ok(named
        .into_iter()
        .map(|(endpoint_id, named_by)| WitnessEntry {
            is_node_default: defaults.contains(&endpoint_id),
            endpoint_id,
            named_by: named_by.into_iter().collect(),
        })
        .collect())
}

/// The endpoints a fetch of `ledger` may ask, in the crawler's source order
/// (proposal 003 section 3): the `peers.json` hints for this ledger, then the
/// node-wide witnesses, then every other witness this wallet knows.
///
/// The crawl's local copy is not a source here: a fetch is about getting the
/// chain from somewhere else.
fn fetch_sources(core: &WalletCore, ledger: LedgerId) -> Result<Vec<Id>, ServiceError> {
    let mut sources: Vec<Id> = Vec::new();
    for planned in plan_sources(core, ledger, &[])? {
        if let Some(endpoint) = planned.endpoint {
            push_source(&mut sources, ids::key(&endpoint));
        }
    }
    for witness in known_witnesses(core)? {
        push_source(&mut sources, witness.endpoint_id);
    }
    Ok(sources)
}

fn push_source(sources: &mut Vec<Id>, endpoint: Id) {
    if !sources.contains(&endpoint) {
        sources.push(endpoint);
    }
}

/// A witness that could not be dialled or did not answer: code 30, reason
/// `witness_unreachable`, the endpoint named in `details`.
fn witness_unreachable(endpoint_id: &Id, sentence: String, detail: String) -> ServiceError {
    ServiceError::network("witness_unreachable", sentence)
        .with_detail("endpoint_id", endpoint_id.as_str())
        .with_detail("error", detail)
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

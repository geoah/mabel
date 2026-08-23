//! The wallet HTTP surface, over the same core the CLI drives.
//!
//! Every method turns the validated request into one call on [`WalletCore`],
//! [`WalletSync`] or [`Verifier`] and renders the document the fixtures under
//! `contracts/http/` freeze. Blocking file work runs under `spawn_blocking`;
//! the network work is already async.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use iroh_base::EndpointId;

use crate::api::documents::{
    Accepted, Admitted, Appended, CreatedIdentity, Id, Identity, Invited, LedgerPage,
    MembershipView, Pushed, Relay, Removed, Revoked, Role, VerificationReport, WalletNode,
};
use crate::api::error::ServiceError;
use crate::api::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, Invite,
    PushRequest, RemoveMembership, ServiceFuture, VerifyRequest, WalletService,
};
use crate::config::RelayMode;
use crate::wallet::core::{AppendLock, WalletCore};
use crate::wallet::ids;
use crate::wallet::sync::WalletSync;
use crate::wallet::verify::Verifier;

/// The version `GET /api/node` reports.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The wallet API over one home and one Iroh endpoint.
#[derive(Debug)]
pub struct WalletApiService {
    core: Arc<WalletCore>,
    sync: WalletSync,
    http_bind: SocketAddr,
    relay: Relay,
}

impl WalletApiService {
    /// A service over `core`, dialling peers through `sync`.
    #[must_use]
    pub fn new(
        core: Arc<WalletCore>,
        sync: WalletSync,
        http_bind: SocketAddr,
        relay: RelayMode,
    ) -> Self {
        Self {
            core,
            sync,
            http_bind,
            relay: match relay {
                RelayMode::N0 => Relay::N0,
                RelayMode::Disabled => Relay::Disabled,
            },
        }
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
            core.create_identity(&request.alias, request.declared_kind, founder)
        })
    }

    fn identity(&self, identity_id: Id) -> ServiceFuture<'_, Identity> {
        self.blocking(move |core| core.identity(ids::parse_identity(&identity_id)?))
    }

    fn identity_ledger(
        &self,
        identity_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage> {
        self.blocking(move |core| core.identity_ledger(ids::parse_identity(&identity_id)?, page))
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
                None => self.witnesses(identity).await?,
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

    fn verify(&self, request: VerifyRequest) -> ServiceFuture<'_, VerificationReport> {
        Box::pin(async move {
            let verifier = Verifier::new(&self.core, Some(&self.sync));
            match request {
                VerifyRequest::Trust {
                    issuer,
                    subject,
                    from,
                } => {
                    let report = verifier
                        .trust_report(
                            ids::parse_identity(&issuer)?,
                            ids::parse_identity(&subject)?,
                            endpoint(from.as_ref())?,
                        )
                        .await?;
                    Ok(VerificationReport::Trust(report))
                }
                VerifyRequest::Ledger { ledger_id, from } => {
                    let report = verifier
                        .ledger_report(ids::parse_ledger(&ledger_id)?, endpoint(from.as_ref())?)
                        .await?;
                    Ok(VerificationReport::Ledger(report))
                }
            }
        })
    }
}

impl WalletApiService {
    /// The witnesses one ledger is pushed to.
    async fn witnesses(
        &self,
        identity: mabel_core::IdentityId,
    ) -> Result<Vec<EndpointId>, ServiceError> {
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
        let witnesses = self.witnesses(identity).await?;
        if witnesses.is_empty() {
            return Ok(());
        }
        self.sync
            .ensure_fresh_locked(&self.core, identity, &witnesses, lock)
            .await?;
        Ok(())
    }
}

/// The pinned source of a request, if it named one.
fn endpoint(from: Option<&Id>) -> Result<Option<EndpointId>, ServiceError> {
    from.map(ids::parse_endpoint).transpose()
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

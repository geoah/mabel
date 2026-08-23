//! The two service traits the handlers call.
//!
//! Handlers hold no node state and touch no storage: they validate the
//! request, call one trait method and render what comes back (proposal 001
//! section 10, ticket 012). The wallet runtime (ticket 011) implements
//! [`WalletService`] and the witness runtime (ticket 010) implements
//! [`WitnessService`]; [`crate::api::stub`] implements both from the frozen
//! fixtures.
//!
//! Methods return a boxed future rather than `async fn` because the routers
//! take `Arc<dyn WalletService>`, and an `async fn` in a trait is not
//! dyn-compatible. Implementations write `Box::pin(async move { .. })`.

use std::future::Future;
use std::pin::Pin;

use super::documents::{
    Appended, CreatedIdentity, DeclaredKind, ForkList, Id, Identity, LedgerList, LedgerPage,
    LedgerView, Pushed, Revoked, VerificationReport, WalletNode, WitnessNode,
};
use super::error::ServiceError;

/// What a service method returns: a boxed future of a document or the error
/// envelope.
pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ServiceError>> + Send + 'a>>;

/// `POST /api/identities`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateIdentity {
    /// Local label for the new identity.
    pub alias: String,
    /// What it declares itself to be. Only [`DeclaredKind::Person`] and
    /// [`DeclaredKind::Organization`] reach the service; `agent` and `service`
    /// are turned away with code 70 in the handler.
    pub declared_kind: DeclaredKind,
}

/// One page of events, from `?since=` and `?limit=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventPageRequest {
    /// The first sequence number to return. Inclusive.
    pub since: u64,
    /// How many events at most, already clamped.
    pub limit: u32,
}

/// One page of a list, from `?offset=` and `?limit=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    /// How many entries to skip.
    pub offset: u32,
    /// How many entries at most, already clamped.
    pub limit: u32,
}

/// `GET /api/forks`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkQuery {
    /// One ledger, or every ledger when the parameter was absent or empty.
    pub ledger_id: Option<Id>,
    /// The page.
    pub page: PageRequest,
}

/// `POST /api/trust`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTrust {
    /// The ledger that signs the attestation.
    pub issuer: Id,
    /// Who it names.
    pub subject: Id,
}

/// `POST /api/sync/push`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRequest {
    /// The identity to push.
    pub identity_id: Id,
    /// One endpoint, or the identity's configured witnesses when absent.
    pub to: Option<Id>,
}

/// `POST /api/verify`, after validation.
///
/// The frozen fixture pins the `trust` request body only. A `ledger` request
/// names its ledger in `ledger_id`, since `issuer` would be the wrong word for
/// it (a deviation recorded in `crate::api`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyRequest {
    /// Verify trust from an issuer to a subject.
    Trust {
        /// The issuer ledger.
        issuer: Id,
        /// The subject the attestation must name.
        subject: Id,
        /// One source to ask, or every configured source when absent.
        from: Option<Id>,
    },
    /// Verify one ledger's chain.
    Ledger {
        /// The ledger to verify.
        ledger_id: Id,
        /// One source to ask, or every configured source when absent.
        from: Option<Id>,
    },
}

/// The wallet API's view of the node (proposal 001 section 10).
pub trait WalletService: Send + Sync + 'static {
    /// `GET /api/node`.
    fn node(&self) -> ServiceFuture<'_, WalletNode>;

    /// `GET /api/identities`, sorted by ascending id, organizations included.
    fn identities(&self) -> ServiceFuture<'_, Vec<Identity>>;

    /// `POST /api/identities`.
    fn create_identity(&self, request: CreateIdentity) -> ServiceFuture<'_, CreatedIdentity>;

    /// `GET /api/identities/{identity_id}`.
    fn identity(&self, identity_id: Id) -> ServiceFuture<'_, Identity>;

    /// `GET /api/identities/{identity_id}/ledger`.
    fn identity_ledger(
        &self,
        identity_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage>;

    /// `POST /api/identities/{identity_id}/witnesses`.
    fn set_witnesses(&self, identity_id: Id, witnesses: Vec<Id>) -> ServiceFuture<'_, Appended>;

    /// `POST /api/trust`.
    fn add_trust(&self, request: AddTrust) -> ServiceFuture<'_, Appended>;

    /// `POST /api/trust/{event_id}/revoke`.
    fn revoke_trust(&self, event_id: Id, issuer: Id) -> ServiceFuture<'_, Revoked>;

    /// `POST /api/sync/push`.
    fn push(&self, request: PushRequest) -> ServiceFuture<'_, Pushed>;

    /// `POST /api/verify`.
    fn verify(&self, request: VerifyRequest) -> ServiceFuture<'_, VerificationReport>;
}

/// The witness API's view of the node, read-only (proposal 001 section 10).
pub trait WitnessService: Send + Sync + 'static {
    /// `GET /api/node`.
    fn node(&self) -> ServiceFuture<'_, WitnessNode>;

    /// `GET /api/ledgers`, sorted by ascending id.
    fn ledgers(&self, page: PageRequest) -> ServiceFuture<'_, LedgerList>;

    /// `GET /api/ledgers/{ledger_id}`.
    fn ledger(&self, ledger_id: Id) -> ServiceFuture<'_, LedgerView>;

    /// `GET /api/ledgers/{ledger_id}/events`.
    fn ledger_events(&self, ledger_id: Id, page: EventPageRequest)
    -> ServiceFuture<'_, LedgerPage>;

    /// `GET /api/forks`.
    fn forks(&self, query: ForkQuery) -> ServiceFuture<'_, ForkList>;
}

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
    Accepted, Admitted, Appended, ContactView, CreatedIdentity, DeclaredKind, FetchedLedger,
    ForkList, GraphSynced, GraphView, Id, Identity, IdentityKeys, Invited, KnownIdentity,
    LedgerList, LedgerPage, LedgerView, Lookup, MembershipView, ProfileReplaced, Pushed, Removed,
    Resolved, Revoked, RoleName, VerificationChecked, WalletNode, WitnessLedgers, WitnessList,
    WitnessNode,
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
    /// The one founding principal of an identity root, or `None` for a raw
    /// root the new ledger keys itself with (proposal 002 section 2).
    pub founder: Option<Id>,
    /// The name the new ledger publishes, or `None` to publish none. Given, it
    /// lands as one `ProfileUpdate` at seq 1 (proposal 005).
    pub display_name: Option<String>,
    /// The email the new ledger publishes, or `None` to publish none. It rides
    /// in the same `ProfileUpdate` as `display_name`.
    pub email: Option<String>,
}

/// `POST /api/identities/{identity_id}/memberships/invitations`, after
/// validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// The ledger the invitation is appended to, from the path.
    pub ledger_id: Id,
    /// The controller that signs it.
    pub by: Id,
    /// The role offered.
    pub role: RoleName,
    /// The decoded `IdentityDescriptor` of the invitee.
    pub invitee_descriptor: Vec<u8>,
}

/// `POST /api/identities/{identity_id}/memberships/acceptances`, after
/// validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptInvitation {
    /// The identity accepting, from the path. This wallet holds its key.
    pub identity_id: Id,
    /// The decoded `InvitationBundle` the inviter handed over.
    pub invitation_bundle: Vec<u8>,
}

/// `POST /api/identities/{identity_id}/memberships/admissions`, after
/// validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmitAcceptance {
    /// The ledger the acceptance is appended to, from the path.
    pub ledger_id: Id,
    /// The controller that signs the acceptance event.
    pub by: Id,
    /// The decoded `AcceptanceFile` the invitee signed.
    pub acceptance: Vec<u8>,
}

/// `POST /api/identities/{identity_id}/memberships/removals`, after
/// validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveMembership {
    /// The ledger the removal is appended to, from the path.
    pub ledger_id: Id,
    /// The controller that signs it.
    pub by: Id,
    /// The identity to remove, whose principal and open invitation both go.
    pub target: Id,
}

/// `POST /api/identities/{identity_id}/profile`, after validation.
///
/// All three fields are here because the operation is replacement: `None`
/// clears that field, and the body must carry all three keys so no client can
/// half-specify one (proposal 003 section 1, proposal 005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceProfile {
    /// The ledger the update is appended to, from the path.
    pub identity_id: Id,
    /// The name to publish, or `None` to clear it.
    pub display_name: Option<String>,
    /// The hostname to claim, or `None` to clear it.
    pub hostname: Option<String>,
    /// The email to publish, or `None` to clear it.
    pub email: Option<String>,
}

/// `PUT /api/identities/{identity_id}/contact`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetContact {
    /// The identity the note is about, local or foreign.
    pub identity_id: Id,
    /// The private name, or `None` to clear it.
    pub nickname: Option<String>,
    /// The private note, or `None` to clear it.
    pub note: Option<String>,
}

/// `GET /api/lookup/{identity_id}`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupRequest {
    /// The identity to look up, which need not be in this home.
    pub identity_id: Id,
    /// The local root the answer is relative to, or `None` for the default.
    pub from: Option<Id>,
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

/// `POST /api/identities/{identity_id}/fetch`, after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchIdentity {
    /// The ledger to fetch, from the path. This home need not hold it.
    pub identity_id: Id,
    /// One witness to ask, or every known witness in the crawler's source
    /// order when absent.
    pub from: Option<Id>,
}

/// The wallet API's view of the node (proposal 001 section 10).
pub trait WalletService: Send + Sync + 'static {
    /// `GET /api/node`.
    fn node(&self) -> ServiceFuture<'_, WalletNode>;

    /// `GET /api/identities`, sorted by ascending id, organizations included.
    fn identities(&self) -> ServiceFuture<'_, Vec<Identity>>;

    /// `POST /api/identities`.
    fn create_identity(&self, request: CreateIdentity) -> ServiceFuture<'_, CreatedIdentity>;

    /// `GET /api/identities/known`, sorted by ascending id: every identity this
    /// home has a local record of and does not control.
    fn known_identities(&self) -> ServiceFuture<'_, Vec<KnownIdentity>>;

    /// `GET /api/identities/{identity_id}`.
    fn identity(&self, identity_id: Id) -> ServiceFuture<'_, Identity>;

    /// `GET /api/identities/{identity_id}/ledger`.
    fn identity_ledger(
        &self,
        identity_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage>;

    /// `GET /api/identities/{identity_id}/keys`.
    fn identity_keys(&self, identity_id: Id) -> ServiceFuture<'_, IdentityKeys>;

    /// `POST /api/identities/{identity_id}/witnesses`.
    fn set_witnesses(&self, identity_id: Id, witnesses: Vec<Id>) -> ServiceFuture<'_, Appended>;

    /// `POST /api/identities/{identity_id}/profile`.
    fn replace_profile(&self, request: ReplaceProfile) -> ServiceFuture<'_, ProfileReplaced>;

    /// `POST /api/identities/{identity_id}/verification`, which forces a DNS
    /// check and waits for it (proposal 003 section 2).
    fn check_verification(&self, identity_id: Id) -> ServiceFuture<'_, VerificationChecked>;

    /// `GET /api/identities/{identity_id}/contact`.
    fn contact(&self, identity_id: Id) -> ServiceFuture<'_, ContactView>;

    /// `PUT /api/identities/{identity_id}/contact`.
    fn set_contact(&self, request: SetContact) -> ServiceFuture<'_, ContactView>;

    /// `POST /api/identities/{identity_id}/fetch`, the CLI `sync fetch` behind
    /// a route.
    fn fetch_identity(&self, request: FetchIdentity) -> ServiceFuture<'_, FetchedLedger>;

    /// `GET /api/lookup/{identity_id}?from=`.
    fn lookup(&self, request: LookupRequest) -> ServiceFuture<'_, Lookup>;

    /// `GET /api/resolve/{hostname}`, one TXT lookup that never touches the
    /// verification cache (proposal 004).
    fn resolve(&self, hostname: String) -> ServiceFuture<'_, Resolved>;

    /// `GET /api/witnesses`, sorted by ascending `endpoint_id`.
    fn witnesses(&self) -> ServiceFuture<'_, WitnessList>;

    /// `GET /api/witnesses/{endpoint_id}/ledgers`, a live `List` against that
    /// witness.
    fn witness_ledgers(
        &self,
        endpoint_id: Id,
        page: PageRequest,
    ) -> ServiceFuture<'_, WitnessLedgers>;

    /// `GET /api/graph`.
    fn graph(&self) -> ServiceFuture<'_, GraphView>;

    /// `POST /api/graph/sync`, which runs one crawl and swaps the pointer.
    fn sync_graph(&self) -> ServiceFuture<'_, GraphSynced>;

    /// `GET /api/identities/{identity_id}/memberships`.
    fn memberships(&self, identity_id: Id) -> ServiceFuture<'_, MembershipView>;

    /// `POST /api/identities/{identity_id}/memberships/invitations`.
    fn invite(&self, request: Invite) -> ServiceFuture<'_, Invited>;

    /// `POST /api/identities/{identity_id}/memberships/acceptances`, which
    /// signs a detached acceptance and appends nothing.
    fn accept_invitation(&self, request: AcceptInvitation) -> ServiceFuture<'_, Accepted>;

    /// `POST /api/identities/{identity_id}/memberships/admissions`.
    fn admit_acceptance(&self, request: AdmitAcceptance) -> ServiceFuture<'_, Admitted>;

    /// `POST /api/identities/{identity_id}/memberships/removals`.
    fn remove_membership(&self, request: RemoveMembership) -> ServiceFuture<'_, Removed>;

    /// `POST /api/trust`.
    fn add_trust(&self, request: AddTrust) -> ServiceFuture<'_, Appended>;

    /// `POST /api/trust/{event_id}/revoke`.
    fn revoke_trust(&self, event_id: Id, issuer: Id) -> ServiceFuture<'_, Revoked>;

    /// `POST /api/sync/push`.
    fn push(&self, request: PushRequest) -> ServiceFuture<'_, Pushed>;
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

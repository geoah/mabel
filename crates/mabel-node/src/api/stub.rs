//! The service that answers from the frozen fixtures.
//!
//! The stub answers every route with the `response` document of the matching
//! file under `contracts/http/`, records the calls it received, and can be
//! told to fail with any [`ServiceError`]. The contract tests use it to check
//! the router against the fixtures without a node home; the runtime answers the
//! same routes in production.
//!
//! Because the fixtures are compiled in with `include_str!`, a fixture whose
//! shape drifts from [`super::documents`] fails the test suite instead of
//! drifting quietly.

use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::documents::{
    Accepted, Admitted, Appended, ContactView, CreatedIdentity, FetchedLedger, ForkList,
    GraphSynced, GraphView, Id, Identity, IdentityKeys, IdentityList, IdentityView, Invited,
    KnownIdentityList, LedgerPage, Lookup, MembershipView, NodeDocument, ProfileReplaced, Pushed,
    Removed, Resolved, Revoked, VerificationChecked, WitnessHoldings, WitnessList,
};
use super::error::ServiceError;
use super::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, FetchIdentity,
    ForkQuery, Invite, LookupRequest, NodeService, PageRequest, PushRequest, RemoveMembership,
    ReplaceProfile, ResolveInput, ServiceFuture, SetContact,
};

/// One file under `contracts/http/`.
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    /// The file name, `node-get-node.json` and so on.
    pub name: &'static str,
    /// Its contents.
    pub json: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        Fixture {
            name: concat!($name, ".json"),
            json: include_str!(concat!("../../../../contracts/http/", $name, ".json")),
        }
    };
}

/// Every frozen HTTP fixture, in the order `contracts/README.md` indexes them.
pub const FIXTURES: [Fixture; 29] = [
    fixture!("node-get-node"),
    fixture!("wallet-get-identities"),
    fixture!("wallet-post-identities"),
    fixture!("wallet-get-known-identities"),
    fixture!("wallet-get-identity"),
    fixture!("wallet-get-identity-ledger"),
    fixture!("wallet-get-identity-keys"),
    fixture!("wallet-post-identity-profile"),
    fixture!("wallet-post-identity-verification"),
    fixture!("wallet-get-identity-contact"),
    fixture!("wallet-put-identity-contact"),
    fixture!("wallet-post-identity-fetch"),
    fixture!("wallet-get-lookup"),
    fixture!("wallet-get-resolve"),
    fixture!("wallet-get-witnesses"),
    fixture!("wallet-get-witness-holdings"),
    fixture!("wallet-get-graph"),
    fixture!("wallet-post-graph-sync"),
    fixture!("wallet-post-identity-witnesses"),
    fixture!("wallet-post-identity-endpoints"),
    fixture!("wallet-get-identity-memberships"),
    fixture!("wallet-post-membership-invitations"),
    fixture!("wallet-post-membership-acceptances"),
    fixture!("wallet-post-membership-admissions"),
    fixture!("wallet-post-membership-removals"),
    fixture!("wallet-post-trust"),
    fixture!("wallet-post-trust-revoke"),
    fixture!("wallet-post-sync-push"),
    fixture!("node-get-forks"),
];

impl Fixture {
    /// The fixture by file name.
    ///
    /// # Panics
    ///
    /// Panics when no fixture has that name.
    #[must_use]
    pub fn named(name: &str) -> Self {
        FIXTURES
            .into_iter()
            .find(|fixture| fixture.name == name)
            .unwrap_or_else(|| panic!("no fixture named {name}"))
    }

    /// The whole file: `route`, `method`, `request`, `response`, `errors`.
    ///
    /// # Panics
    ///
    /// Panics when the file is not the JSON object it is frozen as.
    #[must_use]
    pub fn value(self) -> Value {
        serde_json::from_str(self.json)
            .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", self.name))
    }

    /// The `route` string, path parameters spelled `:identity_id` and any
    /// example query string included.
    ///
    /// # Panics
    ///
    /// Panics when the fixture has no `route`.
    #[must_use]
    pub fn route(self) -> String {
        self.string("route")
    }

    /// The `method` string.
    ///
    /// # Panics
    ///
    /// Panics when the fixture has no `method`.
    #[must_use]
    pub fn method(self) -> String {
        self.string("method")
    }

    /// The example request body, `null` on a `GET`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture has no `request` key.
    #[must_use]
    pub fn request(self) -> Value {
        self.member("request")
    }

    /// The example 200 body, `ok: true` included.
    ///
    /// # Panics
    ///
    /// Panics when the fixture has no `response` key.
    #[must_use]
    pub fn response(self) -> Value {
        self.member("response")
    }

    /// The `{status, body}` examples.
    ///
    /// # Panics
    ///
    /// Panics when `errors` is not an array of `{status, body}` objects.
    #[must_use]
    pub fn errors(self) -> Vec<(u16, Value)> {
        self.member("errors")
            .as_array()
            .unwrap_or_else(|| panic!("{} has no errors array", self.name))
            .iter()
            .map(|error| {
                let status = error
                    .get("status")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| panic!("{} has an error with no status", self.name));
                let body = error
                    .get("body")
                    .unwrap_or_else(|| panic!("{} has an error with no body", self.name))
                    .clone();
                (u16::try_from(status).unwrap_or(u16::MAX), body)
            })
            .collect()
    }

    /// The `response` document parsed into the type that serves it, with the
    /// envelope's `ok` removed first.
    ///
    /// # Panics
    ///
    /// Panics when the fixture does not parse into `T`, which means the type
    /// and the frozen contract disagree.
    #[must_use]
    pub fn parse_response<T: DeserializeOwned>(self) -> T {
        let mut response = self.response();
        response
            .as_object_mut()
            .unwrap_or_else(|| panic!("{} has a response that is not an object", self.name))
            .remove("ok");
        serde_json::from_value(response)
            .unwrap_or_else(|error| panic!("{} does not parse: {error}", self.name))
    }

    fn member(self, key: &str) -> Value {
        self.value()
            .get(key)
            .unwrap_or_else(|| panic!("{} has no {key}", self.name))
            .clone()
    }

    fn string(self, key: &str) -> String {
        self.member(key)
            .as_str()
            .unwrap_or_else(|| panic!("{} has a {key} that is not a string", self.name))
            .to_owned()
    }
}

/// Which [`NodeService`] method was called, and with what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeCall {
    /// `GET /api/node`.
    Node,
    /// `GET /api/identities`.
    Identities,
    /// `POST /api/identities`.
    CreateIdentity(CreateIdentity),
    /// `GET /api/identities/known`.
    KnownIdentities(PageRequest),
    /// `GET /api/identities/{identity_id}`.
    Identity(Id),
    /// `GET /api/identities/{identity_id}/ledger`.
    IdentityLedger(Id, EventPageRequest),
    /// `GET /api/identities/{identity_id}/keys`.
    IdentityKeys(Id),
    /// `POST /api/identities/{identity_id}/witnesses`.
    SetWitnesses(Id, Vec<Id>),
    /// `POST /api/identities/{identity_id}/endpoints`.
    SetEndpoints(Id, Vec<Id>),
    /// `POST /api/identities/{identity_id}/profile`.
    ReplaceProfile(ReplaceProfile),
    /// `POST /api/identities/{identity_id}/verification`.
    CheckVerification(Id),
    /// `GET /api/identities/{identity_id}/contact`.
    Contact(Id),
    /// `PUT /api/identities/{identity_id}/contact`.
    SetContact(SetContact),
    /// `POST /api/identities/{identity_id}/fetch`.
    FetchIdentity(FetchIdentity),
    /// `GET /api/lookup/{identity_id}`.
    Lookup(LookupRequest),
    /// `GET /api/resolve?input=`.
    Resolve(ResolveInput),
    /// `GET /api/witnesses`.
    Witnesses,
    /// `GET /api/witnesses/{identity_id}/holdings`.
    WitnessHoldings(Id, PageRequest),
    /// `GET /api/forks`.
    Forks(ForkQuery),
    /// `GET /api/graph`.
    Graph,
    /// `POST /api/graph/sync`.
    SyncGraph,
    /// `GET /api/identities/{identity_id}/memberships`.
    Memberships(Id),
    /// `POST /api/identities/{identity_id}/memberships/invitations`.
    Invite(Invite),
    /// `POST /api/identities/{identity_id}/memberships/acceptances`.
    AcceptInvitation(AcceptInvitation),
    /// `POST /api/identities/{identity_id}/memberships/admissions`.
    AdmitAcceptance(AdmitAcceptance),
    /// `POST /api/identities/{identity_id}/memberships/removals`.
    RemoveMembership(RemoveMembership),
    /// `POST /api/trust`.
    AddTrust(AddTrust),
    /// `POST /api/trust/{event_id}/revoke`.
    RevokeTrust(Id, Id),
    /// `POST /api/sync/push`.
    Push(PushRequest),
}

/// A [`NodeService`] that answers from the frozen fixtures.
///
/// The document fields are public, so a test can change one answer and leave
/// the rest frozen.
#[derive(Debug)]
pub struct StubNodeService {
    /// `GET /api/node`.
    pub node: NodeDocument,
    /// `GET /api/identities`.
    pub identities: Vec<Identity>,
    /// `POST /api/identities`.
    pub created_identity: CreatedIdentity,
    /// `GET /api/identities/known`.
    pub known_identities: KnownIdentityList,
    /// `GET /api/identities/{identity_id}`.
    pub identity: Identity,
    /// `GET /api/identities/{identity_id}/ledger`.
    pub identity_ledger: LedgerPage,
    /// `GET /api/identities/{identity_id}/keys`.
    pub identity_keys: IdentityKeys,
    /// `POST /api/identities/{identity_id}/witnesses`.
    pub witnesses_appended: Appended,
    /// `POST /api/identities/{identity_id}/endpoints`.
    pub endpoints_appended: Appended,
    /// `POST /api/identities/{identity_id}/profile`.
    pub profile_replaced: ProfileReplaced,
    /// `POST /api/identities/{identity_id}/verification`.
    pub verification_checked: VerificationChecked,
    /// `GET /api/identities/{identity_id}/contact`.
    pub contact: ContactView,
    /// `PUT /api/identities/{identity_id}/contact`.
    pub contact_set: ContactView,
    /// `POST /api/identities/{identity_id}/fetch`.
    pub fetched: FetchedLedger,
    /// `GET /api/lookup/{identity_id}`.
    pub lookup: Lookup,
    /// `GET /api/resolve?input=`.
    pub resolved: Resolved,
    /// `GET /api/witnesses`.
    pub witnesses: WitnessList,
    /// `GET /api/witnesses/{identity_id}/holdings`.
    pub witness_holdings: WitnessHoldings,
    /// `GET /api/forks`.
    pub forks: ForkList,
    /// `GET /api/graph`.
    pub graph: GraphView,
    /// `POST /api/graph/sync`.
    pub graph_synced: GraphSynced,
    /// `GET /api/identities/{identity_id}/memberships`.
    pub memberships: MembershipView,
    /// `POST /api/identities/{identity_id}/memberships/invitations`.
    pub invited: Invited,
    /// `POST /api/identities/{identity_id}/memberships/acceptances`.
    pub accepted: Accepted,
    /// `POST /api/identities/{identity_id}/memberships/admissions`.
    pub admitted: Admitted,
    /// `POST /api/identities/{identity_id}/memberships/removals`.
    pub removed: Removed,
    /// `POST /api/trust`.
    pub trust_appended: Appended,
    /// `POST /api/trust/{event_id}/revoke`.
    pub revoked: Revoked,
    /// `POST /api/sync/push`.
    pub pushed: Pushed,
    failure: Mutex<Option<ServiceError>>,
    calls: Mutex<Vec<NodeCall>>,
}

impl Default for StubNodeService {
    fn default() -> Self {
        Self::new()
    }
}

impl StubNodeService {
    /// A stub primed from the frozen fixtures.
    ///
    /// # Panics
    ///
    /// Panics when a fixture does not parse into the document type that serves
    /// it.
    #[must_use]
    pub fn new() -> Self {
        let identities: IdentityList =
            Fixture::named("wallet-get-identities.json").parse_response();
        let identity: IdentityView = Fixture::named("wallet-get-identity.json").parse_response();
        Self {
            node: Fixture::named("node-get-node.json").parse_response(),
            identities: identities.identities,
            created_identity: Fixture::named("wallet-post-identities.json").parse_response(),
            known_identities: Fixture::named("wallet-get-known-identities.json").parse_response(),
            identity: identity.identity,
            identity_ledger: Fixture::named("wallet-get-identity-ledger.json").parse_response(),
            identity_keys: Fixture::named("wallet-get-identity-keys.json").parse_response(),
            witnesses_appended: Fixture::named("wallet-post-identity-witnesses.json")
                .parse_response(),
            endpoints_appended: Fixture::named("wallet-post-identity-endpoints.json")
                .parse_response(),
            profile_replaced: Fixture::named("wallet-post-identity-profile.json").parse_response(),
            verification_checked: Fixture::named("wallet-post-identity-verification.json")
                .parse_response(),
            contact: Fixture::named("wallet-get-identity-contact.json").parse_response(),
            contact_set: Fixture::named("wallet-put-identity-contact.json").parse_response(),
            fetched: Fixture::named("wallet-post-identity-fetch.json").parse_response(),
            lookup: Fixture::named("wallet-get-lookup.json").parse_response(),
            resolved: Fixture::named("wallet-get-resolve.json").parse_response(),
            witnesses: Fixture::named("wallet-get-witnesses.json").parse_response(),
            witness_holdings: Fixture::named("wallet-get-witness-holdings.json").parse_response(),
            forks: Fixture::named("node-get-forks.json").parse_response(),
            graph: Fixture::named("wallet-get-graph.json").parse_response(),
            graph_synced: Fixture::named("wallet-post-graph-sync.json").parse_response(),
            memberships: Fixture::named("wallet-get-identity-memberships.json").parse_response(),
            invited: Fixture::named("wallet-post-membership-invitations.json").parse_response(),
            accepted: Fixture::named("wallet-post-membership-acceptances.json").parse_response(),
            admitted: Fixture::named("wallet-post-membership-admissions.json").parse_response(),
            removed: Fixture::named("wallet-post-membership-removals.json").parse_response(),
            trust_appended: Fixture::named("wallet-post-trust.json").parse_response(),
            revoked: Fixture::named("wallet-post-trust-revoke.json").parse_response(),
            pushed: Fixture::named("wallet-post-sync-push.json").parse_response(),
            failure: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Makes every later call fail with `error`.
    pub fn fail_with(&self, error: ServiceError) {
        *lock(&self.failure) = Some(error);
    }

    /// The calls this stub received, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<NodeCall> {
        lock(&self.calls).clone()
    }

    /// The one call this stub received.
    ///
    /// # Panics
    ///
    /// Panics unless exactly one call was made.
    #[must_use]
    pub fn call(&self) -> NodeCall {
        let calls = self.calls();
        assert_eq!(calls.len(), 1, "expected one call, got {calls:?}");
        calls.into_iter().next().expect("one call")
    }

    fn answer<T: Send + 'static>(&self, call: NodeCall, value: T) -> ServiceFuture<'_, T> {
        lock(&self.calls).push(call);
        let result = match lock(&self.failure).clone() {
            Some(error) => Err(error),
            None => Ok(value),
        };
        Box::pin(async move { result })
    }
}

impl NodeService for StubNodeService {
    fn node(&self) -> ServiceFuture<'_, NodeDocument> {
        self.answer(NodeCall::Node, self.node.clone())
    }

    fn identities(&self) -> ServiceFuture<'_, Vec<Identity>> {
        self.answer(NodeCall::Identities, self.identities.clone())
    }

    fn create_identity(&self, request: CreateIdentity) -> ServiceFuture<'_, CreatedIdentity> {
        self.answer(
            NodeCall::CreateIdentity(request),
            self.created_identity.clone(),
        )
    }

    fn known_identities(&self, page: PageRequest) -> ServiceFuture<'_, KnownIdentityList> {
        self.answer(
            NodeCall::KnownIdentities(page),
            self.known_identities.clone(),
        )
    }

    fn identity(&self, identity_id: Id) -> ServiceFuture<'_, Identity> {
        self.answer(NodeCall::Identity(identity_id), self.identity.clone())
    }

    fn identity_ledger(
        &self,
        identity_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage> {
        self.answer(
            NodeCall::IdentityLedger(identity_id, page),
            self.identity_ledger.clone(),
        )
    }

    fn identity_keys(&self, identity_id: Id) -> ServiceFuture<'_, IdentityKeys> {
        self.answer(
            NodeCall::IdentityKeys(identity_id),
            self.identity_keys.clone(),
        )
    }

    fn set_witnesses(&self, identity_id: Id, witnesses: Vec<Id>) -> ServiceFuture<'_, Appended> {
        self.answer(
            NodeCall::SetWitnesses(identity_id, witnesses),
            self.witnesses_appended.clone(),
        )
    }

    /// Both list appends answer the one `Appended` document, and each has its
    /// own fixture because the two differ in `payload_kind` and in the key
    /// their `payload` holds: `witness_set` with `witnesses`,
    /// `endpoint_advertisement` with `endpoints`.
    fn set_endpoints(&self, identity_id: Id, endpoints: Vec<Id>) -> ServiceFuture<'_, Appended> {
        self.answer(
            NodeCall::SetEndpoints(identity_id, endpoints),
            self.endpoints_appended.clone(),
        )
    }

    fn replace_profile(&self, request: ReplaceProfile) -> ServiceFuture<'_, ProfileReplaced> {
        self.answer(
            NodeCall::ReplaceProfile(request),
            self.profile_replaced.clone(),
        )
    }

    fn check_verification(&self, identity_id: Id) -> ServiceFuture<'_, VerificationChecked> {
        self.answer(
            NodeCall::CheckVerification(identity_id),
            self.verification_checked.clone(),
        )
    }

    fn contact(&self, identity_id: Id) -> ServiceFuture<'_, ContactView> {
        self.answer(NodeCall::Contact(identity_id), self.contact.clone())
    }

    fn set_contact(&self, request: SetContact) -> ServiceFuture<'_, ContactView> {
        self.answer(NodeCall::SetContact(request), self.contact_set.clone())
    }

    fn fetch_identity(&self, request: FetchIdentity) -> ServiceFuture<'_, FetchedLedger> {
        self.answer(NodeCall::FetchIdentity(request), self.fetched.clone())
    }

    fn lookup(&self, request: LookupRequest) -> ServiceFuture<'_, Lookup> {
        self.answer(NodeCall::Lookup(request), self.lookup.clone())
    }

    fn resolve(&self, input: ResolveInput) -> ServiceFuture<'_, Resolved> {
        self.answer(NodeCall::Resolve(input), self.resolved.clone())
    }

    fn witnesses(&self) -> ServiceFuture<'_, WitnessList> {
        self.answer(NodeCall::Witnesses, self.witnesses.clone())
    }

    fn witness_holdings(
        &self,
        identity_id: Id,
        page: PageRequest,
    ) -> ServiceFuture<'_, WitnessHoldings> {
        self.answer(
            NodeCall::WitnessHoldings(identity_id, page),
            self.witness_holdings.clone(),
        )
    }

    fn graph(&self) -> ServiceFuture<'_, GraphView> {
        self.answer(NodeCall::Graph, self.graph.clone())
    }

    fn sync_graph(&self) -> ServiceFuture<'_, GraphSynced> {
        self.answer(NodeCall::SyncGraph, self.graph_synced.clone())
    }

    fn memberships(&self, identity_id: Id) -> ServiceFuture<'_, MembershipView> {
        self.answer(NodeCall::Memberships(identity_id), self.memberships.clone())
    }

    fn invite(&self, request: Invite) -> ServiceFuture<'_, Invited> {
        self.answer(NodeCall::Invite(request), self.invited.clone())
    }

    fn accept_invitation(&self, request: AcceptInvitation) -> ServiceFuture<'_, Accepted> {
        self.answer(NodeCall::AcceptInvitation(request), self.accepted.clone())
    }

    fn admit_acceptance(&self, request: AdmitAcceptance) -> ServiceFuture<'_, Admitted> {
        self.answer(NodeCall::AdmitAcceptance(request), self.admitted.clone())
    }

    fn remove_membership(&self, request: RemoveMembership) -> ServiceFuture<'_, Removed> {
        self.answer(NodeCall::RemoveMembership(request), self.removed.clone())
    }

    fn add_trust(&self, request: AddTrust) -> ServiceFuture<'_, Appended> {
        self.answer(NodeCall::AddTrust(request), self.trust_appended.clone())
    }

    fn revoke_trust(&self, event_id: Id, issuer: Id) -> ServiceFuture<'_, Revoked> {
        self.answer(
            NodeCall::RevokeTrust(event_id, issuer),
            self.revoked.clone(),
        )
    }

    fn push(&self, request: PushRequest) -> ServiceFuture<'_, Pushed> {
        self.answer(NodeCall::Push(request), self.pushed.clone())
    }

    fn forks(&self, query: ForkQuery) -> ServiceFuture<'_, ForkList> {
        self.answer(NodeCall::Forks(query), self.forks.clone())
    }
}

/// A lock that ignores poisoning: a panicking test must not turn every later
/// call into a second panic.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

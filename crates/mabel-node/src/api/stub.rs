//! Services that answer from the frozen fixtures.
//!
//! The stub exists so the UI (ticket 013) and the contract tests can run
//! against a real router before the witness and wallet runtimes (tickets 010
//! and 011) exist. It answers every route with the `response` document of the
//! matching file under `contracts/http/`, records the calls it received, and
//! can be told to fail with any [`ServiceError`].
//!
//! Because the fixtures are compiled in with `include_str!`, a fixture whose
//! shape drifts from [`super::documents`] fails the test suite instead of
//! drifting quietly.

use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::documents::{
    Accepted, Admitted, Appended, CreatedIdentity, ForkList, Id, Identity, IdentityList,
    IdentityView, Invited, LedgerList, LedgerPage, LedgerView, MembershipView, Pushed, Removed,
    Revoked, VerificationReport, WalletNode, WitnessNode,
};
use super::error::ServiceError;
use super::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, EventPageRequest, ForkQuery,
    Invite, PageRequest, PushRequest, RemoveMembership, ServiceFuture, VerifyRequest,
    WalletService, WitnessService,
};

/// One file under `contracts/http/`.
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    /// The file name, `wallet-get-node.json` and so on.
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
pub const FIXTURES: [Fixture; 20] = [
    fixture!("wallet-get-node"),
    fixture!("wallet-get-identities"),
    fixture!("wallet-post-identities"),
    fixture!("wallet-get-identity"),
    fixture!("wallet-get-identity-ledger"),
    fixture!("wallet-post-identity-witnesses"),
    fixture!("wallet-get-identity-memberships"),
    fixture!("wallet-post-membership-invitations"),
    fixture!("wallet-post-membership-acceptances"),
    fixture!("wallet-post-membership-admissions"),
    fixture!("wallet-post-membership-removals"),
    fixture!("wallet-post-trust"),
    fixture!("wallet-post-trust-revoke"),
    fixture!("wallet-post-sync-push"),
    fixture!("wallet-post-verify"),
    fixture!("witness-get-node"),
    fixture!("witness-get-ledgers"),
    fixture!("witness-get-ledger"),
    fixture!("witness-get-ledger-events"),
    fixture!("witness-get-forks"),
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

/// Which [`WalletService`] method was called, and with what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletCall {
    /// `GET /api/node`.
    Node,
    /// `GET /api/identities`.
    Identities,
    /// `POST /api/identities`.
    CreateIdentity(CreateIdentity),
    /// `GET /api/identities/{identity_id}`.
    Identity(Id),
    /// `GET /api/identities/{identity_id}/ledger`.
    IdentityLedger(Id, EventPageRequest),
    /// `POST /api/identities/{identity_id}/witnesses`.
    SetWitnesses(Id, Vec<Id>),
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
    /// `POST /api/verify`.
    Verify(VerifyRequest),
}

/// A [`WalletService`] that answers from `contracts/http/wallet-*.json`.
///
/// The document fields are public, so a test can change one answer and leave
/// the rest frozen.
#[derive(Debug)]
pub struct StubWalletService {
    /// `GET /api/node`.
    pub node: WalletNode,
    /// `GET /api/identities`.
    pub identities: Vec<Identity>,
    /// `POST /api/identities`.
    pub created_identity: CreatedIdentity,
    /// `GET /api/identities/{identity_id}`.
    pub identity: Identity,
    /// `GET /api/identities/{identity_id}/ledger`.
    pub identity_ledger: LedgerPage,
    /// `POST /api/identities/{identity_id}/witnesses`.
    pub witnesses_appended: Appended,
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
    /// `POST /api/verify`.
    pub report: VerificationReport,
    failure: Mutex<Option<ServiceError>>,
    calls: Mutex<Vec<WalletCall>>,
}

impl Default for StubWalletService {
    fn default() -> Self {
        Self::new()
    }
}

impl StubWalletService {
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
            node: Fixture::named("wallet-get-node.json").parse_response(),
            identities: identities.identities,
            created_identity: Fixture::named("wallet-post-identities.json").parse_response(),
            identity: identity.identity,
            identity_ledger: Fixture::named("wallet-get-identity-ledger.json").parse_response(),
            witnesses_appended: Fixture::named("wallet-post-identity-witnesses.json")
                .parse_response(),
            memberships: Fixture::named("wallet-get-identity-memberships.json").parse_response(),
            invited: Fixture::named("wallet-post-membership-invitations.json").parse_response(),
            accepted: Fixture::named("wallet-post-membership-acceptances.json").parse_response(),
            admitted: Fixture::named("wallet-post-membership-admissions.json").parse_response(),
            removed: Fixture::named("wallet-post-membership-removals.json").parse_response(),
            trust_appended: Fixture::named("wallet-post-trust.json").parse_response(),
            revoked: Fixture::named("wallet-post-trust-revoke.json").parse_response(),
            pushed: Fixture::named("wallet-post-sync-push.json").parse_response(),
            report: Fixture::named("wallet-post-verify.json").parse_response(),
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
    pub fn calls(&self) -> Vec<WalletCall> {
        lock(&self.calls).clone()
    }

    /// The one call this stub received.
    ///
    /// # Panics
    ///
    /// Panics unless exactly one call was made.
    #[must_use]
    pub fn call(&self) -> WalletCall {
        let calls = self.calls();
        assert_eq!(calls.len(), 1, "expected one call, got {calls:?}");
        calls.into_iter().next().expect("one call")
    }

    fn answer<T: Send + 'static>(&self, call: WalletCall, value: T) -> ServiceFuture<'_, T> {
        lock(&self.calls).push(call);
        let result = match lock(&self.failure).clone() {
            Some(error) => Err(error),
            None => Ok(value),
        };
        Box::pin(async move { result })
    }
}

impl WalletService for StubWalletService {
    fn node(&self) -> ServiceFuture<'_, WalletNode> {
        self.answer(WalletCall::Node, self.node.clone())
    }

    fn identities(&self) -> ServiceFuture<'_, Vec<Identity>> {
        self.answer(WalletCall::Identities, self.identities.clone())
    }

    fn create_identity(&self, request: CreateIdentity) -> ServiceFuture<'_, CreatedIdentity> {
        self.answer(
            WalletCall::CreateIdentity(request),
            self.created_identity.clone(),
        )
    }

    fn identity(&self, identity_id: Id) -> ServiceFuture<'_, Identity> {
        self.answer(WalletCall::Identity(identity_id), self.identity.clone())
    }

    fn identity_ledger(
        &self,
        identity_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage> {
        self.answer(
            WalletCall::IdentityLedger(identity_id, page),
            self.identity_ledger.clone(),
        )
    }

    fn set_witnesses(&self, identity_id: Id, witnesses: Vec<Id>) -> ServiceFuture<'_, Appended> {
        self.answer(
            WalletCall::SetWitnesses(identity_id, witnesses),
            self.witnesses_appended.clone(),
        )
    }

    fn memberships(&self, identity_id: Id) -> ServiceFuture<'_, MembershipView> {
        self.answer(
            WalletCall::Memberships(identity_id),
            self.memberships.clone(),
        )
    }

    fn invite(&self, request: Invite) -> ServiceFuture<'_, Invited> {
        self.answer(WalletCall::Invite(request), self.invited.clone())
    }

    fn accept_invitation(&self, request: AcceptInvitation) -> ServiceFuture<'_, Accepted> {
        self.answer(WalletCall::AcceptInvitation(request), self.accepted.clone())
    }

    fn admit_acceptance(&self, request: AdmitAcceptance) -> ServiceFuture<'_, Admitted> {
        self.answer(WalletCall::AdmitAcceptance(request), self.admitted.clone())
    }

    fn remove_membership(&self, request: RemoveMembership) -> ServiceFuture<'_, Removed> {
        self.answer(WalletCall::RemoveMembership(request), self.removed.clone())
    }

    fn add_trust(&self, request: AddTrust) -> ServiceFuture<'_, Appended> {
        self.answer(WalletCall::AddTrust(request), self.trust_appended.clone())
    }

    fn revoke_trust(&self, event_id: Id, issuer: Id) -> ServiceFuture<'_, Revoked> {
        self.answer(
            WalletCall::RevokeTrust(event_id, issuer),
            self.revoked.clone(),
        )
    }

    fn push(&self, request: PushRequest) -> ServiceFuture<'_, Pushed> {
        self.answer(WalletCall::Push(request), self.pushed.clone())
    }

    fn verify(&self, request: VerifyRequest) -> ServiceFuture<'_, VerificationReport> {
        self.answer(WalletCall::Verify(request), self.report.clone())
    }
}

/// Which [`WitnessService`] method was called, and with what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessCall {
    /// `GET /api/node`.
    Node,
    /// `GET /api/ledgers`.
    Ledgers(PageRequest),
    /// `GET /api/ledgers/{ledger_id}`.
    Ledger(Id),
    /// `GET /api/ledgers/{ledger_id}/events`.
    LedgerEvents(Id, EventPageRequest),
    /// `GET /api/forks`.
    Forks(ForkQuery),
}

/// A [`WitnessService`] that answers from `contracts/http/witness-*.json`.
#[derive(Debug)]
pub struct StubWitnessService {
    /// `GET /api/node`.
    pub node: WitnessNode,
    /// `GET /api/ledgers`.
    pub ledgers: LedgerList,
    /// `GET /api/ledgers/{ledger_id}`.
    pub ledger: LedgerView,
    /// `GET /api/ledgers/{ledger_id}/events`.
    pub events: LedgerPage,
    /// `GET /api/forks`.
    pub forks: ForkList,
    failure: Mutex<Option<ServiceError>>,
    calls: Mutex<Vec<WitnessCall>>,
}

impl Default for StubWitnessService {
    fn default() -> Self {
        Self::new()
    }
}

impl StubWitnessService {
    /// A stub primed from the frozen fixtures.
    ///
    /// # Panics
    ///
    /// Panics when a fixture does not parse into the document type that serves
    /// it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            node: Fixture::named("witness-get-node.json").parse_response(),
            ledgers: Fixture::named("witness-get-ledgers.json").parse_response(),
            ledger: Fixture::named("witness-get-ledger.json").parse_response(),
            events: Fixture::named("witness-get-ledger-events.json").parse_response(),
            forks: Fixture::named("witness-get-forks.json").parse_response(),
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
    pub fn calls(&self) -> Vec<WitnessCall> {
        lock(&self.calls).clone()
    }

    /// The one call this stub received.
    ///
    /// # Panics
    ///
    /// Panics unless exactly one call was made.
    #[must_use]
    pub fn call(&self) -> WitnessCall {
        let calls = self.calls();
        assert_eq!(calls.len(), 1, "expected one call, got {calls:?}");
        calls.into_iter().next().expect("one call")
    }

    fn answer<T: Send + 'static>(&self, call: WitnessCall, value: T) -> ServiceFuture<'_, T> {
        lock(&self.calls).push(call);
        let result = match lock(&self.failure).clone() {
            Some(error) => Err(error),
            None => Ok(value),
        };
        Box::pin(async move { result })
    }
}

impl WitnessService for StubWitnessService {
    fn node(&self) -> ServiceFuture<'_, WitnessNode> {
        self.answer(WitnessCall::Node, self.node.clone())
    }

    fn ledgers(&self, page: PageRequest) -> ServiceFuture<'_, LedgerList> {
        self.answer(WitnessCall::Ledgers(page), self.ledgers.clone())
    }

    fn ledger(&self, ledger_id: Id) -> ServiceFuture<'_, LedgerView> {
        self.answer(WitnessCall::Ledger(ledger_id), self.ledger.clone())
    }

    fn ledger_events(
        &self,
        ledger_id: Id,
        page: EventPageRequest,
    ) -> ServiceFuture<'_, LedgerPage> {
        self.answer(
            WitnessCall::LedgerEvents(ledger_id, page),
            self.events.clone(),
        )
    }

    fn forks(&self, query: ForkQuery) -> ServiceFuture<'_, ForkList> {
        self.answer(WitnessCall::Forks(query), self.forks.clone())
    }
}

/// A lock that ignores poisoning: a panicking test must not turn every later
/// call into a second panic.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

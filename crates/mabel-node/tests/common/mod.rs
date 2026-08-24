//! Fixtures the witness tests share: temp homes, signed chains and two
//! in-process Iroh endpoints.
//!
//! Both endpoints bind with relays disabled and dial the loopback
//! `EndpointAddr`, so no test here touches DNS, a relay or the internet
//! (proposal 001 section 11).

#![allow(dead_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use iroh::protocol::Router;
use iroh::{Endpoint, EndpointAddr, TransportAddr};
use iroh_base::{EndpointId, SecretKey};
use mabel_core::proto::DeclaredKind;
use mabel_core::sign::{
    BuiltEvent, Position, Root, build_endpoint_advertisement, build_inception,
    build_trust_attestation, build_trust_revocation, build_witness_set,
};
use mabel_core::{EventId, IdentityId, LedgerId};
use mabel_net::store::Provenance;
use mabel_net::{ALPN, Client, EndpointConfig, LedgerProtocol, RelayChoice, bind_endpoint};
use mabel_node::NewEvent;
use mabel_node::witness::{AdmissionPolicy, WitnessCaps, WitnessStorage, WitnessStore};
use mabel_node::{HomeOptions, NodeConfig, NodeHome, NodeRole, RelayMode};
use tempfile::TempDir;

/// Decision 013: a networked test never waits longer than this.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The timestamp the first event of every chain carries.
pub const T0: u64 = 1_700_000_000_000;

/// How far apart the events of a chain sit.
pub const STEP: u64 = 60_000;

/// Runs a test body under [`TIMEOUT`]. Reachable through `#[macro_use] mod
/// common;`.
#[allow(unused_macros)]
macro_rules! bounded {
    ($body:block) => {
        tokio::time::timeout(crate::common::TIMEOUT, async move $body)
            .await
            .expect("the test timed out")
    };
}

/// A signing key from one seed byte.
#[must_use]
pub fn secret(seed: u8) -> SecretKey {
    SecretKey::from_bytes(&[seed; 32])
}

/// An identity id from one seed byte, for an attestation subject.
#[must_use]
pub fn subject(seed: u8) -> IdentityId {
    IdentityId::from_bytes([seed; 32])
}

/// The key the witness identity's own chain is signed with.
const WITNESS_SEED: u8 = 0x77;

/// The witness identity every home here witnesses for, and the one a chain
/// names in its `WitnessSet` to be admitted (proposal 006 sections 1 and 4).
///
/// It is the ledger a chain keyed by [`WITNESS_SEED`] roots, so it is a real
/// identity id whose chain can carry an advertisement: proposal 006 section 4.1
/// admits a ledger this home does not store only while the local copy of this
/// identity advertises this home's endpoint. The id itself is deterministic,
/// since the inception fixes it and the advertisement that follows does not.
#[must_use]
pub fn witness_identity() -> IdentityId {
    Chain::new(WITNESS_SEED).ledger
}

/// The witness identity's own chain: its inception, a `WitnessSet` naming
/// itself, and one `EndpointAdvertisement` naming `endpoints`.
///
/// A witness identity names itself, which proposal 006 section 1 allows and
/// section 4 needs: a machine that holds this identity's id and nothing else is
/// admitted this chain under clause 3, and every later copy of it under clause
/// 2.
#[must_use]
pub fn witness_chain(endpoints: &[EndpointId]) -> Chain {
    let mut chain = Chain::new(WITNESS_SEED);
    chain.add_witness();
    let built = chain.advertisement(endpoints);
    chain.add(built);
    chain
}

/// A public key as every document spells it: lowercase base32, not the hex
/// `iroh_base` displays.
#[must_use]
pub fn rendered(key: &EndpointId) -> String {
    data_encoding::BASE32_NOPAD
        .encode(key.as_bytes())
        .to_ascii_lowercase()
}

/// A witness home in a temp directory, and the directory that owns it.
pub struct Home {
    /// The temp directory, dropped with this struct.
    pub dir: TempDir,
    /// The home.
    pub home: NodeHome,
}

impl Home {
    /// A witness home with relays disabled, `storage_capacity` bytes,
    /// [`witness_identity`] in `witness_for` and that identity's chain on disk
    /// advertising this home, which is what proposal 006 section 4.1 asks for
    /// before the home takes a ledger it does not store.
    #[must_use]
    pub fn new(storage_capacity: u64) -> Self {
        let home = Self::witnessing_for(storage_capacity, vec![witness_identity()]);
        home.advertise(&[home.endpoint_id()]);
        home
    }

    /// A witness home whose `witness_for` is exactly `witness_for` and which
    /// holds no copy of any witness identity: how a test asks for a home that
    /// witnesses for nobody, or for one whose advertisement has not landed.
    #[must_use]
    pub fn witnessing_for(storage_capacity: u64, witness_for: Vec<IdentityId>) -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let home = create(dir.path(), storage_capacity, witness_for);
        Self { dir, home }
    }

    /// Writes the witness identity's own chain into this home, advertising
    /// `endpoints`.
    ///
    /// This is what a fleet machine holds: a copy of the witness identity's
    /// ledger, no key of it. The events are written straight to `ledgers/`,
    /// since a home that has not stored the advertisement yet cannot be pushed
    /// the chain that carries it.
    pub fn advertise(&self, endpoints: &[EndpointId]) {
        let chain = witness_chain(endpoints);
        let store = self.home.ledger(chain.ledger);
        let events: Vec<NewEvent<'_>> = chain
            .events
            .iter()
            .enumerate()
            .map(|(seq, bytes)| NewEvent {
                seq: seq as u64,
                event_id: mabel_net::wire::signed_event_id(bytes).expect("an event has an id"),
                bytes,
            })
            .collect();
        store.append(&events).expect("the witness chain is written");
    }

    /// Bytes the witness identity's own chain takes in this home, which is
    /// what every ledger count and byte total here starts from.
    #[must_use]
    pub fn stored_bytes(&self) -> u64 {
        self.home
            .ledger(witness_identity())
            .read_all()
            .expect("the events read")
            .iter()
            .map(|event| event.bytes.len() as u64)
            .sum()
    }

    /// Rewrites `node.json`'s `storage_capacity`, which a cap test sets from
    /// what the home already holds.
    pub fn set_storage_capacity(&self, bytes: u64) {
        let mut config = self.home.config().expect("node.json reads");
        config.storage_capacity = bytes;
        self.home
            .write_config(&config)
            .expect("node.json is written");
    }

    /// This home's Iroh endpoint id, which is what admission checks for.
    #[must_use]
    pub fn endpoint_id(&self) -> EndpointId {
        self.home.node_key().expect("the node key reads").public()
    }

    /// The identities this home witnesses for, as `node.json` records them.
    #[must_use]
    pub fn witness_for(&self) -> Vec<IdentityId> {
        self.home.config().expect("node.json reads").witness_for
    }

    /// Storage over this home with `caps`, witnessing for what `node.json`
    /// names.
    #[must_use]
    pub fn storage(&self, caps: WitnessCaps) -> Arc<WitnessStorage> {
        self.storage_with(caps, AdmissionPolicy::witnessing_for(self.witness_for()))
    }

    /// Storage over this home with `caps` and an explicit admission policy,
    /// which is how a test turns the retired tag-11 clause on.
    #[must_use]
    pub fn storage_with(&self, caps: WitnessCaps, policy: AdmissionPolicy) -> Arc<WitnessStorage> {
        Arc::new(
            WitnessStorage::open(self.home.clone(), self.endpoint_id(), caps, policy)
                .expect("the index builds"),
        )
    }
}

fn create(root: &Path, storage_capacity: u64, witness_for: Vec<IdentityId>) -> NodeHome {
    let config = NodeConfig {
        role: NodeRole::Witness,
        http_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        witnesses: Vec::new(),
        witness_for,
        storage_capacity,
        relay: RelayMode::Disabled,
        ..NodeConfig::default()
    };
    NodeHome::create(root, &config, HomeOptions::default()).expect("the home is created")
}

/// A witness home with the default storage capacity.
#[must_use]
pub fn home() -> Home {
    Home::new(mabel_node::DEFAULT_STORAGE_CAPACITY)
}

/// A home that witnesses for nobody, which refuses every stranger's push.
#[must_use]
pub fn home_witnessing_for_nobody() -> Home {
    Home::witnessing_for(mabel_node::DEFAULT_STORAGE_CAPACITY, Vec::new())
}

/// Provenance for a push that arrived from `seed`'s endpoint.
#[must_use]
pub fn from_endpoint(seed: u8) -> Provenance {
    Provenance::from_endpoint(secret(seed).public())
}

/// A chain under construction, signed by one key.
pub struct Chain {
    /// The ledger, which is the id of its seq-0 event.
    pub ledger: LedgerId,
    /// The events built so far, in order.
    pub events: Vec<Vec<u8>>,
    signer: SecretKey,
    prev: EventId,
    seq: u64,
    prev_ms: u64,
}

impl Chain {
    /// A raw-rooted chain holding its inception, keyed by `secret(seed)`.
    #[must_use]
    pub fn new(seed: u8) -> Self {
        let signer = secret(seed);
        let inception = build_inception(
            &signer,
            DeclaredKind::Person,
            Root::Raw {
                reserve_key: &secret(seed.wrapping_add(128)).public(),
            },
            [seed; 16],
            T0,
        )
        .expect("the inception builds");
        Self {
            ledger: inception.event_id.into(),
            events: vec![inception.signed_event],
            signer,
            prev: inception.event_id,
            seq: 1,
            prev_ms: T0,
        }
    }

    /// Where the next event goes.
    #[must_use]
    pub fn at(&self) -> Position {
        Position {
            ledger: self.ledger,
            seq: self.seq,
            prev: self.prev,
            prev_timestamp_ms: self.prev_ms,
        }
    }

    /// The timestamp the next event carries.
    #[must_use]
    pub fn now(&self) -> u64 {
        T0 + self.seq * STEP
    }

    /// The position of the last event built.
    #[must_use]
    pub fn head_seq(&self) -> u64 {
        self.seq - 1
    }

    /// A witness set for the next position, not added to the chain.
    #[must_use]
    pub fn witness_set(&self, witnesses: &[IdentityId]) -> BuiltEvent {
        build_witness_set(&self.signer, &self.at(), witnesses, self.now())
            .expect("the witness set builds")
    }

    /// An endpoint advertisement for the next position, not added to the chain.
    #[must_use]
    pub fn advertisement(&self, endpoints: &[EndpointId]) -> BuiltEvent {
        build_endpoint_advertisement(&self.signer, &self.at(), endpoints, self.now())
            .expect("the advertisement builds")
    }

    /// A retired tag-11 `WitnessConfig` for the next position, not added to the
    /// chain.
    ///
    /// The one caller of the retired signing path outside the vector tests: it
    /// is how a test builds a chain from before witnesses were identities, which
    /// is what the legacy clause of proposal 006 section 4 admits.
    #[must_use]
    pub fn witness_config(&self, endpoints: &[EndpointId]) -> BuiltEvent {
        mabel_core::sign::build_witness_config(&self.signer, &self.at(), endpoints, self.now())
            .expect("the witness config builds")
    }

    /// An attestation for the next position, not added to the chain.
    #[must_use]
    pub fn attestation(&self, seed: u8) -> BuiltEvent {
        build_trust_attestation(&self.signer, &self.at(), subject(seed), self.now())
            .expect("the attestation builds")
    }

    /// A revocation for the next position, not added to the chain.
    #[must_use]
    pub fn revocation(&self, target: EventId) -> BuiltEvent {
        build_trust_revocation(&self.signer, &self.at(), target, self.now())
            .expect("the revocation builds")
    }

    /// An attestation for the next position signed by a key this ledger never
    /// authorized, which no fold accepts.
    #[must_use]
    pub fn forged(&self, seed: u8) -> BuiltEvent {
        build_trust_attestation(&secret(200), &self.at(), subject(seed), self.now())
            .expect("the attestation builds")
    }

    /// Adds a built event at the chain's next position.
    pub fn add(&mut self, built: BuiltEvent) -> EventId {
        self.events.push(built.signed_event);
        self.prev = built.event_id;
        self.prev_ms = self.now();
        self.seq += 1;
        built.event_id
    }

    /// Adds a witness set naming `witnesses`.
    pub fn add_witness_set(&mut self, witnesses: &[IdentityId]) -> EventId {
        let built = self.witness_set(witnesses);
        self.add(built)
    }

    /// Adds a witness set naming the one witness identity these tests use,
    /// which is what admits a push to a home built by [`home`].
    pub fn add_witness(&mut self) -> EventId {
        self.add_witness_set(&[witness_identity()])
    }

    /// Adds a tag-11 `WitnessConfig` naming `endpoints`.
    pub fn add_witness_config(&mut self, endpoints: &[EndpointId]) -> EventId {
        let built = self.witness_config(endpoints);
        self.add(built)
    }

    /// Adds an attestation naming `subject(seed)`.
    pub fn add_attestation(&mut self, seed: u8) -> EventId {
        let built = self.attestation(seed);
        self.add(built)
    }

    /// Adds a revocation of `target`.
    pub fn add_revocation(&mut self, target: EventId) -> EventId {
        let built = self.revocation(target);
        self.add(built)
    }

    /// Every event built so far.
    #[must_use]
    pub fn all(&self) -> Vec<Vec<u8>> {
        self.events.clone()
    }

    /// The events from `since` inclusive.
    #[must_use]
    pub fn from(&self, since: usize) -> Vec<Vec<u8>> {
        self.events[since..].to_vec()
    }

    /// The events in `range`.
    #[must_use]
    pub fn slice(&self, range: std::ops::Range<usize>) -> Vec<Vec<u8>> {
        self.events[range].to_vec()
    }
}

/// A witness serving `mabel/ledger/0` on a loopback endpoint.
pub struct Served {
    /// The home, whose temp directory outlives the router.
    pub home: Home,
    /// The storage both surfaces share.
    pub storage: Arc<WitnessStorage>,
    /// The endpoint id a chain must name to be admitted.
    pub endpoint_id: EndpointId,
    /// Where a client dials.
    pub addr: EndpointAddr,
    router: Router,
}

impl Served {
    /// A witness with `caps` over a fresh home.
    pub async fn start(caps: WitnessCaps) -> Self {
        Self::over(home(), caps).await
    }

    /// A witness with the section 5 caps over a fresh home.
    pub async fn new() -> Self {
        Self::start(WitnessCaps::default()).await
    }

    /// A witness over `home`, whose node key is the endpoint's key.
    pub async fn over(home: Home, caps: WitnessCaps) -> Self {
        let key = home.home.node_key().expect("the node key reads");
        let storage = home.storage(caps);
        let endpoint =
            bind_endpoint(EndpointConfig::new(RelayChoice::Disabled).with_secret_key(key.clone()))
                .await
                .expect("the endpoint binds")
                .endpoint;
        let addr = loopback_addr(&endpoint);
        let router = Router::builder(endpoint)
            .accept(
                ALPN,
                LedgerProtocol::new(Arc::new(WitnessStore::new(storage.clone()))),
            )
            .spawn();
        Self {
            endpoint_id: key.public(),
            home,
            storage,
            addr,
            router,
        }
    }

    /// A client on its own endpoint, dialling this witness.
    pub async fn dial(&self) -> Peer {
        let endpoint = bind_endpoint(EndpointConfig::new(RelayChoice::Disabled))
            .await
            .expect("the endpoint binds")
            .endpoint;
        let client = Client::connect(&endpoint, self.addr.clone())
            .await
            .expect("the client connects");
        Peer { endpoint, client }
    }

    /// Shuts the sync server down.
    pub async fn stop(self) {
        let _ = self.router.shutdown().await;
    }
}

/// A client and the endpoint that owns its connection.
pub struct Peer {
    /// The endpoint, which must outlive the client.
    pub endpoint: Endpoint,
    /// The connected client.
    pub client: Client,
}

/// The endpoint's loopback address, built without any address lookup.
fn loopback_addr(endpoint: &Endpoint) -> EndpointAddr {
    addr_of(endpoint.id(), &endpoint.bound_sockets())
}

/// A dialable address for an endpoint whose sockets are known, with an
/// unspecified bind read as loopback.
#[must_use]
pub fn addr_of(id: EndpointId, sockets: &[SocketAddr]) -> EndpointAddr {
    let addrs = sockets
        .iter()
        .copied()
        .filter(|socket| socket.is_ipv4())
        .map(|socket| {
            let ip = if socket.ip().is_unspecified() {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            } else {
                socket.ip()
            };
            TransportAddr::Ip(SocketAddr::new(ip, socket.port()))
        })
        .collect();
    EndpointAddr { id, addrs }
}

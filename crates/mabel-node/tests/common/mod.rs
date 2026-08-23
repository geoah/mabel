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
    BuiltEvent, Position, Root, build_inception, build_trust_attestation, build_trust_revocation,
    build_witness_config,
};
use mabel_core::{EventId, IdentityId, LedgerId};
use mabel_net::store::Provenance;
use mabel_net::{ALPN, Client, EndpointConfig, LedgerProtocol, RelayChoice, bind_endpoint};
use mabel_node::witness::{WitnessCaps, WitnessStorage, WitnessStore};
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
    /// A witness home with relays disabled and `storage_capacity` bytes.
    #[must_use]
    pub fn new(storage_capacity: u64) -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let home = create(dir.path(), storage_capacity);
        Self { dir, home }
    }

    /// This home's Iroh endpoint id, which is what admission checks for.
    #[must_use]
    pub fn endpoint_id(&self) -> EndpointId {
        self.home.node_key().expect("the node key reads").public()
    }

    /// Storage over this home with `caps`.
    #[must_use]
    pub fn storage(&self, caps: WitnessCaps) -> Arc<WitnessStorage> {
        Arc::new(
            WitnessStorage::open(self.home.clone(), self.endpoint_id(), caps)
                .expect("the index builds"),
        )
    }
}

fn create(root: &Path, storage_capacity: u64) -> NodeHome {
    let config = NodeConfig {
        role: NodeRole::Witness,
        http_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        witnesses: Vec::new(),
        storage_capacity,
        relay: RelayMode::Disabled,
    };
    NodeHome::create(root, &config, HomeOptions::default()).expect("the home is created")
}

/// A witness home with the default storage capacity.
#[must_use]
pub fn home() -> Home {
    Home::new(mabel_node::DEFAULT_STORAGE_CAPACITY)
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

    /// A witness config for the next position, not added to the chain.
    #[must_use]
    pub fn witness_config(&self, witnesses: &[EndpointId]) -> BuiltEvent {
        build_witness_config(&self.signer, &self.at(), witnesses, self.now())
            .expect("the config builds")
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

    /// Adds a witness config naming `witnesses`.
    pub fn add_witness_config(&mut self, witnesses: &[EndpointId]) -> EventId {
        let built = self.witness_config(witnesses);
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

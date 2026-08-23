# Iroh research for mabel

Researched 2026-08-23 against crates.io, docs.rs and the n0-computer GitHub
repos. Everything below is from the current published sources, not from prior
knowledge.

## 1. Versions

iroh 1.0 has shipped. `iroh 1.0.0` was published on 2026-06-15 after 65
pre-releases. The latest patch is **1.0.3** (2026-07-20). Version history:
0.97.0 (2026-03-16), 0.98.0 (2026-04-17), 0.98.2 (2026-04-28), 1.0.0-rc.0
(2026-05-07), 1.0.0-rc.1 (2026-05-27), 1.0.0 (2026-06-15), 1.0.1, 1.0.2, 1.0.3.

n0 promises wire-protocol stability across all v1 endpoints and across language
bindings. The 0.35 line keeps public relay support until 2026-12-31; everything
older is on its own.

Companion crates, all released on 2026-06-15 alongside 1.0.0 and all depending
on `iroh = "1"`:

| crate | version | depends on | repo activity |
| --- | --- | --- | --- |
| `iroh-base` | 1.0.3 | (core types) | in the iroh repo |
| `iroh-tickets` | 1.0.0 | iroh-base 1 | in the iroh repo |
| `iroh-blobs` | 0.103.0 | iroh ^1.0.0 | pushed 2026-08-03 |
| `iroh-gossip` | 0.101.0 | iroh ^1 | pushed 2026-08-03 |
| `iroh-docs` | 0.101.0 | iroh ^1, iroh-blobs 0.103, iroh-gossip 0.101 | pushed 2026-08-19 |
| `iroh-mdns-address-lookup` | 0.5.0 (2026-08-18) | iroh ^1.0.0 | pushed 2026-08 |
| `iroh-mainline-address-lookup` | same repo | iroh ^1.0.0 | DHT lookup |

`iroh-docs` is **not** deprecated and **not** archived. It was rebased onto iroh
1.0, and the repo had commits four days ago. It is still on a 0.x version, so it
does not carry iroh's 1.0 stability promise. Same for blobs and gossip: the
core is 1.0, the protocol crates are not.

## 2. Renames you must know about

The 0.9x line renamed almost every identifier that older tutorials use. From the
changelog:

- `Node` to `Endpoint` "in all cases" (#3542). So **`NodeId` is now
  `EndpointId`**, **`NodeAddr` is now `EndpointAddr`**, `NodeTicket` is now
  `EndpointTicket`.
- `Discovery` trait renamed to `AddressLookup` (#3853), and the `iroh::discovery`
  module is now **`iroh::address_lookup`**. `MdnsDiscovery` and `DhtDiscovery`
  moved out into the separate `iroh-address-lookups` repo.
- `ProtocolError` renamed to `AcceptError` (#3339).
- `EndpointAddr` became transport-generic (#3554): it is now
  `{ id: PublicKey, addrs: Set<TransportAddr> }` where
  `TransportAddr` is `Relay(RelayUrl) | Ip(SocketAddr) | Custom(CustomAddr)`.
  Build with `EndpointAddr::from_parts(pk, [TransportAddr::Ip(..)])`, not with
  struct literals.
- The QUIC implementation is no longer quinn. iroh 1.x uses **`noq`** (n0's own
  QUIC, 1.1.0), re-exported through `iroh::endpoint`. Stream and connection APIs
  are shaped like quinn's, but `quinn::` types are gone from the public surface.
- Errors use **`n0-error`** (1.0.0), not anyhow. `n0_error::Result` and
  `AcceptError::from_err` show up in every example. You can still use anyhow in
  your own code; `iroh-ping` does exactly that.
- Endpoint construction now goes through **presets**: `Endpoint::bind(preset)` /
  `Endpoint::builder(preset)`. There is no zero-arg `Endpoint::builder()`.

MSRV is Rust 1.91, edition 2024.

## 3. Endpoint, presets and defaults

```rust
pub fn builder(preset: impl Preset) -> Builder
pub async fn bind(preset: impl Preset) -> Result<Self, BindError>
pub async fn connect(&self, endpoint_addr: impl Into<EndpointAddr>, alpn: &[u8])
    -> Result<Connection, ConnectError>
pub fn accept(&self) -> Accept<'_>
pub fn id(&self) -> EndpointId
pub fn addr(&self) -> EndpointAddr
pub fn watch_addr(&self) -> impl Watcher<Value = EndpointAddr>
pub async fn online(&self)
pub async fn close(&self)
```

Presets in `iroh::endpoint::presets`:

- `Empty`: sets nothing. `bind` always fails with it, because the crypto
  provider is mandatory.
- `Minimal`: only sets the rustls crypto provider. No relays, no lookup.
- `N0`: the crypto provider, n0's default relays, plus a `PkarrPublisher`, a
  `PkarrResolver` and (outside browsers) a `DnsAddressLookup`, all pointed at
  n0's `iroh.link` DNS server. This is what you want for mabel.
- `N0DisableRelay`: N0 minus relays.

Note that `N0` publishes and resolves. Under test cfg or with the `test-utils`
feature it silently swaps to the staging pkarr relay, which matters if you ever
mix test and prod binaries.

Crate default features: `["metrics", "fast-apple-datapath", "portmapper",
"tls-ring"]`. Relevant extras: `test-utils` (local relay + DNS server),
`tls-aws-lc-rs`, `qlog`. `default-features = false` means you must pick a TLS
backend and cannot use `presets::N0` or `Minimal` (they are gated on
`with_crypto_provider`).

Builder methods you will use: `secret_key(SecretKey)`, `alpns(Vec<Vec<u8>>)`,
`relay_mode(RelayMode)` where `RelayMode` is `Disabled | Default |
Custom(RelayMap)`, `address_lookup(impl AddressLookupBuilder)` (callable
repeatedly, all services are queried in parallel), `clear_address_lookup()`,
`addr_filter(AddrFilter)`, `bind_addr(addr)`, `transport_config(..)`, and
`bind()`.

Address lookup services live in `iroh::address_lookup`: `MemoryLookup`,
`DnsAddressLookup`, `PkarrPublisher`, `PkarrResolver`. mDNS and Mainline DHT are
external crates.

## 4. Router and ProtocolHandler

`iroh::protocol` gives you `Router`, `RouterBuilder`, `ProtocolHandler`,
`AcceptError`, and `IncomingFilter`. One handler per ALPN. The returned future
from `accept` runs on its own tokio task, so it can live as long as the
connection.

Verbatim shape from `iroh/examples/echo.rs` on main:

```rust
let router = Router::builder(endpoint).accept(ALPN, Echo).spawn();
router.endpoint().online().await;      // wait until relay/addr are usable
let addr = router.endpoint().addr();
router.shutdown().await?;

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();          // authenticated peer
        let (mut send, mut recv) = connection.accept_bi().await?;
        tokio::io::copy(&mut recv, &mut send).await?;
        send.finish()?;
        connection.closed().await;
        Ok(())
    }
}
```

`connection.remote_id()` is the peer's `EndpointId`, authenticated by the QUIC
TLS handshake. For mabel this is free authentication of the caller: a witness
can check the pushing peer's `EndpointId` against the ledger's owner key without
any extra challenge/response, provided the wallet's iroh secret key is the same
ed25519 key as the ledger identity (iroh keys are ed25519, so this is possible).

## 5. Recommended pattern for mabel's protocol

Use a plain custom protocol over `open_bi`, one request per stream, postcard for
framing. Reasons in section 7.

`Cargo.toml`:

```toml
[dependencies]
iroh = "1.0.3"
iroh-base = "1.0.3"
iroh-tickets = "1.0.0"
n0-error = "1.0.0"
postcard = { version = "1.1.1", features = ["use-std"] }
serde = { version = "1.0.219", features = ["derive"] }
ed25519-dalek = { version = "3.0.0-rc.0", features = ["serde", "rand_core"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1.0"
tracing = "0.1"

[dev-dependencies]
iroh = { version = "1.0.3", features = ["test-utils"] }
tracing-subscriber = "0.3"
```

Note `ed25519-dalek`: iroh 1.0.3 pins `>=3.0.0-rc.0,<4.0.0`. If mabel signs
ledger events with dalek directly, match that range or you will get two
incompatible `SigningKey` types in the dependency graph. Simpler option: reuse
`iroh_base::SecretKey` / `PublicKey` for the ledger identity and never depend on
dalek yourself.

Sketch:

```rust
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::{Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"mabel/ledger/0";

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    /// Ask for every event at or after `since` in the ledger for `ledger_id`.
    Get { ledger_id: [u8; 32], since: u64 },
    /// Offer events to a witness. The witness verifies before storing.
    Push { ledger_id: [u8; 32], events: Vec<Event> },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Events(Vec<Event>),
    Accepted { head_seq: u64 },
    NotFound,
    Rejected(String),
}

/// Write a postcard value with a 4-byte little-endian length prefix.
async fn write_msg<T: Serialize>(
    send: &mut iroh::endpoint::SendStream,
    msg: &T,
) -> anyhow::Result<()> {
    let bytes = postcard::to_stdvec(msg)?;
    send.write_all(&(bytes.len() as u32).to_le_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

async fn read_msg<T: for<'de> Deserialize<'de>>(
    recv: &mut iroh::endpoint::RecvStream,
    max: usize,
) -> anyhow::Result<T> {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await?;
    let len = u32::from_le_bytes(len) as usize;
    anyhow::ensure!(len <= max, "message too large: {len}");
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    Ok(postcard::from_bytes(&buf)?)
}
```

Because mabel's ledgers are kilobytes, you can skip the length prefix entirely
and lean on QUIC stream termination instead, which is what the echo and ping
examples do: the client writes the request, calls `send.finish()`, and the
server reads to EOF with a byte cap.

```rust
// client side, simplest possible: one request, one response, one stream
async fn fetch(ep: &Endpoint, at: impl Into<EndpointAddr>, req: &Request)
    -> anyhow::Result<Response>
{
    let conn = ep.connect(at, ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(&postcard::to_stdvec(req)?).await?;
    send.finish()?;                              // EOF frames the request
    let bytes = recv.read_to_end(1024 * 1024).await?;   // hard cap
    conn.close(0u32.into(), b"done");
    Ok(postcard::from_bytes(&bytes)?)
}
```

Server side:

```rust
#[derive(Debug, Clone)]
pub struct LedgerProto { store: Store }

impl ProtocolHandler for LedgerProto {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let peer: EndpointId = connection.remote_id();
        // Loop, so a wallet can push several ledgers over one connection.
        while let Ok((mut send, mut recv)) = connection.accept_bi().await {
            let bytes = recv.read_to_end(1024 * 1024)
                .await.map_err(AcceptError::from_err)?;
            let req: Request = postcard::from_bytes(&bytes)
                .map_err(AcceptError::from_err)?;
            let resp = self.store.handle(peer, req).await;
            send.write_all(&postcard::to_stdvec(&resp).unwrap())
                .await.map_err(AcceptError::from_err)?;
            send.finish()?;
        }
        connection.closed().await;
        Ok(())
    }
}

// wiring
let ep = Endpoint::builder(presets::N0)
    .secret_key(secret_key)          // persist this: it is the node's identity
    .bind()
    .await?;
let router = Router::builder(ep).accept(ALPN, LedgerProto { store }).spawn();
router.endpoint().online().await;
println!("witness id: {}", router.endpoint().id());
```

Keep the wallet's `SecretKey` on disk. The `EndpointId` is derived from it, and
witnesses are configured by `EndpointId`, so a regenerated key means a new
node identity.

## 6. Addressing, tickets and dialing by id alone

`Endpoint::connect` takes `impl Into<EndpointAddr>`, and `EndpointId` implements
`Into<EndpointAddr>`. So **yes, with `presets::N0` you can dial by
`EndpointId` alone**: the N0 preset installs `PkarrPublisher` (the endpoint
publishes its own relay URL and addresses to n0's pkarr/DNS server at
`iroh.link`) plus `DnsAddressLookup` and `PkarrResolver` on the resolving side.
The only out-of-band data mabel needs is the 32-byte `EndpointId`, which
base32-encodes to a 52-character string via `Display`/`FromStr`.

That is the ideal shape for mabel: a witness list in config is just a list of
`EndpointId` strings, and `LedgerId` -> owner `EndpointId` needs no address
plumbing at all.

Tickets are for when you want to bundle addresses too, for example a witness on
a LAN with no DNS publication. `iroh-tickets` 1.0.0 gives `EndpointTicket`:

```rust
use iroh_tickets::{Ticket, endpoint::EndpointTicket};
use iroh_base::{EndpointAddr, TransportAddr};

let ticket = EndpointTicket::new(endpoint.addr());
let s = ticket.encode_string();     // "endpoint" + base32 payload
let back: EndpointTicket = s.parse()?;
let addr: EndpointAddr = back.into();
```

For statically configured addresses that iroh should treat as discovery results,
use `address_lookup::memory::MemoryLookup`: `MemoryLookup::new()`, then
`add_endpoint_info(addr)` / `remove_endpoint_info(id)` at runtime, and register
it with `.address_lookup(lookup.clone())` on the builder. iroh 1.x has no
`Endpoint::add_node_addr`; `MemoryLookup` replaced it.

## 7. Recommendation: custom protocol, not gossip, not blobs

Use a custom ALPN over `open_bi`. Concretely:

- **Not iroh-blobs.** Blobs is a BLAKE3 verified-streaming stack (bao-tree,
  redb, irpc) built for large content-addressed data. For a few KB it is pure
  overhead, and content addressing is the wrong primitive anyway: a ledger has a
  mutable head, so a fetch by content hash cannot express "give me everything
  after seq N". You would end up building a pointer layer on top. It also drags
  in ~12 extra dependencies and is still 0.x.
- **Not iroh-gossip.** Gossip is epidemic broadcast over a topic mesh. mabel's
  topology is known and small: a wallet knows its witnesses, a reader knows the
  owner or a witness. Gossip gives you unordered best-effort delivery to a
  membership set you would have to bootstrap, plus a message size limit, plus no
  answer for "fetch the whole history of ledger X on demand", which is the main
  read path. If you later want push notification of new heads to many watchers,
  gossip becomes worth reconsidering as an addition, not a replacement.
- **Custom protocol wins** because the ledger is already an authenticated,
  hash-chained, self-verifying structure. The transport only needs to move bytes
  and tell you who the peer is; iroh's `connection.remote_id()` does the latter
  for free. Request/response over one bi-stream is 100 lines of code, and the
  verification you must write anyway (chain, signatures) is what actually
  provides integrity, so there is nothing for blobs or gossip to add.

Design notes for the protocol itself: make `Get { since }` incremental from the
start so a witness push is cheap on the second run; have the witness respond to
`Push` with its current `head_seq` so the wallet can compute the delta; and
verify every event server-side before storing, since witnesses accept from
anyone.

## 8. Docker and local testing

Docker. The `docker/` directory in the iroh repo only ships images for
`iroh-relay` and `iroh-dns-server`; there is no image for an application
endpoint, so mabel builds its own. Practical points for containerized endpoints:

- The endpoint binds a UDP socket. Publish it with `-p <port>/udp` and pass
  `.bind_addr(...)` with a fixed port so the mapping is stable, otherwise iroh
  picks an ephemeral port each start.
- Under default bridge networking, NAT traversal from inside a container is
  unreliable and iroh will usually fall back to the relay. That works and costs
  latency. `--network host` gives direct-path hole punching a real chance; use
  it for witnesses you actually care about the throughput of.
- Disable `portmapper` (a default feature) if the container has no gateway to
  probe; it just wastes startup time.
- Persist the secret key on a volume. A fresh key on every container start
  changes the `EndpointId` and invalidates every witness config referencing it.
- Relays are reached over HTTPS/QUIC outbound only, so no inbound ports are
  required for the relay path to work.

Local testing. Multiple endpoints in one process is the normal pattern and the
one n0's own tests use: `iroh-ping`'s test binds a server endpoint and a client
endpoint in the same `#[tokio::test]`, spawns a `Router` on the server, and
dials `server_router.endpoint().addr()` directly. No mock layer.

Three levels, pick per test:

1. **Fully offline, no network at all.** Bind both endpoints with
   `presets::Minimal` and `.relay_mode(RelayMode::Disabled)`, then dial with the
   full `EndpointAddr` from `endpoint.addr()`, which contains the loopback
   `TransportAddr::Ip`. Fast, deterministic, no DNS or relay involved. Use this
   for protocol-level tests.
2. **Discovery in the loop but still local.** Enable the `test-utils` feature and
   use `iroh::test_utils::run_relay_server()` plus `DnsPkarrServer`, both of
   which are drop-guarded, and point the builder at them with
   `RelayMode::Custom(relay_map)` and a `PkarrPublisher` / `DnsAddressLookup`
   aimed at the test server. This exercises the dial-by-`EndpointId` path.
3. **Real LAN.** Add `iroh-mdns-address-lookup = "0.5.0"` for multicast
   discovery of endpoints on the same network, no relay and no DNS needed. Good
   for a manual two-laptop demo of wallet plus witness.

There is no in-memory transport in iroh 1.x. Loopback UDP with relays disabled is
the substitute, and it is fast enough for unit tests.

## Sources

- Crate metadata: https://crates.io/crates/iroh, /iroh-blobs, /iroh-gossip, /iroh-docs, /iroh-tickets, /iroh-mdns-address-lookup
- API docs: https://docs.rs/iroh/latest/iroh/ , /iroh/protocol/index.html , /iroh/endpoint/struct.Endpoint.html , /iroh/endpoint/struct.Builder.html , /iroh/endpoint/presets/index.html , /iroh/address_lookup/index.html , /iroh/test_utils/index.html
- https://docs.rs/iroh-tickets/latest/iroh_tickets/
- Echo example: https://github.com/n0-computer/iroh/blob/main/iroh/examples/echo.rs
- Quickstart protocol: https://github.com/n0-computer/iroh-ping/blob/main/src/lib.rs
- Manifest and features: https://github.com/n0-computer/iroh/blob/main/iroh/Cargo.toml
- Presets source: https://github.com/n0-computer/iroh/blob/main/iroh/src/endpoint/presets.rs
- MemoryLookup source: https://github.com/n0-computer/iroh/blob/main/iroh/src/address_lookup/memory.rs
- Changelog (renames): https://github.com/n0-computer/iroh/blob/main/CHANGELOG.md
- Docker images: https://github.com/n0-computer/iroh/blob/main/docker/README.md
- Address lookup crates: https://github.com/n0-computer/iroh-address-lookups
- 1.0-rc.0 announcement: https://www.iroh.computer/blog/iroh-1-0-0-rc-0
- Docs site: https://docs.iroh.computer/

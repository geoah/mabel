# Containers

One image runs every node, and `compose.yaml` brings up the topology of
proposal 001 section 11: one witness and two wallets, alice and bob, on one
bridge network, with the witness's `EndpointTicket` seeded into both wallets.
Nothing here contacts the internet at run time (ticket 015).

## Build and run

```sh
docker build -f docker/Dockerfile -t mabel:dev .          # from the repo root
docker compose -f docker/compose.yaml up --build -d       # all healthy in ~12s
docker/smoke.sh                                           # the scripted check
docker compose -f docker/compose.yaml down -v             # -v drops the homes
```

`docker/smoke.sh` needs `curl` and `jq` on the host. It creates an identity in
alice, names the witness in her ledger, pushes, reads the ledger back from the
witness API and has bob verify it through the witness.

`demo/run-demo.sh` walks the same topology through the whole product story with
the CLI in about 17 seconds: identities, membership, trust, revocation and a
stranger verifying from an empty home. See [demo/README.md](../demo/README.md).

The image is one build, three stages: `node:22-bookworm` builds `ui/` into
`ui/dist`, `rust:1.98-bookworm` builds the release binary with `ui/dist` in
place so `rust-embed` compiles the bundle in, and `debian:bookworm-slim` carries
the `mabel` binary, a non-root user (uid 10001), `/data` as `MABEL_HOME` and the
two scripts. The runtime stage installs no packages: the entrypoint and the
healthcheck use bash, coreutils and `getent`.

## Ports

Every HTTP port is published as host port == container port, and the reason is
the API's loopback rules (proposal 001 section 10): a request is refused with
403 unless its `Host` is `127.0.0.1:<the port the API is bound to>` or
`localhost:<that port>`. `curl http://127.0.0.1:9081/api/node` from the host
sends `Host: 127.0.0.1:9081`, which matches only because the container also
binds 9081. Publishing `9181:9081` would make every request from the host a 403.
The healthcheck sends the same header from inside the container, so a mapping
that breaks the rule shows up as an unhealthy service.

| Service     | Role     | HTTP and UI      | Iroh UDP   | Volume             |
| ----------- | -------- | ---------------- | ---------- | ------------------ |
| witness     | witness  | `127.0.0.1:9080` | `9070/udp` | `witness-data`     |
| alice       | wallet   | `127.0.0.1:9081` | `9071/udp` | `alice-data`       |
| bob         | wallet   | `127.0.0.1:9082` | `9072/udp` | `bob-data`         |
| witness-two | witness  | `127.0.0.1:9083` | `9073/udp` | `witness-two-data` |
| resolver    | test DNS | none             | none       | `resolver-zones`   |

The last two rows come from the overlays below and are not in the base
topology. The resolver publishes no host port: it answers DNS on port 53 on
the compose network only, and its volume holds a zone file rather than a node
home.

Each node binds its HTTP API to `0.0.0.0` so the published port reaches it, and
each one logs the warning that says what that costs: the API has no
authentication, so anyone who can reach the port can use that node's keys. This
topology is for a laptop and for the test suite.

## Why no internet is needed

Two things would otherwise reach out. Relays and discovery: every `node.json`
here sets `relay: "disabled"`, so the Iroh endpoint uses no n0 relay, no DNS
lookup and no pkarr publish, and a peer is reachable only at an address it was
handed. Addresses: the witness publishes its `EndpointTicket`, its endpoint id and its
witness identity id to the shared `witness-ticket` volume before it starts
serving, and each wallet passes that ticket as `--peer` when it starts, which
seeds the address into its lookup, and records the identity and the endpoint as
one `node.json.witnesses` entry. A ticket is an address hint and never
authorization (proposal 001 section 4).

Startup order follows from that: the wallets declare
`depends_on: witness: condition: service_healthy`, and their entrypoints also
wait for `/shared/witness.ticket` to exist.

The claim is checked, not asserted, by the `compose.internal.yaml` overlay
below.

## Overlays

Each overlay is a second `-f` after `compose.yaml`. None of them renames a
service, moves a port or drops a volume, so anything written against the base
topology keeps working with an overlay on top.

### `compose.internal.yaml`, no route out

The overlay marks the network `internal`, so the containers have no route out
at all. All three still reach healthy and a push from alice still reaches the
witness. Published ports do not work on an internal network, so drive that run
from inside a container (`docker compose exec alice bash`) rather than with
`docker/smoke.sh`.

### `compose.two-witnesses.yaml`, a second witness

```sh
docker compose -f docker/compose.yaml -f docker/compose.two-witnesses.yaml \
  up --build -d
```

`witness-two` is on `127.0.0.1:9083` and UDP 9073, with its own home volume,
and publishes `/shared/witness-two.ticket`, `/shared/witness-two.id` and
`/shared/witness-two.identity` beside the first witness's. Alice and bob wait
for both tickets, start with both seeded as `--peer` and record both witness
identities in `node.json`, so a command in either wallet can push to either
witness with no `--peer` of its own. The two witnesses are two witness
identities, not two machines answering for one: each home mints its own and
witnesses for that one alone, which is what lets stories 004 and 005 push one
branch of a ledger to one witness and another branch to the other.

### `compose.dns.yaml`, a test resolver

```sh
docker compose -f docker/compose.yaml -f docker/compose.dns.yaml up --build -d
docker compose -f docker/compose.yaml -f docker/compose.dns.yaml \
  exec -T alice getent hosts ns.example        # the resolver answers
```

`resolver` is CoreDNS on Alpine (`docker/Dockerfile.resolver`), fixed at
`172.29.0.53`, serving the `example` zone from
`/etc/coredns/zones/example.zone` and answering REFUSED for every other name,
so no lookup leaves the machine. Alice and bob are pointed at it with `dns:`;
Docker's embedded resolver still owns the container names and forwards the
rest. The address is fixed because `dns:` takes addresses, which is why this
overlay gives the network a declared subnet.

The zone lives in the `resolver-zones` volume, seeded from
`docker/dns/zones/example.zone` in the image. The `file` plugin rereads the
file within five seconds, so a rewritten zone needs no restart, but its serial
has to rise or CoreDNS keeps serving what it loaded:

```sh
docker compose -f docker/compose.yaml -f docker/compose.dns.yaml exec -T \
  resolver sh -c 'cat > /etc/coredns/zones/example.zone' < zone-with-the-ids
```

Story 007 is what this is for. A hostname claim is
`_mabel.<hostname> IN TXT "mabel=<identity id>"` (proposal 003 section 2), and
`mabel-endpoints=<id>,<id>` beside it names the machines that answer for
whatever identity that label claims (proposal 006 section 6). The committed zone
carries `_mabel.many-machines.example`, a label naming five machines split
across two character-strings, which a reader joins with no separator before it
parses anything: `mabel-endpoints=` plus four ids is 227 of the 255 bytes a
character-string holds. No container answers at any of those five, which is the
point: the label proves the parsing rule and costs no container. The story
publishes three more cases: a record naming alice, a record under
`bob.example` naming the wrong identity, and no record at all under
`nobody.example`. Keep `_mabel.health.example` in any zone you write: the
resolver's healthcheck asks for it on every interval, and a rewritten zone
that drops it makes the container unhealthy.

The overlay also passes `MABEL_WITNESSES` through to both wallets, empty
unless the environment sets it. One entry per witness,
`<mabel id>=<endpoint id>[,<endpoint id>...]`, because `node.json.witnesses`
names an identity and the machines that answer for it (proposal 006 section
5.4). Neither half exists until the witness has started, so a run that wants the
node-wide witness brings the topology up in two phases:

```sh
docker compose -f docker/compose.yaml -f docker/compose.dns.yaml \
  up -d --wait witness resolver
witness_identity="$(docker compose -f docker/compose.yaml exec -T witness \
  cat /shared/witness.identity)"
witness_id="$(docker compose -f docker/compose.yaml exec -T witness \
  cat /shared/witness.id)"
MABEL_WITNESSES="$witness_identity=$witness_id" \
  docker compose -f docker/compose.yaml \
  -f docker/compose.dns.yaml up -d --wait
```

That is source 4 of resolution (proposal 006 section 5): without it a wallet has
nowhere to read a stranger's ledger from, and story 007's lookup finds no path
to carol.

## Tickets

`mabel node ticket` prints this node's `EndpointTicket`, the string `--peer`
takes:

```sh
docker compose -f docker/compose.yaml exec -T witness mabel node ticket --port 9070
```

`--port` pairs the node's own address, detected from its default route, with
that UDP port. `--addr <IP:PORT>` names an address instead and is repeatable.
With neither flag the ticket names the endpoint alone, which is enough for a
node whose `node.json` sets `relay: "n0"`.

Text output is the ticket and nothing else, so `--peer "$(mabel node ticket
--addr ...)"` works. That is what `docker/entrypoint.sh` does when
`MABEL_PUBLISH_TICKET` is set, and it passes `--addr` rather than `--port`
because a container on the `compose.internal.yaml` network has no default
route to detect from: `--port` there exits 2 with `no_local_address`.

A wrong ticket is loud rather than silent: `serve --peer` exits 2 with
reason `malformed_peer_ticket`, so the wallet never becomes healthy.

`peers.json` holds ledger hints: an accepted `sync push` records the endpoint
that took it as a source for that ledger (proposal 003 section 3). A ticket
reaches a node on the command line, never through this file.

## Operating notes

- The ticket names the witness's container IP. If the witness alone is
  recreated and lands on a different address, the running wallets hold a stale
  hint; restart them (`docker compose -f docker/compose.yaml restart alice bob`)
  or bring the topology up together, which rewrites the ticket first.
- `mabel serve` stops on SIGINT and does not handle SIGTERM, so
  the services set `stop_signal: SIGINT`. Without it every `down` would wait out
  the stop grace period.
- `node.json` is written by the entrypoint on every start from the service's
  environment (`MABEL_HTTP_BIND`, `MABEL_RELAY`, `MABEL_STORAGE_CAPACITY`,
  `MABEL_WITNESSES`), so an edited compose file takes effect on restart. The
  file it writes carries no `role` and no `accept_legacy_witness_config`: one
  node serves one API, and no ledger in this topology was written before
  witnesses were identities. `MABEL_WITNESSES` holds one
  `<mabel id>=<endpoint id>[,<endpoint id>...]` entry per witness, separated by
  spaces, and each goes through `mabel witness set-default`, so a typo fails the
  container rather than being stored. These are the witnesses the node queries
  for any ledger, which is a different set from the witnesses a ledger's own
  chain names.
  Nothing in mabel rewrites that file. `node.key`, the identities and the
  ledgers live on the home volume and survive a recreate; `down -v` drops them.
- A volume carrying the pre-proposal-006 `node.json`, with a `role` line and
  64-character hex endpoint ids under `witnesses`, is rewritten by the
  entrypoint before anything loads it, so it starts clean. Started past the
  entrypoint (`docker run --entrypoint mabel ... serve`) the same file exits 10
  and names the command that fixes it: a hex endpoint id is 64 characters and a
  base32 identity id is 52, so the loader tells the two apart rather than
  configuring a witness that is not one.
- The image runs `mabel` as the container command, and the command is `serve`
  on every node: what a node can do is read from the identities its home holds
  and from `node.json.witness_for` (proposal 006 section 8). `MABEL_ROLE` is
  read by the entrypoint alone and written nowhere: it picks whether this
  container mints a witness identity, advertises itself on it and lists it in
  `witness_for`. `docker run --rm mabel:dev node id` works the same way, and
  `docker run --rm --entrypoint mabel mabel:dev --help` skips the compose
  preparation entirely.

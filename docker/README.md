# Containers

One image runs both node roles, and `compose.yaml` brings up the topology of
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

| Service | Role    | HTTP and UI       | Iroh UDP    | Home volume     |
| ------- | ------- | ----------------- | ----------- | --------------- |
| witness | witness | `127.0.0.1:9080`  | `9070/udp`  | `witness-data`  |
| alice   | wallet  | `127.0.0.1:9081`  | `9071/udp`  | `alice-data`    |
| bob     | wallet  | `127.0.0.1:9082`  | `9072/udp`  | `bob-data`      |

Each node binds its HTTP API to `0.0.0.0` so the published port reaches it, and
each one logs the warning that says what that costs: the API has no
authentication, so anyone who can reach the port can use that node's keys. This
topology is for a laptop and for the test suite.

## Why no internet is needed

Two things would otherwise reach out. Relays and discovery: every `node.json`
here sets `relay: "disabled"`, so the Iroh endpoint uses no n0 relay, no DNS
lookup and no pkarr publish, and a peer is reachable only at an address it was
handed. Addresses: the witness publishes its `EndpointTicket` to the shared
`witness-ticket` volume before it starts serving, and each wallet passes that
ticket as `--peer` when it starts, which seeds the address into its lookup. A
ticket is an address hint and never authorization (proposal 001 section 4).

Startup order follows from that: the wallets declare
`depends_on: witness: condition: service_healthy`, and their entrypoints also
wait for `/shared/witness.ticket` to exist.

The claim is checked, not asserted:

```sh
docker compose -f docker/compose.yaml -f docker/compose.internal.yaml up -d
```

The overlay marks the network `internal`, so the containers have no route out
at all. All three still reach healthy and a push from alice still reaches the
witness. Published ports do not work on an internal network, so drive that run
from inside a container (`docker compose exec alice bash`) rather than with
`docker/smoke.sh`.

## The ticket gap

`mabel` has no command that prints an `EndpointTicket`. `mabel node id` prints
the endpoint id, `witness run` prints the id and the bound UDP addresses, and
`--peer` is the only place a ticket is read. `docker/entrypoint.sh` therefore
assembles the ticket itself from the endpoint id, this container's address on
the compose network and the fixed UDP port; the byte layout of iroh-tickets
1.0.0 is documented at that function. A `mabel node ticket [--addr]` command
would replace those twenty lines, and the entrypoint should switch to it when it
exists.

A wrong ticket is loud rather than silent: `wallet serve --peer` exits 2 with
reason `malformed_peer_ticket`, so the wallet never becomes healthy.

`peers.json` has a `tickets` field, but no runtime reads it yet, so the ticket
is passed on the command line instead. Ticket 015's acceptance criterion asks
for the ticket in `peers.json`; seeding it there today would seed nothing.

## Operating notes

- The ticket names the witness's container IP. If the witness alone is
  recreated and lands on a different address, the running wallets hold a stale
  hint; restart them (`docker compose -f docker/compose.yaml restart alice bob`)
  or bring the topology up together, which rewrites the ticket first.
- `witness run` and `wallet serve` stop on SIGINT and do not handle SIGTERM, so
  the services set `stop_signal: SIGINT`. Without it every `down` would wait out
  the stop grace period.
- `node.json` is written by the entrypoint on every start from the service's
  environment (`MABEL_ROLE`, `MABEL_HTTP_BIND`, `MABEL_RELAY`,
  `MABEL_STORAGE_CAPACITY`), so an edited compose file takes effect on restart.
  Nothing in mabel rewrites that file. `node.key`, the identities and the
  ledgers live on the home volume and survive a recreate; `down -v` drops them.
- The image runs `mabel` as the container command, so the role is the command:
  `witness run` or `wallet serve`. `docker run --rm mabel:dev node id` works the
  same way, and `docker run --rm --entrypoint mabel mabel:dev --help` skips the
  compose preparation entirely.

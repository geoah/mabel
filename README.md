# mabel

Mabel is a peer-to-peer identity ledger. Every identity, a person or an
organization, owns one append-only, hash-chained, ed25519-signed ledger that
records its inception, its witnesses, its trust attestations and revocations,
and the principals it admits. Ledgers replicate over [Iroh](https://iroh.computer)
to passive witnesses and verify from nothing: no CA, no blockchain, no KERI.
It rebuilds hearsay's idea, a signed statement that one identity personally
knows another, on the smallest data model that carries it.

Verified means "this identity signed this statement at this position in its
chain". It is not proof that the statement is true, not proof of legal
identity, and not proof of unique humanity.

Mabel proves no liveness and runs no challenge-response: the issuer is
responsible for out-of-band confirmation that the subject controls the
identity. Every trust report says so, in `subject_control`:

```
subject control was not proven to this verifier; the issuer is responsible for out-of-band confirmation
```

## Quickstart

Build the UI first, because `cargo build` embeds `ui/dist` into the binary,
then build the workspace and run every check:

```sh
(cd ui && npm ci && npm run build)        # writes ui/dist
cargo build --workspace                   # needs Rust 1.91, edition 2024
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
(cd ui && npm test && npm run lint)
```

One node home, no network:

```sh
cargo run -p mabel-cli -- --home /tmp/mabel identity create --alias alice
cargo run -p mabel-cli -- --home /tmp/mabel identity list --json
cargo run -p mabel-cli -- --home /tmp/mabel wallet serve --http 127.0.0.1:9080
```

The wallet serves its HTTP API and the wallet UI on loopback only. `--json` on
any command prints the document `contracts/cli/` freezes. `mabel --help` lists
the rest: `identity`, `membership`, `profile`, `contact`, `graph`, `lookup`,
`trust`, `witness`, `sync`, `verify`, `wallet` and `node`.

A witness in a second home, serving its read-only API and the debug UI that
lists the ledgers it holds:

```sh
cargo run -p mabel-cli -- --home /tmp/mabel-witness witness run --http 127.0.0.1:9081
```

Both roles serve the bundle compiled into the binary. `--ui-dir ui/dist` reads
it from disk instead, and a binary built with no `ui/dist` answers
`ui_not_built` on the UI paths and keeps serving `/api`.

The UI on its own, against the frozen fixtures through a mock service worker,
so no node has to be running:

```sh
(cd ui && npm run dev)     # port 5173: /wallet, /wallet/lookup, /wallet/verify, /witness
```

The whole story, over containers:

```sh
demo/run-demo.sh                                        # up, the story, down -v
docker compose -f docker/compose.yaml up --build -d     # one witness, two wallets
docker/smoke.sh                                         # the scripted check
docker compose -f docker/compose.yaml down -v
```

`demo/run-demo.sh` walks two wallets and one witness through identities,
membership, trust, revocation and a stranger verifying from an empty home. It
needs docker, curl and jq, and reaches nothing outside the bridge network.

The Playwright suite runs the six stories in [docs/stories/](docs/stories/README.md)
against the same topology, driving the CLI through `docker compose exec` and
both UIs in a browser:

```sh
(cd tests/e2e && npm ci && npx playwright install --with-deps chromium && npm test)
```

It builds the `mabel:dev` image from committed `HEAD` through `git archive`, so
an edited working tree cannot change what the topology serves mid-run. Set
`MABEL_E2E_COMMIT` to build a different commit, `MABEL_E2E_REBUILD=1` to force
the build, and `KEEP_TOPOLOGY=1` to leave the containers up afterwards.

## Layout

| Path | What is in it |
|---|---|
| `crates/mabel-core` | the fold, canonical encoding, digests, no networking |
| `crates/mabel-net` | the Iroh sync protocol |
| `crates/mabel-node` | wallet and witness runtimes, the HTTP API, the UI embed |
| `crates/mabel-cli` | the `mabel` binary |
| `proto/mabel/v0` | the normative `.proto` schemas |
| `test-vectors/` | golden and rejection vectors |
| `ui/` | one React app serving the wallet and witness routes |
| `contracts/` | the frozen HTTP and `--json` fixtures every surface is built against |
| `docker/`, `demo/` | the image, the compose topology and the demo script |
| `tests/e2e/` | the Playwright suite that runs the stories over containers |
| `docs/` | decisions, proposals, tickets, stories and research notes |

## Docs

[docs/](docs/README.md) holds the decision records that own the product shape,
the proposals, the research notes, the implementation tickets and the user
stories. Start with
[proposals/001-architecture.md](docs/proposals/001-architecture.md) as amended
by [002-unified-ledger.md](docs/proposals/002-unified-ledger.md) and
[003-wallet-ux-dns-and-trust-graph.md](docs/proposals/003-wallet-ux-dns-and-trust-graph.md).

[contracts/](contracts/README.md) freezes the HTTP responses and the `--json`
documents. A response that does not match a fixture is a bug in the node or
the CLI, not in the fixture.

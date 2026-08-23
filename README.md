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

Build the workspace and the UI, then run the tests:

```sh
cargo build --workspace                   # needs Rust 1.91, edition 2024
cargo test --workspace
(cd ui && npm ci && npm run build && npm test)
```

One node home, no network:

```sh
cargo run -p mabel-cli -- --home /tmp/mabel identity create --alias alice
cargo run -p mabel-cli -- --home /tmp/mabel identity list --json
cargo run -p mabel-cli -- --home /tmp/mabel wallet serve --http 127.0.0.1:9080
```

The wallet serves its HTTP API and the wallet UI on loopback only. `--json` on
any command prints the document `contracts/cli/` freezes.

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
| `docker/`, `demo/` | the image, the compose topology and the demo script |

## Docs

[docs/](docs/README.md) holds the decision records that own the product shape,
the proposals, the research notes and the implementation tickets. Start with
[proposals/001-architecture.md](docs/proposals/001-architecture.md) as amended
by [002-unified-ledger.md](docs/proposals/002-unified-ledger.md).

[contracts/](contracts/README.md) freezes the HTTP responses and the `--json`
documents. A response that does not match a fixture is a bug in the node or
the CLI, not in the fixture.

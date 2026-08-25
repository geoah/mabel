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

Build the UI first, because `cargo build --release` embeds `ui/dist` into the
binary and a debug build reads it from that path at runtime, then build the
workspace and run every check:

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
cargo run -p mabel-cli -- --home /tmp/mabel serve --http 127.0.0.1:9080
```

The node serves its HTTP API and the UI on loopback only. `--json` on any
command prints the document `contracts/cli/` freezes. `mabel --help` lists the
rest: `identity`, `membership`, `profile`, `contact`, `graph`, `lookup`,
`trust`, `witness`, `sync`, `verify`, `serve` and `node`.

One command serves every node: what a node can do is read from what it holds,
the identities in its home and the `witness_for` of its `node.json`, never from
a role. A witness is a second home whose `node.json` names the identities it
witnesses for:

```sh
cargo run -p mabel-cli -- --home /tmp/mabel-witness serve --http 127.0.0.1:9081
```

Every node serves the bundle compiled into the binary. `--ui-dir ui/dist` reads
it from disk instead, and a binary built with no `ui/dist` answers
`ui_not_built` on the UI paths and keeps serving `/api`.

The UI with its own dev server, in front of a real node. `mabel dev seed` fills
an empty home with five identities (one of them a witness), an organization,
four attestations and a private note, all of them really signed, so there is
something to look at:

```sh
cargo run -p mabel-cli -- --home /tmp/mabel-dev dev seed
cargo run -p mabel-cli -- --home /tmp/mabel-dev serve --http 127.0.0.1:9080
(cd ui && npm run dev)     # port 5173, /api proxied to 127.0.0.1:9080
```

`MABEL_API` points that proxy somewhere else. No build ships fake data: the app
talks to the node that served it, in dev and in a release alike. `npm run
harness` is the one exception and is not a build anyone installs: it serves the
screens against the frozen `contracts/http/` documents through a mock service
worker, for a screenshot or for a state a real node makes hard to reach.
`npm run screenshots` builds it, captures every route at three widths, and
reports any page that scrolls sideways.

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

The Playwright suite runs the nine stories in [docs/stories/](docs/stories/README.md),
driving the CLI through `docker compose exec` and both UIs in a browser. Six
run against the compose topology above; 004 and 005 add the second witness
overlay, 007 brings the topology up again with the test resolver overlay, and
008 and 009 hand-start one borrowed home each:

```sh
(cd tests/e2e && npm ci && npx playwright install --with-deps chromium && npm test)
```

It builds the `mabel:dev` and `mabel-resolver:dev` images from committed `HEAD`
through `git archive`, so an edited working tree cannot change what the
topology serves mid-run. Set
`MABEL_E2E_COMMIT` to build a different commit, `MABEL_E2E_REBUILD=1` to force
the build, and `KEEP_TOPOLOGY=1` to leave the containers up afterwards.

## Releases

Every push to main publishes a release, and the version comes from the commits.
The workflow reads the conventional commits made since the last `v<x>.<y>.<z>`
tag and picks the next version from them: a `!` after the type, or a
`BREAKING CHANGE:` footer, bumps the major; a `feat` bumps the minor; anything
else, `fix` and `docs` and `chore` alike, bumps the patch. There is no "nothing
to release" answer, so a docs-only push still ships a patch release with
binaries.

It then writes that version into every manifest that carries one (the workspace
`Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/src-tauri/tauri.conf.json`,
`ui/package.json`, `tests/e2e/package.json` and the two cargo lockfiles and two
npm lockfiles beside them), commits that as `chore(release): v<x>.<y>.<z>`
authored by `github-actions[bot]`, and tags the commit. Every later job builds
from the tag, so the tarball name, the binary's own `--version` and the image
tag are the same version as the release.

A release carries four files:

- `mabel-<version>-x86_64-linux.tar.gz`, the `mabel` binary with the UI
  compiled in.
- `mabel-<version>-aarch64-macos.tar.gz`, the same binary for arm64 macOS,
  signed with a Developer ID and notarized.
- `mabel-app-<version>-macos.dmg`, the desktop app from `app/`, signed,
  notarized and stapled.
- `mabel-app-<version>-macos.zip`, the same `.app` outside a disk image.

Signing needs six Apple secrets on the repository, listed in
[app/README.md](app/README.md#the-secrets). Without them the release still
carries all four files, unsigned, and the notes say so on the release page;
Gatekeeper refuses an unsigned download. The `mabel` binary in the macOS tarball
is notarized but not stapled, because a bare executable has nowhere to keep a
ticket, so Gatekeeper looks that one up over the network. The `.dmg` and the
`.app` carry their ticket with them.

The same commit's image goes to `ghcr.io/geoah/mabel` tagged with the short
commit sha, the release tag and `latest`. The release notes are the commits
since the previous release, grouped by conventional-commit type.

The release commit carries `[skip ci]` and is pushed with the workflow's own
token, so it starts no further runs. To bump a version by hand, run
`scripts/version.sh set <x.y.z>` and commit the result.

Pushing a tag by hand releases exactly that version and computes nothing:

```sh
git tag v1.0.0 && git push origin v1.0.0
```

The manifests have to say `1.0.0` already. The workflow runs
`scripts/version.sh check 1.0.0` first and stops, naming every file that
disagrees, rather than attach `mabel-0.9.0-*.tar.gz` to a release called
`v1.0.0`.

`.github/workflows/ci.yml` runs `cargo fmt`, clippy, the workspace tests and the
UI checks on every push and pull request. The Playwright suite is not in CI: it
needs a docker daemon and the compose topology, so it stays a local check.

## Layout

| Path | What is in it |
|---|---|
| `crates/mabel-core` | the fold, canonical encoding, digests, no networking |
| `crates/mabel-net` | the Iroh sync protocol |
| `crates/mabel-node` | the node home, the one store, the runtime, the HTTP API, the UI embed |
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

## The app

[app/](app/README.md) is a Tauri v2 app for macOS and iOS: a wallet node running
in the app process, with the same wallet UI in a webview in front of it. It calls
the same `NodeRuntime::start` that `mabel serve` calls, binds the HTTP
API to an ephemeral loopback port, and opens its window on
`http://127.0.0.1:<port>/wallet`. It keeps its own node home under the app data
directory rather than `~/.mabel`.

```sh
(cd ui && npm ci && npm run build)                      # the node embeds ui/dist
cargo install tauri-cli --version 2.11.4 --locked
(cd app/src-tauri && cargo tauri dev)
```

`app/src-tauri` is its own cargo workspace, so `cargo build --workspace` and
`cargo test --workspace` at the repository root do not build tauri.
The release workflow builds the macOS app and attaches the `.dmg` to the
release, as [Releases](#releases) describes.
[.github/workflows/app.yml](.github/workflows/app.yml) builds an unsigned iOS
simulator build on every push to `main` and uploads it as a workflow artifact,
and builds the macOS app on demand.

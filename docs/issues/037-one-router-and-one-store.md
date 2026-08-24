# 037: one router and one store

- Status: open
- Depends on: 035, 036

## Goal

Every node serves one router from one store: `api::wallet` and `api::witness`
merge, the two runtimes merge, `WalletReadStore` is deleted, `WitnessStorage`
becomes `node::LedgerStorage`, and `mabel serve` replaces the two serve commands
(proposal 006 section 8).

## Scope

- `crates/mabel-node/src/api/wallet.rs` and `api/witness.rs` merge into one
  router. `NodeRole` stops being read and `role` leaves `GET /api/node`, which
  reports `identity_count` and `witness_for` instead.
- `crates/mabel-node/src/witness/storage.rs` becomes `node::LedgerStorage`,
  keeping the admission rule ticket 034 installed, and `wallet/store.rs` is
  deleted. `WitnessCaps` becomes `StorageCaps` and applies everywhere, so the
  10000-ledger cap and `storage_capacity` now bound a wallet. The startup index
  replaces the re-fold-per-`List` of the wallet adapter, `forks/` is created
  lazily on every node, and `push` runs section 4's rule rather than a flat
  refusal. `wallet/runtime.rs` and `witness/runtime.rs` merge into one runtime.
- Routes: `GET /api/ledgers`, `/api/ledgers/:ledger_id` and
  `/api/ledgers/:ledger_id/events` are removed and answered by the three
  identity routes; `GET /api/identities/known` gains `offset`, `limit` and
  `more`, default 100 and maximum 256; `GET /api/forks` keeps its name and its
  optional `ledger_id` on every node; `GET /api/witnesses` rows become
  identities with `endpoints: [{endpoint_id, binding}]`; `GET
  /api/witnesses/:endpoint_id/ledgers` becomes `GET
  /api/witnesses/:identity_id/holdings`, resolving through section 5 before
  proxying `List`, keeping `witness_unreachable` and its 502 with `details`
  naming the identity and every endpoint tried.
- `List` narrows to the ledgers this home signs for plus, when `witness_for` is
  non-empty, the ledgers it admitted as a witness. `mabel-net`'s `server.rs`
  takes this as a `Store` contract and `sync.proto` gains the comment on
  `ListReq` recording it.
- Value-surface refusals of section 8: `unresolvable_witness` for an id this
  home cannot resolve to a known identity within the 5.2 budget, with
  `details.witness` and `details.endpoints_tried`, and `endpoint_not_identity`
  before any dial for an id equal to an endpoint id this home knows. A mutating
  route naming a ledger this home holds and cannot append to answers 403 code 2
  `no_local_signer`, `unknown_ledger` keeps its meaning, and `ledger_not_held`
  dies with the witness routes. Nothing gates a route: a home with no identities
  answers `{"ok": true, "identities": []}`.
- `mabel serve`, with `wallet serve` and `witness run` as hidden undocumented
  aliases. `role` in `node.json` stays recognised, is read by nothing, and is
  logged once at startup with the file, the key and the fix.

## Acceptance criteria

- [ ] One router is served by every node and no route is gated on what the home
      holds.
- [ ] A home with an empty `witness_for` and no signing key answers
      `NOT_ADMITTED` for a ledger it does not store, with the reason naming the
      rule.
- [ ] A ledger this home merely fetched is served by `Get` and never appears in
      `List`.
- [ ] An existing `node.json` carrying `role` loads and logs once.
- [ ] tests: push and fetch across a two-node topology, before any fixture is
      touched, in `crates/mabel-node/tests/wallet.rs` and
      `crates/mabel-cli/tests/sync.rs`.
- [ ] tests: paging on `GET /api/identities/known` at the default, at the
      maximum and over it; `GET /api/forks` answering on a home with no
      `witness_for`; a client sending an endpoint id to `/holdings` getting 404.
- [ ] tests: `cargo fmt`, `clippy` and the workspace suite pass, with the
      witness-only test files folded into the node suite rather than deleted.

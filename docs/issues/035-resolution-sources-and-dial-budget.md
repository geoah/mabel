# 035: resolution sources, dial budget and `peers.json` hygiene

- Status: open
- Depends on: 034

## Goal

One resolution path finds a ledger through eight ranked sources, resolves a
witness identity to endpoints without recursing, dials at most 16 distinct
endpoints per operation under a per-class allocation, and keeps `peers.json` a
bounded cache (proposal 006 section 5).

## Scope

- `crates/mabel-node/src/graph/model.rs`: `FetchSource` becomes the eight
  variants of section 5, replacing the four of proposal 003 section 3.
  `LedgerEndpoint`, `WitnessIdentity` and `LegacyWitnessHint` stay three
  variants: an endpoint reached through the tag-11 list is never merged into a
  tag-18 advertisement, never establishes a binding and never reports
  `verified`.
- `crates/mabel-node/src/graph/fetcher.rs`: `plan_sources` in source order, one
  `BTreeSet<IdentityId>` visited set per top-level operation, every applicable
  source queried rather than stopping at the first, and source 8 queried only
  when sources 1 to 7 produced no reachable copy.
- Witness resolution as the base operation of 5.1: the same list with sources 4
  and 6 removed, plus the bootstrap endpoints `node.json` records beside the
  witness id.
- The dial budget of 5.2: 16 distinct endpoints counted once per endpoint id
  after dedupe, the per-class caps with 4 slots reserved for `NodeWitness`, the
  crawl's existing 60-second `RUN_BUDGET` when the operation is a crawl and a
  new 20-second `RESOLVE_BUDGET` otherwise. `crates/mabel-node/src/graph/crawl.rs`
  takes the shared deadline; the crawler's other caps are unchanged.
- `crates/mabel-node/src/peers.rs`: each hint becomes
  `{endpoint, first_seen_ms, last_success_ms, failures}`, 8 per ledger with the
  oldest `last_success_ms` evicted over the cap, a 30-day age-out, eviction after
  three consecutive failures and a reset on success. A bare string loads as a
  hint with no timestamps; the file is rewritten in the new shape on first write.
  A `CallerHint` endpoint is never written.
- `crates/mabel-node/src/config.rs`: `witnesses` becomes
  `[{identity, endpoints}]`; an array holding 64-character hex endpoint ids fails
  to load with a message naming what to run instead. `mabel witness set-default
  --witness <mabel-id> [--endpoints <endpoint,...>]` writes both and refuses with
  `unresolvable_witness` when it can reach no endpoint for the witness.
- `crates/mabel-node/src/wallet/core.rs::witnesses_of` and `wallet/sync.rs` take
  resolution; `mabel sync fetch --from-witness` resolves through 5.1 and `from`
  becomes a plain `CallerHint`, so the `unknown_witness` refusal is deleted and
  `conflicting_source` refuses both keys at once.

## Acceptance criteria

- [ ] The source order, the per-class caps and the reservation match 5.2; an
      endpoint three sources name costs one budget slot.
- [ ] A ledger naming itself in its own `WitnessSet` terminates, and a witness
      named both in `node.json` and in the chain is resolved once.
- [ ] Source 8 does not run when an earlier source produced a reachable copy.
- [ ] tests: a chain naming 16 witnesses still leaves 4 dials for
      `node.json.witnesses`; a run stops at 16 distinct endpoints and at
      `RESOLVE_BUDGET`.
- [ ] tests: `peers.json` cap, age-out, three-failure eviction, success reset,
      bare-string load, new-shape rewrite, and a `CallerHint` endpoint absent
      from the file after a successful fetch.
- [ ] tests: an old `node.json` with hex endpoint ids fails to load with the
      message; `witness set-default` refuses an unreachable witness.
- [ ] tests: push path unbroken. `crates/mabel-node/tests/wallet.rs` and
      `crates/mabel-cli/tests/sync.rs` push and fetch through a witness resolved
      from `node.json` alone; `cargo fmt`, `clippy` and the workspace suite pass.

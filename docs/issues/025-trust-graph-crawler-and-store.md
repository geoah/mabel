# 025: trust graph crawler and generation store

- Status: done
- Depends on: 023, 011

## Goal

`mabel graph sync` crawls trust attestations breadth-first from every local
identity, verifies each ledger in memory, and writes a generation the lookup
route reads, per proposal 003 section 3.

## Scope

- A `LedgerFetcher` trait wrapping the four-source order of section 3, every
  applicable source queried rather than stopping at the first, so the crawler's
  tests inject a fake.
- Verification in memory over the ticket 011 `WalletSync::candidate` path. The
  crawler writes no stranger's ledger under `ledgers/`.
- Equivocation across sources recorded on the graph node with both endpoints
  and both event ids, under the existing rule (proposal 001 section 3.7), never
  resolved to one branch. A source that served a verified copy is written back
  to `peers.json`.
- Breadth-first crawl with ties broken by ascending identity id, every root a
  local identity at depth 0, and the caps of section 3 (depth, nodes, in-flight
  fetches, per-fetch and whole-run clocks, fetch count).
- Generations: `graph/generations/<sync_id>/nodes/<identity_id>.json` and
  `summary.json` with the fields section 3 lists, then an atomic swap of
  `graph/current.json`. Older generations garbage collected to the last two.
- Reverse edges computed by scanning the generation at load and always returned
  as `{best_effort: true, entries: [...]}`.
- Node and graph staleness at 24 hours, and the graph service trait in
  `crates/mabel-node/src/api/` that tickets 026 and 029 read through.
- CLI `mabel graph sync`. The HTTP routes are ticket 026.

## Acceptance criteria

- [ ] No file under `ledgers/` is created or modified by a crawl.
- [ ] tests, with a fake `LedgerFetcher`: all four sources are queried for one
      frontier ledger, in the order section 3 gives.
- [ ] tests: one case per `truncated_by` value (`depth`, `nodes`, `fetches`,
      `time`), each setting `truncated` and the reason in `summary.json`.
- [ ] tests: two crawls over the same fake graph produce identical node sets and
      ordering, including when truncated.
- [ ] tests: a reader holding `current.json` during a sync sees the old
      generation whole; the pointer swap is atomic; the third-oldest generation
      is collected.
- [ ] tests: two sources serving divergent heads record an equivocation with
      both endpoints and both event ids.
- [ ] tests: a node reached from two local roots carries both in `roots` with
      its depth from each.
- [ ] tests: `cargo fmt`, `clippy` and the workspace suite pass.

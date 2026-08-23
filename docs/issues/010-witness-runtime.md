# 010: witness runtime, admission, push semantics and forks

- Status: done
- Depends on: 006, 007, 009

## Goal

`mabel witness run` starts a passive replica that verifies before storing,
enforces the admission rule, applies the push semantics of proposal 001
section 5, records forks and serves reads.

## Scope

- Store implementation over ticket 007 storage backing the ticket 009 server,
  using ticket 007's fork file naming.
- Admission: a `Push` is accepted only if the ledger is already stored or the
  pushed chain's folded `WitnessConfig` lists this witness's own `EndpointId`
  (section 5). Reads stay open to all.
- Push semantics: events must start at seq 0 for an unheld ledger, at
  `stored_head + 1`, or overlap the stored suffix byte-identically; a gap is
  `Rejected { MALFORMED }`; a partially valid push stores its valid prefix
  atomically and answers `Rejected { INVALID }` with the failing `at_seq`.
- Verification strategy: full verification from nothing at first ingest, the
  folded state kept and rebuilt from disk on startup or on demand, later pushes
  verifying only the spliced suffix against it (section 5).
- Forks: first-seen-wins, a divergent event validated with
  `validate_fork_record` and stored as a `ForkRecord` with both events and the
  source endpoint; recording stops after 8 records per ledger (section 5).
- Caps: 4096 events and 4 MiB per ledger, 10000 ledgers and the configurable
  `storage_cap` per witness (sections 5 and 8).
- `LedgerSummary` assembly, paging for `List` and `Forks`, and `witness run
  [--http <addr>] [--iroh-port <n>]`; the HTTP surface is ticket 012.
- The witness read service trait in `crates/mabel-node/src/api/`, implemented
  over this store so ticket 012 drops its stub: ledger list, ledger detail,
  events with an inclusive `since`, and forks, returning the documents the
  `contracts/http/witness-*.json` fixtures pin.
- The witness holds no identity keys and signs nothing (section 2).

## Acceptance criteria

- [ ] tests, each a named case: a push of an unheld ledger naming this witness
      is admitted; a third party may relay to an already stored ledger; a push
      for an unknown ledger not naming the witness answers `NOT_ADMITTED`.
- [ ] tests: first ingest verifies the full chain and a following push verifies
      only the spliced suffix; a restart rebuilds folded state from disk.
- [ ] tests: a gapped push answers `MALFORMED`; an overlapping re-push is
      idempotent; a partially invalid push stores the valid prefix atomically
      and answers `INVALID` with the right `at_seq`.
- [ ] tests: a fork push produces a `ForkRecord` carrying both events while the
      first event survives; an invalid conflicting event is rejected and not
      stored; the ninth fork on one ledger is not recorded and
      `forks_truncated` is set.
- [ ] tests: pushes crossing the 4096-event, 4 MiB, 10000-ledger and
      `storage_cap` limits are rejected.
- [ ] tests: `List` paging is stable in ascending ledger id order across pages.
- [ ] tests: the witness service trait, backed by this store, returns documents
      matching every `contracts/http/witness-*.json` fixture, so ticket 012's
      route tests pass against the real implementation.
- [ ] The stored event bytes are the received bytes (section 3.1).

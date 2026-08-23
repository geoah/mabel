# 010: witness runtime, admission, push semantics and forks

- Status: open
- Depends on: 006, 007, 009

## Goal

`mabel witness run` starts a passive replica that verifies before storing,
enforces the admission rule, applies the push semantics of proposal 001
section 5, records forks and serves reads.

## Scope

- Store implementation over ticket 007 storage backing the ticket 009 server.
- Admission: a `Push` is accepted only if the ledger is already stored or the
  pushed chain's folded `WitnessConfig` lists this witness's own `EndpointId`;
  otherwise `Rejected { NOT_ADMITTED }`. Reads stay open to all (section 5).
- Push semantics: events must start at seq 0 for an unheld ledger, at
  `stored_head + 1`, or overlap the stored suffix byte-identically; a gap is
  `Rejected { MALFORMED }`; a partially valid push stores its valid prefix
  atomically and answers `Rejected { INVALID }` with the failing `at_seq`
  (section 5).
- Verification strategy: full verification from nothing at first ingest, the
  folded state kept and rebuilt from disk on startup or on demand, later pushes
  verifying only the spliced suffix against it (section 5).
- Forks: first-seen-wins, a divergent event validated with
  `validate_fork_record` and stored as a `ForkRecord` carrying both events and
  the source endpoint; recording stops at 8 records per ledger with
  `forks_truncated` set (section 5).
- Per-ledger caps 4096 events and 4 MiB, per witness 10000 ledgers and the
  storage cap from `node.json`, default 2 GiB (section 5).
- `LedgerSummary` assembly for `List` and paging for `List` and `Forks`
  (section 5).
- `witness run [--http <addr>] [--iroh-port <n>]` starting the Iroh endpoint;
  the HTTP surface is ticket 012.
- The witness holds no identity keys and signs nothing (section 2).

## Acceptance criteria

- [ ] Admission, push, fork and cap behaviour match section 5 clause by clause.
- [ ] The stored event bytes are the received bytes (section 3.1).
- [ ] tests: protocol tests over two endpoints cover a gapped push
      (`MALFORMED`), an idempotent overlapping re-push, a partially invalid
      push storing the valid prefix and naming `at_seq`, a push for an unknown
      ledger that does not name the witness (`NOT_ADMITTED`), and a fork push
      producing a `ForkRecord` with both events while the first event survives
      (section 11, protocol tests bullet).
- [ ] tests: restarting the runtime rebuilds folded state from disk and a
      following suffix push verifies against it.

# 016: CLI integration tests and the fresh-verifier test

- Status: open
- Depends on: 010, 011

## Goal

An integration suite drives the CLI across two wallets and two witnesses over
real Iroh endpoints, covering the network exit codes and the fresh-verifier
case proposal 001 section 11 calls the best acceptance test.

## Scope

- Harness spawning two witness nodes and two wallet nodes, each with its own
  temp home, over loopback endpoints with relays disabled; the second witness
  exists for the equivocation case (sections 3.7 and 11).
- Full flow through the CLI: two identities, witness configuration, push, org
  founding, invite, accept, admit, promotion, removal, trust attestation from
  the person and from the org, revocation, `verify ledger` and `verify trust`
  at each stage (section 9).
- Network exit codes end to end: 30 for an unreachable peer, 50 for a stale
  append against an org head another wallet moved, 20 for equivocation across
  the two witnesses on divergent branches (sections 3.7, 5 and 9). The
  component-level twins of the exit-50 and exit-20 cases live in ticket 011.
- `--json` shape stability across the network commands, asserting `source`,
  `head_seq`, `head_event` and `fetched_at` are present (section 6, flag R).
- Fresh-verifier test: a home with no identities, no ledgers and no keys, which
  learns the witness only through `--peer <ticket>`, then runs `verify trust`.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here.

## Acceptance criteria

- [ ] The fresh verifier fetches and verifies both the issuer's and the
      subject's ledger from bytes, from the witness alone, and reports the
      pinned `verify trust` result of section 9.
- [ ] Verification in the suite goes through the full-chain-from-nothing path,
      not the witness suffix path (section 5).
- [ ] tests: the suite runs under `cargo test` with no network access beyond
      loopback and asserts exit codes 0, 20, 30 and 50 with their JSON bodies
      and text prefixes.

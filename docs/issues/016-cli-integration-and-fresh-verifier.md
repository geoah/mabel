# 016: CLI integration tests and the fresh-verifier test

- Status: open
- Depends on: 010, 011

## Goal

An integration suite drives the CLI across a wallet and a witness over real
Iroh endpoints, covering the network exit codes and the fresh-verifier case
proposal 001 section 11 calls the best acceptance test.

## Scope

- Harness spawning one witness node and two wallet nodes, each with its own
  temp home, over loopback endpoints with relays disabled (section 11, protocol
  tests bullet).
- Full flow through the CLI: two identities, witness configuration, push, org
  founding, invite, accept, admit, promotion, removal, trust attestation from
  the person and from the org, revocation, `verify ledger` and `verify trust`
  at each stage (section 9).
- Network exit codes: 30 for an unreachable peer, 50 for a stale append against
  a moved org head, 20 for equivocation across two witnesses on divergent
  branches (sections 3.7, 5 and 9).
- `--json` shape stability across the network commands, asserting `source`,
  `head_seq`, `head_event` and `fetched_at` are present (section 6, flag R).
- Fresh-verifier test: wipe the home, then run `verify trust` against a witness
  with no local state and no keys (section 11, CLI tests bullet).

Out of scope: Playwright e2e, which is phase 6.

## Acceptance criteria

- [ ] The fresh verifier resolves both the issuer's and the subject's ledger
      from the witness alone and reports the pinned `verify trust` result of
      section 9.
- [ ] Verification in the suite goes through the full-chain-from-nothing path,
      not the witness suffix path (section 5).
- [ ] tests: the suite runs under `cargo test` with no network access beyond
      loopback and asserts the exit codes 0, 20, 30 and 50 named above.

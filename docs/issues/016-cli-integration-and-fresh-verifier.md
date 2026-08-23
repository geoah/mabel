# 016: CLI integration tests and the fresh-verifier test

- Status: open
- Depends on: 010, 011, 018

## Goal

An integration suite drives the CLI across two wallets and two witnesses over
real Iroh endpoints, covering the network exit codes and the fresh-verifier
case proposal 001 section 11 calls the best acceptance test.

## Scope

- Harness spawning two witness nodes and two wallet nodes, each with its own
  temp home, over loopback endpoints with relays disabled; the second witness
  exists for the equivocation case (sections 3.7 and 11).
- Full flow through the CLI: two identities, witness configuration, push, an
  identity-rooted ledger created with `identity create --founder`, membership
  invite, accept, admit, promotion, removal, trust attestation from a
  raw-rooted and from an identity-rooted ledger, revocation, `verify ledger` and
  `verify trust` at each stage (proposal 002 section 6).
- Network exit codes end to end: 30 for an unreachable peer, 50 for a stale
  append against a shared ledger head another wallet moved, 20 for equivocation
  across the two witnesses on divergent branches (sections 3.7, 5 and 9). The
  component-level twins of the exit-50 and exit-20 cases live in ticket 011.
- `--json` shape stability across the network commands, asserting `source`,
  `head_seq`, `head_event` and `fetched_at_ms` are present (section 6, flag R),
  and that `verify trust` carries `signing_principal` with the principal that
  signed the attestation (proposal 002 section 5).
- Fresh-verifier test: a home with no identities, no ledgers and no keys, which
  learns the witness only through `--peer <ticket>`, then runs `verify trust`.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here.

## Acceptance criteria

- [ ] The fresh verifier fetches and verifies both the issuer's and the
      subject's ledger from bytes, from the witness alone, and reports the
      `verify trust` document of `contracts/cli/verify-trust.json`, including
      `signing_principal`.
- [ ] Verification in the suite goes through the full-chain-from-nothing path,
      not the witness suffix path (section 5).
- [ ] tests: the suite runs under `cargo test` with no network access beyond
      loopback and asserts exit codes 0, 20, 30 and 50 with their JSON bodies
      and text prefixes.

- [ ] Wire the append discipline (WalletSync::ensure_fresh) into the CLI
      appending commands (trust add/revoke, witness add, membership) for
      ledgers that name witnesses, with a test (ticket 011 deviation 2).

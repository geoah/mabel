# 030: witness-side crawl and pull provenance

- Status: open (second priority, may slip)
- Depends on: 025

## Goal

A witness may pull ledgers named by trust attestations in ledgers it already
stores, recording why it holds each one. Off by default, per proposal 003
section 3's deferred piece.

## Scope

- An opt-in setting in `node.json`, default off, enabling the pull.
- The pull reuses the ticket 025 crawl and the ticket 011 verify path; a
  witness still verifies before storing and still signs nothing.
- Per-ledger `meta.json` gains `pull_reason`, either `pushed` or
  `referenced_by:<ledger id>`, recorded at first store.
- Admission (proposal 002 section 5) is untouched: this is a pull, so a pulled
  ledger need not name this witness, and pushes keep the existing rule.
- The existing global `storage_cap` and per-ledger caps bound the pull; a pull
  that would cross a cap stops rather than evicting.
- The witness UI shows `pull_reason` on the ledger row, behind developer mode
  where ticket 027 puts provenance.

## Acceptance criteria

- [ ] The setting defaults to off, and with it off a witness pulls nothing and
      behaves exactly as ticket 010 leaves it.
- [ ] A pulled ledger is stored only after full verification.
- [ ] tests: with the setting on, a stored ledger's trust attestation causes a
      pull, and the pulled ledger's `meta.json` records
      `referenced_by:<ledger id>`.
- [ ] tests: a pushed ledger records `pull_reason: pushed`, and a later pull
      reference does not overwrite it.
- [ ] tests: an unadmitted push is still rejected while the pull is enabled.
- [ ] tests: a pull that would cross `storage_cap` stops and evicts nothing.
- [ ] tests: `cargo fmt`, `clippy` and the workspace suite pass.

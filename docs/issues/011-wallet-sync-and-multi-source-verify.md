# 011: wallet sync, append discipline and multi-source verification

- Status: open
- Depends on: 008, 009

## Goal

A wallet pushes and fetches ledgers over Iroh, refuses to append on stale state
with exit code 50, and verifies a ledger against several witnesses with the
comparison rules of proposal 001 section 3.7.

## Scope

- `sync push --identity <alias|id> [--to <id>]` and `sync fetch <ledger-id>
  --from <id>`, plus `--from` and `--peer <ticket>` on the network commands
  (section 9).
- Append discipline for shared org ledgers: query `Head` from the org's
  configured witnesses first, fast-forward when a witness is ahead, and when a
  local unpushed event conflicts with an observed head, discard it and re-sign
  the same intent on the new head; the CLI surfaces the conflict as exit code
  50, stale state (section 5, appending to a shared ledger).
- Multi-source verification (section 3.7): with no `--from`, query every
  configured witness in parallel and verify each candidate independently from
  nothing; a longer candidate wins only if it extends the shorter one event id
  for event id; two diverging valid candidates are equivocation, reported with
  both source endpoints and both event ids at the divergence, exit 20.
- Subject resolution for `verify trust`: fetch the subject's ledger, verify it
  from nothing, require `ledger_id` to equal the requested id, and report
  `subject: unresolved (not held by any queried source)` when no source holds
  it, without failing the verification (section 3.7).
- The wallet serves the Iroh protocol read-only so peers can fetch its ledgers
  (section 2).

## Acceptance criteria

- [ ] Dialling uses `EndpointId` alone, with tickets only as address hints
      (sections 3.7 and 9).
- [ ] Every verification result names its source, head sequence, head event and
      fetch time (section 6, flag R).
- [ ] tests: an append against a moved head exits 50 and a retry on the new
      head succeeds (section 11, CLI tests bullet).
- [ ] tests: two witnesses on divergent branches make the verifier report both
      source endpoints and both event ids and exit 20; a witness holding a
      strict prefix of another loses without an equivocation report
      (section 3.7).
- [ ] tests: `verify trust` with a subject no source holds reports
      `unresolved` and still succeeds.

# 011: wallet sync, append discipline and multi-source verification

- Status: open
- Depends on: 007, 008, 009, 018

## Goal

A wallet pushes and fetches ledgers over Iroh, refuses to append on stale state
with exit code 50, and verifies a ledger against several sources with the
comparison rules of proposal 001 section 3.7.

## Scope

- `sync push --identity [--to]`, `sync fetch <ledger-id> --from`, plus `--from`
  and `--peer <ticket>` on the network commands (section 9).
- A read-only store adapter over ticket 007 storage so peers can fetch the
  wallet's ledgers (section 2). It is a thin read adapter, not shared with the
  witness store of ticket 010.
- Append discipline for shared org ledgers, wired into the org-mutating command
  paths of tickets 008 and 018: query `Head` from the org's witnesses first,
  fast-forward when one is ahead, and when a local unpushed event conflicts
  with an observed head, discard it and re-sign the same intent on the new head
  (section 5).
- Multi-source verification (section 3.7): with no `--from`, query every
  configured witness in parallel and verify each candidate independently from
  nothing; a longer candidate wins only if it extends the shorter one event id
  for event id; two diverging valid candidates are equivocation, reported with
  both source endpoints and both event ids at the divergence.
- Subject resolution for `verify trust`: fetch the subject's ledger, verify it
  from nothing, require `ledger_id` to equal the requested id, and report
  `subject: unresolved (not held by any queried source)` when no source holds
  it, without failing the verification (section 3.7).

## Acceptance criteria

- [ ] Dialling uses `EndpointId` alone, with tickets only as address hints
      (sections 3.7 and 9).
- [ ] Every verification result names its source, head sequence, head event and
      fetch time (section 6, flag R).
- [ ] This ticket owns exit codes 30 and 50: a dial or request failure exits 30
      with the `Network error:` prefix, and an append against an advanced
      remote head exits 50; each has a test asserting the code, the
      `{ok, code, message, details}` body and the text prefix.
- [ ] tests run against in-process doubles, a stub witness implementing the
      ticket 009 store trait; the real two-witness versions of the exit-50 and
      exit-20 cases belong to ticket 016.
- [ ] tests: after an exit-50 append no stale event remains in the home, and a
      retry re-signs the same intent on the new head and succeeds.
- [ ] tests: two stub sources on divergent branches report both endpoints and
      both event ids and exit 20; a source holding a strict prefix of another
      loses without an equivocation report; `verify trust` for a subject no
      source holds reports `unresolved` and still succeeds.

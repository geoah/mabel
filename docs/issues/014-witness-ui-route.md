# 014: witness debug UI route

- Status: open
- Depends on: 013

## Goal

The witness route lists what one witness holds and shows fork records with both
conflicting events, the diagnostic surface proposal 001 sections 5 and 6
describe.

## Scope

- Ledger list from `GET /ledgers`, showing the `LedgerSummary` fields of
  section 5: ledger, kind, head sequence, head event, event count, first seen,
  updated, fork count and `forks_truncated`.
- Ledger detail from `GET /ledgers/:id` and `/ledgers/:id/events?since=` with
  paging.
- Forks view from `GET /forks`, showing each `ForkRecord`'s sequence, both
  `SignedEvent`s, observed time and source endpoint, labelled as evidence of
  equivocation or of a lost race between honest controllers, and not as
  authorization (sections 4 and 5).
- A note that the list is what this one witness holds, a diagnostic and not an
  index (section 6, flag D).
- `data-testid` attributes on ledger rows, fork rows and both event panes.

Out of scope: Playwright specs, which are phase 6.

## Acceptance criteria

- [ ] Every `LedgerSummary` and `ForkRecord` field named in section 5 appears
      in the route.
- [ ] The fork view shows the kept and the conflicting event side by side so a
      reader checks the conflict without a second request (section 5).
- [ ] The route is read-only and issues no mutating request (section 10).
- [ ] tests: `npm run build`, typecheck and lint pass, and the route renders
      against a witness node holding at least one ledger and one fork record.

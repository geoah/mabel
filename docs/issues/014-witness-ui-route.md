# 014: witness debug UI route

- Status: open
- Depends on: 013

## Goal

The witness route lists what one witness holds and shows fork records with both
conflicting events, the diagnostic surface proposal 001 sections 5 and 6
describe. It must land before the milestone 10 Playwright run, which drives it.

## Scope

- Ledger list from `GET /ledgers`, showing the `LedgerSummary` fields of
  section 5: `ledger_id`, `declared_kind`, head sequence, head event, event
  count, first seen, updated, fork count and `forks_truncated`. The column is
  labelled "declared", because the kind is advisory (proposal 002 section 3).
- Ledger detail from `GET /ledgers/:id` and `/ledgers/:id/events?since=` with
  paging, using the inclusive `since` semantics of ticket 012.
- Forks view from `GET /forks`, showing each `ForkRecord`'s sequence, both
  `SignedEvent`s, observed time and source endpoint, labelled as evidence of
  equivocation or of a lost race between honest controllers, and not as
  authorization (sections 4 and 5).
- A note that the list is what this one witness holds, a diagnostic and not an
  index (section 6, flag D).
- `data-testid` attributes on ledger rows, fork rows and both event panes.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here. The route is built against the witness fixtures
in `contracts/http/`, which the ticket 012 stub serves; real witness data
arrives when ticket 010 implements the witness service trait behind those
routes, with no change here.

## Acceptance criteria

- [ ] Every `LedgerSummary` and `ForkRecord` field named in section 5 appears
      in the route.
- [ ] The fork view shows the kept and the conflicting event side by side so a
      reader checks the conflict without a second request (section 5).
- [ ] The route is read-only and issues no mutating request (section 10).
- [ ] tests: vitest plus testing-library component tests with a mocked API
      cover ledger paging, fork rendering with both events, the
      `forks_truncated` indicator, and the absence of any mutating control.
- [ ] tests: `npm run build`, typecheck and lint pass.

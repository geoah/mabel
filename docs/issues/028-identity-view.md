# 028: identity view, overview table, ledger lines, state and actions

- Status: done
- Depends on: 026, 027, 021

## Goal

The wallet identity route becomes the four-part screen of proposal 003
section 4: overview, ledger, state and actions. It absorbs ticket 019, which
proposal 003 closes as superseded.

## Scope

- Overview: one compact key-value table, key and value on a line and never
  stacked, holding the fields section 4 lists, with the verification icon
  beside the hostname.
- Ledger: one line per event as sequence plus event type, each expandable to
  the event detail the current panels show.
- State: the trusted list with resolved names, verification icons and links
  into lookup; the principals table, shown only when the ledger has more than
  its root principal.
- Actions, each with the one-line description section 4 asks for, covering the
  twelve operations it lists.
- Ticket 019's screens rebuilt on this layout: the membership flow (invite,
  accept, admit, remove), the raw-root `CONTROLLER` warning before confirming
  an acceptance, the sync screen and the verify screen. They call the ticket
  021 routes.
- Verification status rendered as section 2 specifies, each state advisory and
  labelled, with the declared kind still labelled declared.
- `data-testid` attributes on every form control and result region.

Out of scope: the foreign-identity drill-down, which is ticket 029, and
Playwright specs, which belong to milestone 10.

## Acceptance criteria

- [ ] No stacked label-over-value panel survives on this route (decision 014).
- [ ] Every wallet API route in `contracts/http/` is reachable from the UI once
      this ticket, 027 and 029 are done.
- [ ] The UI holds no keys and does no crypto; the invitee's acceptance is
      produced by the node.
- [ ] The verify screen never prints "unrevoked", always shows the source and
      head sequence, and names the signing principal.
- [ ] tests: the overview table renders each of the five verification statuses
      with its own marker, and `unclaimed` renders nothing.
- [ ] tests: the membership forms drive the three ticket 021 routes against a
      mocked API, including the raw-root `CONTROLLER` warning.
- [ ] tests: an error envelope renders from `code` and `details.reason`.
- [ ] tests: `npm run build`, typecheck, lint and vitest pass.

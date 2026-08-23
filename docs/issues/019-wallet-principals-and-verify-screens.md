# 019: wallet Principals panel, membership, sync and verify screens

- Status: open
- Depends on: 013; the Principals panel and the membership screens also need
  ticket 021's contract freeze

## Goal

The wallet route covers the rest of the product: one identity screen with a
Principals panel for every controlled ledger, the three-step membership flow,
removal, pushing to witnesses and verifying a claim, all through the ticket 012
API (proposal 002 section 6).

## Scope

- Principals panel on the identity screen, for every ledger this node controls,
  not a separate organization screen: the root variant, each principal's
  identity id, key and role, and the open invitations with their status.
- Membership screens: invite an identity, accept an invitation as an identity,
  admit an acceptance, and remove a principal or open invitee, calling
  `POST /api/identities/:identity_id/memberships/invitations`, `/acceptances`
  and `/removals`.
- The accept screen shows the ledger's root variant, its current controllers and
  the offered role, and warns that accepting `CONTROLLER` on a raw-rooted ledger
  means signing as that identity, before the user confirms (proposal 002
  section 4, accept surface).
- Identity creation gains the declared kind and an optional founder, matching
  `identity create --kind` and `--founder` (proposal 002 section 6). Declared
  kind is labelled "declared" wherever it appears, never as a checked fact.
- Sync screen calling `POST /sync/push`, showing per-witness results.
- Verify screen calling `POST /verify`, rendering the report with the flag L and
  flag R wording, the `signing_principal` of the answering event, and the
  equivocation result with both sources and both event ids (sections 3.7 and 6).
- `data-testid` attributes on every form control and result region.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here. The sync and verify screens need only ticket
013, because their routes are frozen; the membership request and response
shapes are pending until ticket 021 fixtures them
(`contracts/http/PENDING-membership.md`), and ticket 021 wires these forms to
the live routes.

## Acceptance criteria

- [ ] Every wallet API route in `contracts/http/` is reachable from the UI once
      this ticket and ticket 013 are done.
- [ ] There is no organization screen: membership is reached through the
      identity screen for any ledger (proposal 002 section 6).
- [ ] The UI holds no keys and does no crypto; the invitee's acceptance is
      produced by the node, not the browser (section 10).
- [ ] The verify screen never prints "unrevoked", always shows the source and
      head sequence, and names the signing principal (section 6, flag R).
- [ ] tests: vitest plus testing-library component tests with a mocked API
      cover the membership forms, their validation, the raw-root `CONTROLLER`
      warning, an error envelope rendering and the equivocation result.
- [ ] tests: `npm run build`, typecheck and lint pass.

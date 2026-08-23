# 019: wallet org, sync and verify screens

- Status: open
- Depends on: 013

## Goal

The wallet route covers the rest of the product: founding an org, the three-step
membership flow, removal, pushing to witnesses and verifying a claim, all
through the ticket 012 API.

## Scope

- Org screens: create an org, list and show an org with its controllers,
  members and open invites, invite an identity, accept an invite as an
  identity, admit an acceptance, and remove a member, calling `POST /orgs`,
  `/orgs/:id/invites`, `/orgs/:id/acceptances`, `/orgs/:id/removals`
  (section 10).
- Sync screen calling `POST /sync/push`, showing per-witness results.
- Verify screen calling `POST /verify`, rendering the "as of seq N from source
  S" struct with the flag L and flag R wording, and rendering the equivocation
  result with both sources and both event ids (sections 3.7 and 6).
- `data-testid` attributes on every form control and result region.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here.

## Acceptance criteria

- [ ] Every wallet API route from section 10 is reachable from the UI once this
      ticket and ticket 013 are done.
- [ ] The UI holds no keys and does no crypto; the invitee's acceptance is
      produced by the node, not the browser (section 10).
- [ ] The verify screen never prints "unrevoked" and always shows the source
      and head sequence (section 6, flag R).
- [ ] tests: vitest plus testing-library component tests with a mocked API
      cover the org forms, their validation, an API error rendering and the
      equivocation result rendering.
- [ ] tests: `npm run build`, typecheck and lint pass.

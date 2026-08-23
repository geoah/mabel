# 021: membership HTTP routes, fixtures and the wallet wiring

- Status: done
- Depends on: 012, 018, 019

## Goal

The three membership routes exist on the wallet API with frozen fixtures, and
the ticket 019 membership screens call them instead of showing a code 70. This
retires `contracts/http/PENDING-membership.md`.

## Scope

- Freeze first, before the rest of this ticket and before ticket 019 starts:
  add `contracts/http/wallet-post-membership-invitations.json`,
  `-acceptances.json` and `-removals.json` for `POST
  /api/identities/:identity_id/memberships/invitations`, `/acceptances` and
  `/removals`, using the field names ticket 018 settles for the CLI. Add the
  `principals` field to the identity document fixtures, each principal carrying
  `identity_id`, `key` and `role`, plus the open invitations. Add the
  `inception`, `membership_invitation`, `membership_acceptance` and
  `membership_removal` `payload_kind` values to the event document, with the
  root variant inside the inception payload. Delete `PENDING-membership.md` and
  fold its four items into `contracts/README.md`.
- `POST /api/identities` gains an optional `founder`, keeping the frozen
  `declared_kind` spelling of the request body (proposal 002 section 6 writes
  it `kind`; the fixture name wins, per `contracts/README.md`).
- Implement the three routes in `crates/mabel-node` over the ticket 018 command
  paths, replacing the code 70 answers of ticket 012, and the membership service
  trait in `crates/mabel-node/src/api/`.
- The node produces the invitee's acceptance signature; the browser never signs
  (section 10).
- Wire the ticket 019 membership forms to the live routes and render their
  errors from the envelope.

## Acceptance criteria

- [x] `contracts/http/PENDING-membership.md` is gone and every item it listed is
      answered in `contracts/README.md`.
- [x] The three routes return the frozen documents; the identity document
      carries `principals` on every ledger, raw-rooted or identity-rooted.
- [x] The route paths spell `memberships`, and no `/orgs` route exists.
- [x] Admitting an already-used acceptance answers 409 with `code: 50` and
      `reason: acceptance_already_used`, matching `contracts/cli/errors.json`.
- [x] tests: one happy-path and one error test per route against an in-process
      server, asserting the fixture bodies.
- [ ] tests: the ticket 019 membership screens drive the three routes against a
      mocked API, and `npm run build`, typecheck and lint pass.

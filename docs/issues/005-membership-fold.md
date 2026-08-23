# 005: membership fold, admission and the last-controller rule

- Status: done
- Depends on: 001

## Goal

The fold accepts `MembershipInvitation`, `MembershipAcceptance` and
`MembershipRemoval` on every ledger, raw-rooted or identity-rooted, with the
lifecycle, admission and removal rules of proposal 002 section 4. Most of this
lands in the ticket 001 refactor; the criteria below are the checklist to verify
it against.

## Scope

- Invitation table in `LedgerState` for every ledger, keyed by event id with the
  invitee identity, key, role and a status of `open`, `accepted` or `cancelled`.
- Admission: the acceptance admits `(invitation.invitee, invitation.invitee_key,
  invitation.role)` read from the invitation event, never from the acceptance
  blob, which only has to match it. The outer event must be signed by a current
  `CONTROLLER`, and single use is branch-local.
- Acceptance binding: canonical `Acceptance` bytes, `ledger` equal to this
  ledger, `invitation_event` naming an `open` invitation, `invitee` and
  `invitee_key` equal to that invitation's, and the invitee signature verifying
  over `accept_input`.
- Invitation rules, all against the state folded from `0..=i-1`: an invitee with
  an `open` invitation is rejected, and an `invitee` equal to the ledger id is
  rejected so the root principal cannot be shadowed.
- Duplicate keys are rejected at the acceptance, where the principal is added;
  any invitation-time check is advisory. Promotion is the exception: an
  invitation naming an existing principal must carry that principal's current
  key, and its acceptance changes only the role.
- Removal: `target` names an identity, cancels its open invitation and removes
  its membership, whichever exist. It must leave at least one `CONTROLLER`
  counted over distinct keys. The raw root counts toward that minimum and is
  never removable; self-removal stays legal under the same rule.
- `MEMBER` is recorded data on every ledger, with no signing authority.
- `Attestation` records `signing_principal`, the matched principal's identity id
  plus the event's `author_key` (proposal 002 section 5).

## Acceptance criteria

- [x] Membership verification performs no cross-ledger lookup and yields no
      "unresolved" verdict.
- [x] Events signed by a controller before its removal stay valid.
- [ ] tests: on a raw-rooted ledger, invite, accept and promote a second
      `CONTROLLER`, then have it sign an event that folds without violation.
- [x] tests: an invitation is rejected for an invitee holding an open
      invitation, and for `invitee` equal to the ledger id.
- [x] tests: an acceptance is rejected for a duplicate principal key, and a
      promotion carrying a stale key for that principal is rejected.
- [ ] tests: four acceptance transplants are rejected, one each for another
      ledger, another invitation, another identity and another key, plus reuse
      on the same branch and an outer event signed by a non-controller.
- [x] tests: removal of the raw root is rejected, removal leaving no
      `CONTROLLER` is rejected, self-removal succeeds while another controller
      remains, and removal cancels an open invitation.
- [x] tests: a `MEMBER` signing an event is rejected.
- [x] tests: `Attestation.signing_principal` names the delegate that signed,
      not the ledger subject.

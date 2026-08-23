# 005: org ledger fold, membership and acceptance

- Status: open
- Depends on: 004

## Goal

The fold handles org ledgers: embedded inceptions, controller authority, the
invite lifecycle, cross-signed acceptance, removal and promotion, as specified
in proposal 001 sections 3.4 and 3.5.

## Scope

- Org branch of `State`: the member and controller map, the invite table with
  `open`, `accepted` and `cancelled`, seeded by `OrgInception` with the founder
  as `CONTROLLER` (sections 3.4 and 3.6 step 3).
- `OrgInception`, `OrgInvite`, `OrgAcceptance`, `OrgRemoval`, plus
  `WitnessConfig`, `TrustAttestation` and `TrustRevocation` on org ledgers
  (section 3.4, valid payloads by ledger kind).
- Embedded inception binding for `founder_inception` and `invitee_inception`
  using `verify_inception_standalone` from ticket 003, with the id and key
  equality checks of section 3.4.
- Authority: `author_key` must equal the active key the org ledger recorded for
  an identity whose role is `CONTROLLER` in the state from `0..=i-1`; `MEMBER`
  grants no signing authority (section 3.4).
- The stateful `OrgRemoval.target` row ticket 003 leaves open: the target must
  be a current member, controller or open invitee (section 3.4).
- Acceptance verification: all conditions listed in section 3.5, including
  branch-local single use.
- Invite lifecycle and removal semantics of section 3.4: a new invite is
  rejected only against an `open` invite for the same invitee, re-invite plus
  acceptance updates the role, removal cancels an open invite and removes
  membership and must leave at least one controller.

## Acceptance criteria

- [ ] Org verification performs no cross-ledger lookup and yields no
      "unresolved" verdict for membership (section 3.4).
- [ ] Events signed by a controller before its removal stay valid (section
      3.6, state boundary).
- [ ] tests: an `OrgInvite` whose embedded inception does not hash to
      `invitee`, and one whose `active_key` differs from `invitee_key`, are
      both rejected (section 11, field table bullet).
- [ ] tests: a `MEMBER` signing an org event is rejected (section 11).
- [ ] tests: an `OrgAcceptance` is rejected for each of malformed acceptance
      bytes, wrong `org`, unknown `invite_event`, an invite that is not `open`,
      mismatched `invitee`, mismatched `invitee_key`, an invalid invitee
      signature, reuse on the same branch, and an outer event signed by a
      non-controller (section 3.5).
- [ ] tests: invite over an open invite is rejected; re-invite plus acceptance
      promotes a member; removal of an unknown target is rejected; self-removal
      succeeds while another controller remains; removal of the last controller
      is rejected; removal cancels an open invite (section 3.4).
- [ ] tests: a valid founder, invite, acceptance, promotion and removal
      sequence folds with no violation and the expected controller set.

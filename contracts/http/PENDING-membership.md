# Membership surface is not frozen

Proposal 001 section 10 lists four mutating membership routes on the wallet
API. They are left out of the fixtures because decision 012 (full words in
names) renames `org` to `organization` everywhere user-visible, and proposal
002 reworks the whole membership model, so their final paths and payload
names are not settled.

Routes as proposal 001 section 10 spells them today:

| Method | Route | What it does |
|---|---|---|
| POST | `/api/orgs` | creates an organization ledger with a founder |
| POST | `/api/orgs/:id/invites` | appends an invitation naming an invitee and a role |
| POST | `/api/orgs/:id/acceptances` | appends the acceptance the invitee signed |
| POST | `/api/orgs/:id/removals` | removes a member, a controller or an open invitee |

The draft of `docs/proposals/002-unified-ledger.md` in this tree deletes the
`/orgs` routes and replaces them with
`POST /api/identities/:identity_id/memberships/invitations`, `/acceptances`
and `/removals`. Nothing here depends on either spelling.

## Also pending, for the same reason

- **Membership fields on the identity document.** Proposal 001 would give an
  organization a `founder` and a member list; draft 002 replaces both with a
  `principals` set of `(identity id, key, role)` on every ledger. The frozen
  identity document carries neither, so no consumer picks a side. An
  organization still reads back through `GET /api/identities` and
  `GET /api/identities/:identity_id` with `declared_kind: "organization"`,
  and there is no `GET /api/orgs` (proposal 001, clarifications).
- **`payload_kind` values for membership and inception events.** The frozen
  event document only carries `person_inception`, `witness_config`,
  `trust_attestation` and `trust_revocation`. Draft 002 collapses the two
  inception payloads into one `inception` with a root discriminator and
  names the membership payloads `membership_invitation`,
  `membership_acceptance` and `membership_removal`.
- **`POST /api/identities` mints persons only.** Creating an organization
  goes through the membership surface, because under proposal 001 an
  organization needs a founder and has no keys of its own (section 3.4).
- **Membership wording in error messages.** `contracts/cli/errors.json`
  spells the code 50 replay case with 001's words ("acceptance", "admitted").
  The exit code, the layer prefix and `details.reason` are frozen; the
  sentence is not.

When 002 is accepted, fixture the new routes here and fold these four items
into the frozen contract.

# 018: membership CLI commands and the file artifacts

- Status: open
- Depends on: 006, 008

## Goal

The three-step membership flow of proposal 002 section 6 runs end to end on one
machine through files: `identity export`, `mabel membership invite`, `accept`
and `admit`, plus removal and the principal display. It works on any ledger,
raw-rooted or identity-rooted.

## Scope

- Commands: `identity export`, `membership invite --ledger --by --invitee
  <descriptor-file> --role --out`, `membership accept <invitation-bundle> --as
  --out`, `membership admit --ledger --by <acceptance-file>`, `membership remove
  --ledger --by --target`, and `membership list --ledger` showing principals and
  open invitations. Ledger creation is `identity create --founder` (ticket 008).
- Hidden undocumented aliases `org` and `member` for `membership`, absent from
  `--help` (proposal 002 section 6).
- Artifact IO for the three files of proposal 001 section 3.8: `identity export`
  writes an `IdentityDescriptor`, `membership invite` writes an
  `InvitationBundle` holding events `0..=invitation`, `membership accept` writes
  an `AcceptanceFile` with a `signature` field.
- `membership accept` verifies the bundle's chain from inception, then displays
  the ledger's root variant, its current controllers and the offered role, and
  warns that a `CONTROLLER` offer on a raw-rooted ledger means signing as that
  identity, before it signs anything (proposal 002 section 4, accept surface).
- Caps enforced on every artifact read before allocation: 1 MiB, 4 KiB, 64 KiB
  (section 3.8, pitfall 7).
- `--json` documents for each command, using the ticket 008 envelope. These
  shapes are not frozen: `contracts/http/PENDING-membership.md` lists the
  membership surface as pending, and ticket 021 fixtures the HTTP counterpart
  (`POST /api/identities/:identity_id/memberships/invitations`, `/acceptances`,
  `/removals`) with the field names this ticket settles.

Out of scope: pushing any of this to a witness (ticket 011).

## Acceptance criteria

- [ ] Every command is spelled `membership`, with `org` and `member` accepted
      but undocumented.
- [ ] `membership invite` takes the invitee's `IdentityDescriptor` file, never a
      raw id and key, and the invitation embeds the invitee's inception.
- [ ] `membership accept` prints the summary and the raw-root warning before
      signing, and refuses a bundle whose prefix fails to fold.
- [ ] This ticket owns exit code 10: an over-cap or malformed artifact file
      exits 10 with the `Schema error:` prefix and the JSON error envelope.
- [ ] tests: `assert_cmd` over a temp home runs export, invite, accept, admit,
      promotion and removal against both a raw-rooted and an identity-rooted
      ledger, and asserts exit 50 with `reason: acceptance_already_used` for a
      replayed acceptance and exit 10 for each over-cap artifact.
- [ ] tests: `--json` shape stability for every command in this ticket.

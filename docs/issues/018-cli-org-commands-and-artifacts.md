# 018: CLI org commands and the file artifacts

- Status: open
- Depends on: 006, 008

## Goal

The three-step org flow of proposal 001 section 9 runs end to end on one
machine through files: `identity export`, `org invite`, `org accept` and
`org admit`, plus org creation, display and removal.

## Scope

- Commands: `identity export`, `org create --alias --founder`, `org show`,
  `org invite --org --by --invitee <descriptor-file> --role --out`,
  `org accept <invite-bundle> --as --out`, `org admit --org --by
  <acceptance-file>`, `org remove --org --by --member` (section 9).
- Artifact IO for the three files of section 3.8: `identity export` writes an
  `IdentityDescriptor`, `org invite` writes an `InviteBundle` holding org
  events `0..=invite`, `org accept` writes an `AcceptanceFile`.
- `org accept` verifies the bundle's chain from inception, displays the org,
  its controllers and the offered role, and only then signs (section 3.8).
- Caps enforced on every artifact read before allocation: 1 MiB, 4 KiB, 64 KiB
  (section 3.8, pitfall 7).
- `--json` documents for each command, using the ticket 008 envelope.

Out of scope: pushing any of this to a witness (ticket 011).

## Acceptance criteria

- [ ] `org invite` takes the invitee's `IdentityDescriptor` file, never a raw
      id and key, and the resulting invite embeds the invitee's inception
      (sections 3.4 and 9).
- [ ] `org accept` prints the org summary before signing and refuses a bundle
      whose prefix fails to fold (section 3.8).
- [ ] This ticket owns exit code 10: an over-cap or malformed artifact file
      exits 10 with the `Schema error:` prefix and a
      `{ok, code, message, details}` JSON body.
- [ ] tests: `assert_cmd` tests over a temp home run create, export, invite,
      accept, admit, promotion and removal, and assert exit 20 for an
      acceptance replayed to another invite and exit 10 for each over-cap
      artifact (sections 3.5 and 3.8).
- [ ] tests: `--json` shape stability for every command in this ticket.

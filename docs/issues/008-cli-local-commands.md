# 008: CLI local commands, exit codes and `--json`

- Status: open
- Depends on: 006, 007

## Goal

The `mabel` binary runs every command in proposal 001 section 9 that needs no
network: identity, trust, org, witness config and verify, with the section 9
exit codes, error prefixes and `--json` documents.

## Scope

- `crates/mabel-cli` with clap: global flags `--home`, `--json`, `--verbose`,
  `--allow-insecure-permissions` (section 9).
- Commands: `identity create|list|show|export`, `trust add|revoke|list`,
  `org create|show|invite|accept|admit|remove`, `witness add`,
  `verify ledger`, `verify trust`, `node id` (section 9).
- Artifact flow: `org invite` takes the invitee's `IdentityDescriptor` file and
  writes an `InviteBundle`; `org accept` verifies the bundle, shows the summary
  and writes an `AcceptanceFile`; `org admit` appends `OrgAcceptance` (sections
  3.8 and 9).
- Output: `--json` on every command with `{ok, code, message, details}` for
  errors; every `--json` verification result carries `source`, `head_seq`,
  `head_event`, `fetched_at` (section 6, flag R).
- Text output for verification uses the flag R wording, never "unrevoked", and
  includes the flag L sentence about subject control (section 6).
- `verify ledger` prints `valid to seq N, failed at seq M: <reason>` and exits
  20 on partial validity (section 3.6). `verify trust` is pinned as section 9
  specifies and exits 0 for both `trusted: true` and `trusted: false`.
- Exit codes 0, 2, 10, 20, 50, 60, 70 and the six error layer prefixes
  (section 9).

Out of scope: `sync`, `--from`, `--peer`, `witness run`, `wallet serve`
(tickets 010, 011, 012).

## Acceptance criteria

- [ ] Every command and flag spelled in section 9 exists with those names.
- [ ] Aliases resolve locally and are never signed; ids are authoritative
      (section 9).
- [ ] `verify trust` output is identical in text and `--json` content and
      matches the pinned rule in section 9, including the revoked-attestation
      count and event ids.
- [ ] tests: `assert_cmd` tests over a temp home cover each command, exit codes
      0, 2, 10, 20 and 60, `--json` shape stability, `verify trust` trusted and
      revoked cases, and the file-artifact caps of section 3.8 (section 11,
      CLI tests bullet).

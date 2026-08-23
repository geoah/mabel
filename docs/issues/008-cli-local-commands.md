# 008: CLI skeleton, output framework and identity, trust and node commands

- Status: done
- Depends on: 001, 007

## Goal

The `mabel` binary exists with the global flags, output rendering, `--json`
envelope and exit codes of proposal 001 section 9, and runs the identity,
trust, witness-config, verify and `node id` commands. Every `--json` document
matches its fixture in `contracts/cli/`.

## Scope

- `crates/mabel-cli` with clap: global flags `--home`, `--json`, `--verbose`,
  `--allow-insecure-permissions` (section 9).
- Output framework: `--json` on every command, errors rendered as
  `{ok, code, message, details}` with a stable snake_case `details.reason`, text
  errors prefixed with their layer, and the exit-code table of section 9. The
  frozen cases are `contracts/cli/errors.json`, one per code.
- `identity create --alias <a> [--kind person|organization|agent|service]
  [--founder <alias|id>]`: `--founder` selects an identity root naming that
  identity as the founding principal, its absence a raw root. `--kind` is the
  declared kind and gates nothing (proposal 002 sections 3 and 6).
- Commands: `identity create|list|show`, `trust add|revoke|list`,
  `witness add`, `verify ledger`, `verify trust`, `node id` (section 9).
- `identity rotate`, a stub that exits 70 with the message `key rotation is not
  part of this POC` (decisions/008, section 9 code 70).
- `--json` shapes come from `contracts/cli/identity-create.json`,
  `identity-list.json`, `trust-add.json`, `verify-ledger.json` and
  `verify-trust.json`: every key present, absent values `null`, timestamps as
  `*_ms` numbers, bytes as unpadded lowercase base32. A raw-rooted identity
  carries `active_key` and `reserve_commit`; an identity-rooted one does not.
- Text output uses the flag R wording, never "unrevoked", and includes the flag
  L sentence about subject control (section 6).
- `verify ledger` prints `valid to seq N, failed at seq M: <reason>` and exits
  20 on partial validity (section 3.6). `verify trust` is pinned as section 9
  specifies and exits 0 for both `trusted: true` and `trusted: false`.

Out of scope: membership commands, the three file artifacts and `identity
export` (ticket 018); all network commands (tickets 011 and 012). The
`principals` view of an identity is not frozen
(`contracts/http/PENDING-membership.md`), so no command in this ticket emits it.

## Acceptance criteria

- [ ] Every command and flag this ticket owns is spelled as in proposal 001
      section 9 and proposal 002 section 6.
- [ ] `identity create` with no `--founder` produces a raw root, and with
      `--founder` an identity root whose founding principal is that identity.
- [ ] Aliases resolve locally and are never signed; ids are authoritative.
- [ ] This ticket owns exit codes 0, 2, 20, 60 and 70: a successful `identity
      list`, an unknown flag, `verify ledger` on a tampered ledger, a 0644
      `active.key`, and `identity rotate`. Each has a test asserting the code,
      the JSON body and the text prefix.
- [ ] tests: `assert_cmd` over a temp home covers each command and asserts each
      `--json` document against the matching `contracts/cli/` fixture, key for
      key, including the `verify trust` trusted and revoked cases.

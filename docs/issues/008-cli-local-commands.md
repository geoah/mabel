# 008: CLI skeleton, output framework and identity, trust and node commands

- Status: open
- Depends on: 005, 007

## Goal

The `mabel` binary exists with the global flags, output rendering, `--json`
envelope and exit codes of proposal 001 section 9, and runs the identity,
trust, witness-config, verify and `node id` commands.

## Scope

- `crates/mabel-cli` with clap: global flags `--home`, `--json`, `--verbose`,
  `--allow-insecure-permissions` (section 9).
- Output framework: `--json` on every command, errors rendered as
  `{ok, code, message, details}`, text errors prefixed with their layer
  (`Schema error:`, `Ledger error:`, `Policy error:`, `State error:`,
  `Replay error:`, `Network error:`), and the exit-code table of section 9.
- Commands: `identity create|list|show`, `trust add|revoke|list`,
  `witness add`, `verify ledger`, `verify trust`, `node id` (section 9).
- `identity rotate`, a stub that exits 70 with the message `key rotation is not
  part of this POC` (decisions/008, section 9 code 70).
- Every `--json` verification result carries `source`, `head_seq`,
  `head_event`, `fetched_at`; text output uses the flag R wording, never
  "unrevoked", and includes the flag L sentence about subject control
  (section 6).
- `verify ledger` prints `valid to seq N, failed at seq M: <reason>` and exits
  20 on partial validity (section 3.6). `verify trust` is pinned as section 9
  specifies and exits 0 for both `trusted: true` and `trusted: false`.

Out of scope: org commands, the three file artifacts and `identity export`
(ticket 018); all network commands (tickets 011 and 012).

## Acceptance criteria

- [ ] Every command and flag this ticket owns is spelled as in section 9.
- [ ] Aliases resolve locally and are never signed; ids are authoritative
      (section 9).
- [ ] `verify trust` output is identical in text and `--json` content and
      matches the pinned rule in section 9, including the revoked-attestation
      count and event ids.
- [ ] This ticket owns exit codes 0, 2, 20, 60 and 70: a successful
      `identity list`, an unknown flag, `verify ledger` on a tampered ledger, a
      0644 `active.key`, and `identity rotate`. Each has a test asserting the
      code, the `{ok, code, message, details}` JSON body and the text prefix.
- [ ] tests: `assert_cmd` tests over a temp home cover each command, `--json`
      shape stability and the `verify trust` trusted and revoked cases
      (section 11, CLI tests bullet).

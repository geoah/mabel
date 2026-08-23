# 001: unified proto schemas, regenerated vectors and workspace checks

- Status: done
- Depends on: none

## Goal

`proto/mabel/v0/*.proto` carries the unified schema of proposal 002 section 7,
`mabel-proto` generates from it, the golden and rejection vectors regenerate
under the new names, and the workspace passes fmt, clippy and tests. A refactor
agent is implementing this now; this ticket records what a reviewer checks.

The workspace layout, the dependency table, `mabel-proto`'s `build.rs` and the
`iroh-base` key probe of proposal 001 section 7 are done and out of scope.

## Scope

- `ledger.proto` rewritten in place (proposal 002 section 7): `DeclaredKind`,
  `Role`, `RawRoot`, `IdentityRoot`, one `Inception` with the root `oneof`,
  `MembershipInvitation`, `MembershipAcceptance`, `MembershipRemoval`,
  `Acceptance`, payload tags 10 to 16 and `reserved 20 to 29`. Every `Org*` name
  disappears and `sig` becomes `signature`, including in `SignedEvent`.
- `files.proto`: `InviteBundle` becomes `InvitationBundle`, its `org_prefix`
  becomes `ledger_prefix`, `AcceptanceFile.sig` becomes `signature`. Parsing is
  ticket 006.
- `sync.proto`: `LedgerSummary.kind` becomes `DeclaredKind declared_kind`, and
  the `mabel-net` descriptor accepts enum values 3 and 4. Ticket 009 is already
  done, so this edit rides here.
- `proto/mabel/v0/README.md` and the field table it carries: the inception and
  `Org*` rows are replaced by proposal 002 section 8.
- `test-vectors/`: all nine golden vectors regenerate. `05-org-inception`
  becomes `05-identity-root-inception`, `06` to `08` become the membership
  vectors, and a new vector covers a raw-rooted ledger adding a second
  controller. The rejection set takes the renames, deletions and the nine
  additions listed in proposal 002 section 10.
- One task running `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings` and `cargo test --workspace`.
- Root `README.md` carries the "verified means" sentence of proposal 001
  section 1 and the flag L sentence of section 6.

The descriptor, fold and stable-code changes of proposal 002 section 10 land in
the same refactor, against the committed code of tickets 003 and 004. Ticket 005
covers the membership fold.

## Acceptance criteria

- [x] `ledger.proto` matches proposal 002 section 7 field for field, including
      the payload tags and `reserved 20 to 29`.
- [x] `grep -ri` over `proto/`, `crates/` and `test-vectors/` finds no `Org`
      message name, no `sig` field and no `person_inception` payload name.
- [x] `files.proto` names `InvitationBundle.ledger_prefix` and
      `AcceptanceFile.signature`; `sync.proto` names `declared_kind`.
- [x] The vector set carries the names above and regenerating twice produces
      byte-identical files.
- [x] `mabel-proto` contains only `build.rs` and re-exports, and `cargo tree -p
      mabel-core` lists neither `tokio` nor `iroh` proper.
- [ ] Root `README.md` contains both sentences verbatim.
- [x] tests: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
      and `cargo test --workspace` all pass.

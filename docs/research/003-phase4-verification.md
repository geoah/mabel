# 003: phase 4 verification of tickets 001 to 022

- Date: 2026-08-23
- Sources: `docs/issues/001` to `022`, the repository at commit `8399dda`,
  `docs/proposals/001-architecture.md` sections 3.1, 3.2, 3.4, 3.5, 5, 9, 10,
  11 and 12, `docs/proposals/002-unified-ledger.md` sections 4, 6, 7 and 8,
  `docs/proposals/003-wallet-ux-dns-and-trust-graph.md` section 6,
  `contracts/README.md`, `contracts/http/`, `contracts/cli/`, `test-vectors/`,
  and the test binaries of `mabel-core`, `mabel-net`, `mabel-node`,
  `mabel-cli` and `ui`.

## Findings

Every acceptance criterion of tickets 001 to 022 was checked against the
artifact it names: the test function body, the source line, the fixture file,
the clap definition or the command output. The ticket's own status line was not
treated as evidence.

122 criteria checked, 104 pass, 13 fail, 5 unverifiable in this environment.
Ticket 019's 6 criteria are excluded: proposal 003 section 6 folds ticket 019
into ticket 027, so its status line now reads "superseded by proposal 003
ticket cut" and its boxes stay unticked.

| Ticket | Criteria | Pass | Fail | Unverified |
|---|---|---|---|---|
| 001 workspace and proto schemas | 7 | 6 | 1 | 0 |
| 002 canonical encoding and digests | 7 | 7 | 0 | 0 |
| 003 wire-format validator | 5 | 5 | 0 | 0 |
| 004 ledger fold | 6 | 6 | 0 | 0 |
| 005 membership fold | 9 | 7 | 2 | 0 |
| 006 file artifacts and fork records | 6 | 6 | 0 | 0 |
| 007 node home and storage | 7 | 7 | 0 | 0 |
| 008 CLI local commands | 5 | 5 | 0 | 0 |
| 009 mabel-net sync protocol | 5 | 5 | 0 | 0 |
| 010 witness runtime | 8 | 8 | 0 | 0 |
| 011 wallet sync and verify | 7 | 7 | 0 | 0 |
| 012 HTTP API and loopback rules | 6 | 4 | 2 | 0 |
| 013 UI shell and wallet route | 6 | 3 | 3 | 0 |
| 014 witness UI route | 5 | 5 | 0 | 0 |
| 015 docker image and compose | 4 | 2 | 1 | 1 |
| 016 CLI integration and fresh verifier | 4 | 4 | 0 | 0 |
| 017 demo script | 4 | 1 | 1 | 2 |
| 018 CLI membership commands | 6 | 6 | 0 | 0 |
| 019 wallet principals and verify screens | 6 | - | - | superseded |
| 020 API contract fixtures | 5 | 3 | 2 | 0 |
| 021 membership HTTP routes | 6 | 5 | 1 | 0 |
| 022 mobile-friendly UI | 4 | 2 | 0 | 2 |
| **Total** | **122** | **104** | **13** | **5** |

### Global gates

Run twice: once on the committed tree at `8399dda`, the baseline for the
per-ticket walk, and once at the end on the working tree while a concurrent fix
agent was editing `mabel-core` and `mabel-net`.

| Gate | On `8399dda` | On the working tree, retry | Wall time |
|---|---|---|---|
| `cargo fmt --all -- --check` | pass, no diff | **fail**, 3 diffs in `crates/mabel-core/src/fold.rs` at 2389, 2425, 2432 | 0s |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 warnings | pass, 0 warnings | 4s |
| `cargo test --workspace` | pass, 412 passed, 0 failed, 1 ignored | pass, 416 passed, 0 failed | 15s / 24s |
| `npm test` (ui) | pass, 7 files, 44 tests | pass, 7 files, 44 tests | 4s |
| `npm run build` (ui) | pass, one 313.72 kB chunk | pass | 1s |
| `npm run lint` (ui) | pass (`eslint . && tsc -b`) | pass | 3s |

Total gate wall time is about 30 seconds on a warm `target/`; the Rust runs
above rebuilt only the crates the fix agent touched.

The fmt failure is the fix agent's unformatted in-flight edit, not a property of
the committed tree: the first attempt also hit a failing
`no_rejection_file_is_stale` (`crates/mabel-core/tests/rejections.rs:2062`)
against a half-written vector set, and the retry a minute later passed it. The
test count rose from 412 to 416 because that agent added
`an_acceptance_demoting_the_last_controller_is_rejected`,
`a_controller_may_be_demoted_once_another_controller_exists` and
`the_raw_root_is_never_demoted` to `crates/mabel-core/src/fold.rs`. Those are
demotion tests and do not touch either 005 failure below, which were re-checked
against the edited tree and still hold.

The one ignored Rust test is `ledger::tests::crash_child_dies_before_the_head_rename`,
which runs as a child process of `a_killed_append_leaves_a_shorter_valid_ledger`.

### Failures

**001 criterion 6, root `README.md` contains both sentences verbatim.**
`README.md` is 7 bytes, `# mabel`, unchanged since the initial commit `530aef4`.
Neither the "Verified means" sentence of proposal 001 section 1 line 47 nor the
flag L sentence of section 6 line 500 is present. The verified-means sentence
does exist in verifier output, at
`crates/mabel-node/src/api/documents.rs:31`.

**005 criterion 3, a promoted `CONTROLLER` signs an event that folds.**
The composite is split and the promote-then-sign link is untested.
`fold::tests::a_raw_rooted_ledger_delegates_signing_to_a_second_controller`
(`crates/mabel-core/src/fold.rs:1781`) invites as `CONTROLLER` and signs, with
no promotion.
`fold::tests::a_member_is_promoted_by_a_second_invitation_carrying_the_same_key`
(`fold.rs:1882`) promotes but asserts only `principal().role` and
`invitations().len()`; the promoted key never signs. The CLI twin
(`crates/mabel-cli/tests/membership.rs:449`) stops at `membership list`.

**005 criterion 6, an outer acceptance event signed by a non-controller.**
Five of the six listed cases have tests; this one has none. Every `admit(...)`
call site in `crates/mabel-core/src/fold.rs` (lines 1803, 1858, 1894, 1902,
1935, 2005, 2012, 2120, 2156, 2188, 2235, 2267, 2313, 2347, 2541) passes
`&secret(1)`, the root controller, as the outer signer. No rejection vector
covers it either.

**012 criterion 3, the membership routes answer 501 with `code: 70`.**
They answer 200. `crates/mabel-node/src/api/wallet.rs:33-48` registers working
handlers and `the_membership_routes_spell_memberships_and_answer_the_frozen_documents`
(`crates/mabel-node/src/api/tests.rs:1014`) asserts `StatusCode::OK`. Ticket 021
superseded this criterion but nobody amended the ticket text, so as written it
is false.

**012 criterion 4, `--ui-dir` serves the bundle from disk.**
No such flag exists. `mabel wallet serve` takes only `--http`, `--iroh-port`
and `--peer` (`crates/mabel-cli/src/cli.rs:372-385`), `mabel witness run` the
same (`:292-302`), and `crates/mabel-cli/src/commands/wallet_serve.rs:55-60`
leaves `WalletOptions.ui` at its default. The library half exists and is unused:
`UiSource::from_option` at `crates/mabel-node/src/api/ui.rs:43-47`. The
default-`127.0.0.1`-and-warn half of the criterion passes
(`crates/mabel-node/src/api/bind.rs:16-17`, `:35-42`).

**013 criterion 2, client types derived from `contracts/http/`, a fixture
parses with no missing or extra key.** `interface Identity`
(`ui/src/api/types.ts:58-68`) omits `principals` and `open_invitation_count`,
both frozen in `contracts/http/wallet-get-identities.json:18,26` and
`contracts/http/wallet-get-identity.json:29`. No test asserts that a fixture
document parses with no missing or extra key. TypeScript does not catch the
drift because the fixtures are non-literal JSON imports, so excess-property
checking never runs.

**013 criterion 3, pinned dependency versions match section 12.**
`ui/package.json:42` pins `typescript` 5.9.3 against section 12's 7.0.2
(`docs/proposals/001-architecture.md:701`), and `:40` pins `playwright`
`^1.57.0`, a different package and a floating range, against section 12's
`@playwright/test` 1.62.1. `react` 19.2.8, `vite` 8.2.2 and `tailwindcss`
4.3.3 do match. The one-bundle half passes: `dist/assets/index-Ckrym4Ht.js`,
a single chunk covering both routes.

**013 criterion 6, the built bundle is served by `wallet serve` from the embed
and via `--ui-dir`.** Build, typecheck and lint pass. The serving half is
unsatisfiable for the same reason as 012 criterion 4, and no test serves the
bundle through `wallet serve` at all: the only UI-serving test,
`the_ui_bundle_serves_outside_api_and_never_inside_it`
(`crates/mabel-node/src/api/tests.rs:1102-1126`), builds a `UiSource::Directory`
from a tempdir against a bare `wallet_router`.

**015 criterion 3, each wallet's `peers.json` contains the witness's ticket
before its first command runs.** Nothing writes `peers.json`.
`docker/entrypoint.sh:137` appends the ticket to argv instead
(`set -- "$@" --peer "$ticket"`), and `docker/README.md:94-96` records the
deviation without the ticket carrying a Deviations section.

**017 criterion 2, the script uses only the CLI surface of section 9.**
Phase 11 leaves it: `demo/run-demo.sh:273-274` calls
`curl -fsS http://127.0.0.1:9080/api/ledgers` and pipes it through `jq`
(`:277`), which is the section 10 HTTP debug API. Phases 1 to 10 do use the CLI
and the seeded ticket (`:120-127`).

**020 criterion 2, every `mabel` command that takes `--json` has a fixture.**
`--json` is a global flag (`crates/mabel-cli/src/cli.rs:26`), so all 21
subcommands take it, and `contracts/cli/` holds 6 command fixtures. Commands
with a documented `--json` document and no fixture: `trust revoke`
(`crates/mabel-cli/src/documents.rs:78`), `trust list` (`:103`), `witness add`
(`:116`), `sync fetch` (`:155`), `node id` (`:177`), `identity export`
(`:314`), `membership invite` (`:334`), `membership accept` (`:395`),
`membership admit` (`:408`), plus `identity show`, `identity rotate`,
`membership remove`, `membership list`, `wallet serve` and `witness run`.

**020 criterion 5, fields that do not apply are present and `null`.**
`contracts/http/wallet-get-identities.json:9-27`, the identity-rooted entry,
omits `active_key` and `reserve_commit`; the raw-rooted sibling at `:29-60`
carries both. `contracts/README.md:186-189` codifies the omission and
`contracts/README.md:98-101` forbids it in the same file. No test guards it.

**021 criterion 6, the membership screens drive the three routes against a
mocked API.** There are no membership screens.
`ui/src/routes/wallet/PrincipalsPanel.tsx:16-18` is a placeholder reading "The
membership surface is not frozen yet, so this node serves no principals field."
`ui/src/mocks/handlers.ts` and `ui/src/api/client.ts` contain no membership
call, and `ui/src/test/` has no membership test. This criterion depends on
ticket 019, which proposal 003 supersedes, so it is inherited by ticket 027.

### Unverifiable here

- **015 criterion 4** and **017 criteria 1 and 4** need `docker compose up`
  and a real build, which this run did not perform. The artifacts exist:
  healthchecks on all three services (`docker/compose.yaml:66-71`, `:89-94`,
  `:112-117`), the no-egress overlay `docker/compose.internal.yaml:9`
  (`internal: true`), and `docker/smoke.sh:75-95`. One caveat found by reading:
  `docker/README.md:77-78` says `smoke.sh` cannot be driven from the host under
  the internal overlay, so the two halves of 015 criterion 4 are never
  exercised in one committed run. For 017 criterion 4, no CI or task harness
  invokes `demo/run-demo.sh`: there is no `.github/` directory and `mise.toml`
  declares no tasks.
- **022 criteria 1 and 4** rest on screenshots. No vitest test asserts page
  overflow; the only mechanical check is `measureOverflow` in
  `ui/scripts/screenshots.mjs:71-90`, which needs a served build and a browser.
  42 PNGs at 360x780, 768x1024 and 1280x800 exist under `ui/screenshots`, but
  `ui/.gitignore:5` ignores that directory, so nothing in the repository records
  that the run was clean.

### Things that look done but are not

- **Ticket 020's checkboxes were already all ticked** before this verification,
  including criteria 2 and 5, which fail. This verification may only add ticks,
  so those two remain ticked and wrong. They need unticking by hand.
- **Ticket 019 was `Status: open`, not `done`**, despite being counted among
  the finished set.
- **`ui/` is a route behind the contracts.** Three of the 13 failures (013
  criterion 2, 021 criterion 6) plus the stale `person_inception` spellings in
  `ui/src/mocks/store.ts:240,251`, `ui/src/mocks/fixtures.ts:162-169` and the
  doc comment at `ui/src/api/types.ts:24-26` are all one drift: the UI still
  encodes the pre-proposal-002 contract. The contracts themselves are current,
  `contracts/http/witness-get-ledger-events.json:23` spells `inception`, and
  `contracts/README.md:206-218` freezes the seven `payload_kind` values.
- **One intermittent test failure.** On one run of `cargo test -p mabel-node`,
  `a_source_holding_a_strict_prefix_loses_without_an_equivocation_report`
  panicked at `crates/mabel-node/tests/wallet.rs:496` with "the longer candidate
  wins / left: 1 / right: 2". An identical rerun passed. Not covered by any
  criterion.
- **Stale references to deleted files.** `contracts/http/PENDING-membership.md`
  is gone, but `crates/mabel-cli/src/documents.rs:15`,
  `crates/mabel-cli/tests/membership.rs:6` and
  `ui/src/routes/wallet/PrincipalsPanel.tsx:5` still cite it in comments.

### Weak spots that stop short of failing

- **008 criterion 4**: the exit-0 case is asserted only in `--json` mode
  (`crates/mabel-cli/tests/cli.rs:230`, `:242`); no test runs `identity list` in
  text mode. Every error code does assert its text prefix.
- **009 criterion 5**: the frame cap is tested only from above
  (`crates/mabel-net/tests/protocol.rs:447`). The single-event cap (`:456`) and
  the push count cap (`:476`) each test both sides.
- **006 criterion 4**: `validate_fork_record` is genuinely the single
  implementation, called by the witness at
  `crates/mabel-node/src/witness/storage.rs:701`, but no reader calls it today,
  so the "and readers" half is unexercised.
- **005 criterion 5**: `Reason::PrincipalKeyMismatch` is unreachable in practice
  and appears only in `violation_codes_are_stable`; the test comment at
  `crates/mabel-core/src/fold.rs:1921-1925` says so. The stated rejection is
  still asserted, by `inception_key_mismatch`.
- **004's wording predates proposal 002**: it names `PersonInception`,
  `OrgRemoval` and "payload wrong for the ledger kind". Each criterion was
  judged by its substance under the unified model, where declared kind gates
  nothing (`fold::tests::declared_kind_gates_no_payload`) and the surviving rule
  is the position rule (`check_position`, `crates/mabel-core/src/fold.rs:805`).

## Recommendations

1. Untick `docs/issues/020` criteria 2 and 5 and fix them: add the missing
   `contracts/cli/` fixtures, and settle the nullability contradiction between
   `contracts/README.md:98-101` and `:186-189` in one direction.
2. Amend ticket 012 criterion 3 to match ticket 021's outcome, and either add
   the `--ui-dir` flag to `wallet serve` and `witness run` or drop it from 012
   criterion 4 and 013 criterion 6.
3. Fold 013 criterion 2 and 021 criterion 6 into ticket 027 with 019's, and add
   the missing-or-extra-key parse test there so the UI cannot drift from the
   frozen fixtures again.
4. Add the two missing `mabel-core` tests for 005: a promoted controller signing
   an event, and a `MembershipAcceptance` outer event signed by a
   non-controller.
5. Decide 015 criterion 3 and 017 criterion 2 as amendments or as work: either
   make a runtime read `peers.json` and seed it, or record the argv approach as
   a deviation in the ticket; either drop the `curl` phase from the demo or
   widen the criterion to allow the read-only HTTP API.
6. Update the root `README.md` with the two sentences 001 criterion 6 requires.
7. Reproduce and fix the intermittent
   `a_source_holding_a_strict_prefix_loses_without_an_equivocation_report`
   failure before it hides a real regression.

# 020: frozen HTTP and `--json` contract fixtures

- Status: done
- Depends on: none

## Goal

`contracts/` holds the wire contract between the node HTTP APIs, the `mabel
--json` output and the two UI routes, as example documents. UI, CLI and HTTP
tickets are built against the fixtures instead of against each other, so they
run in parallel rather than in a chain.

## Scope

- `contracts/http/*.json`: one file per route, each holding `route`, `method`,
  `request`, `response` and `errors`, covering the wallet and witness route
  lists of proposal 001 section 10.
- `contracts/cli/*.json`: one file per `--json` command, each holding `command`
  and `cases` with `case`, `command`, `exit_code` and `document`, plus
  `errors.json` with one case per exit code and layer prefix.
- `contracts/README.md`: the index, the naming, id, number, timestamp,
  nullability, ordering and paging conventions, the `ok` envelope, the exit-code
  to HTTP-status table, the verification report shape, the shared identity and
  event documents, and the decisions taken in the fixtures.
- `contracts/http/PENDING-membership.md`: what is deliberately not frozen, the
  membership routes and the membership fields of the identity document.
- Fixture data reuses the identities and event ids of `test-vectors/`, so a
  fixture and a golden vector name the same event.

## Acceptance criteria

- [x] Every route of proposal 001 section 10 has a fixture, or is listed in
      `PENDING-membership.md` with the reason.
- [x] Every `mabel` command that takes `--json` has a fixture with one case per
      outcome worth pinning.
- [x] One error case per exit code, each with a `details.reason`.
- [x] Every field name follows decision 012: snake_case, full words, no
      abbreviation, and one name per thing across both surfaces.
- [x] Fields that do not apply are present and `null`, so a consumer can rely
      on every key existing.

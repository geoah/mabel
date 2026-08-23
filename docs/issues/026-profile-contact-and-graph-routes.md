# 026: profile, contact, verification, lookup and graph routes

- Status: open
- Depends on: 023, 024, 025

## Goal

The fixtures land first, then the axum handlers, the CLI `--json` renderers and
the UI types, carrying the shared identity document, `ResolvedIdentity`, the
local contact store and lookup with `from`, per proposal 003 section 5.

## Scope

- Freeze first, before any handler: the seven new `contracts/http/` fixtures and
  the four new `contracts/cli/` fixtures section 5 names, plus the edits to
  `wallet-get-identity.json` and `wallet-get-identities.json`.
- `contracts/README.md`: index rows for each new fixture, the `profile_update`
  payload row, the four CLI rows, the explicit-nulls note, and the line
  recording that proposal 003 amends the payload-table freeze.
- Both identity routes return one document with the `profile`, `verification`
  and `contact` objects of section 5, explicit nulls rather than omitted keys.
- `ResolvedIdentity` as section 4 defines it, returned everywhere a foreign
  identity appears: selector rows, trusted lists, path hops, lookup headings,
  expansions and reverse edges, with the resolution order section 4 gives.
- The contact store at `contacts/<identity_id>.json` per section 1, valid for
  foreign ids, never signed or synced, and separate from `IdentityMeta`.
- New routes: profile replace (409 `no_op_profile_update`), forced
  verification, contact GET and PUT, lookup with `from`, graph read and graph
  sync.
- CLI `mabel profile replace`, `mabel contact set`, `mabel graph sync` and
  `mabel lookup <id> --from <alias|id>`, with the replacement diff and
  confirmation of section 1 and `--yes` to skip it.

## Acceptance criteria

- [ ] Every new fixture exists and is indexed in `contracts/README.md` before
      the handlers land.
- [ ] `GET /api/identities` and `GET /api/identities/:identity_id` parse into
      one type with no key present in one and absent in the other.
- [ ] `GET /api/identities` triggers no DNS lookup (section 2).
- [ ] A lookup for an identity absent from the graph is a 200 with `degrees:
      null` and an empty path list, not a 404 (section 5).
- [ ] tests: one happy-path and one error test per new route against an
      in-process server, asserting the fixture bodies.
- [ ] tests: a profile replace whose effect equals the current profile answers
      409 `no_op_profile_update`.
- [ ] tests: a profile body missing either key is refused; either key may be
      null.
- [ ] tests: `mabel profile replace` prints the before-and-after diff and asks
      for confirmation unless `--yes` is given.

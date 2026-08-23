# 027: wallet shell, name resolution and developer mode

- Status: done
- Depends on: 026

## Goal

The wallet gains the address-book shell: an identity selector, one component
that renders every foreign identity by name, and a developer-mode toggle that
holds everything the current screens show by default, per proposal 003
section 4.

## Scope

- Identity selector in `ui/src/routes/wallet/`, listing this node's identities
  as resolved names with ids beside them, remembering the last choice in
  `localStorage`. The selection is the default `from` for lookups.
- A `ResolvedIdentity` component rendering the contract object of section 4,
  enforcing the anti-spoofing rules there so no screen can forget them: name
  styling distinct from id and hostname styling, the id always beside the name,
  full ids when two entries in one list resolve to the same name, and no
  sorting, matching or deduplication on a name.
- `ui/src/components/Identifier.tsx` gains the name slot it has no equivalent
  of today, and the resolved-name path goes through it.
- Developer mode: a header-menu toggle, default off, persisted under
  `mabel.developer_mode`, revealing head event ids, witness endpoint ids,
  principal keys, sync freshness, fork and crawl provenance, and the raw
  response document. Nothing is removed from the product.
- Consent panels of section 4, shown before the first hostname publication and
  before the first graph sync, remembered per node home.
- A graph-sync button in the header showing counts, `truncated` and
  `truncated_by`.
- `ui/src/api/types.ts`, `ui/src/mocks/store.ts` and the existing component
  tests updated for the ticket 026 documents.

## Acceptance criteria

- [ ] Every existing panel remains reachable with developer mode on; none is
      deleted (section 4).
- [ ] The selector's choice survives a reload and is the `from` sent with a
      lookup.
- [ ] tests: a display name of `alice.example` renders in the name style, never
      the hostname style, and its id is shown beside it.
- [ ] tests: two entries in one list resolving to the same name both render
      their full ids.
- [ ] tests: name resolution tries the profile display name, then the alias or
      contact nickname, then the truncated id.
- [ ] tests: the consent panel appears once per node home before the first
      graph sync and before the first hostname publication.
- [ ] tests: `npm run build`, typecheck, lint and vitest pass.

# 029: lookup view for a foreign identity and the graph surfaces

- Status: open
- Depends on: 026, 027

## Goal

Looking up an identity the wallet does not control answers "how do I know this
person" from the selected root: the same overview table, the path in named
hops, their trust list and the best-effort reverse list, per proposal 003
sections 3 and 4.

## Scope

- Lookup route in `ui/src/routes/wallet/` calling
  `GET /api/lookup/:identity_id?from=<identity_id>`, defaulting `from` to the
  ticket 027 selector's choice.
- Overview table for the foreign identity, the same shape ticket 028 builds,
  plus its verification status.
- Path rendering: the shortest path length in edges, up to three shortest paths
  as hops, every hop a `ResolvedIdentity`.
- Their outgoing trust list, and the reverse list always labelled best-effort
  and always worded as who in this crawl trusts them, never who trusts them.
- Expansion one level in place, capped at two levels.
- Staleness and truncation surfaces: `graph_stale`, `graph_truncated`,
  `truncated_by`, per-hop `fetched_at_ms` and `stale`, and equivocation shown
  on the hop that recorded it.
- `degrees: null` stated as "shortest path found in this crawl", never as "no
  relationship" (section 3).
- A graph view listing the current generation's nodes with their depth, roots
  and staleness, and the sync control's counts.
- `data-testid` attributes on the controls and result regions.

## Acceptance criteria

- [ ] The reverse list carries its best-effort label on every render, not only
      the first (section 3).
- [ ] Expansion stops at two levels, so a lookup cannot walk the whole graph.
- [ ] tests: a `degrees: null` response renders the crawl wording and an empty
      path list, and is not treated as an error.
- [ ] tests: a stale graph, a truncated graph and a stale hop each render their
      marker.
- [ ] tests: an equivocation on a path node renders on that hop with both event
      ids.
- [ ] tests: changing the selected identity re-issues the lookup with the new
      `from`.
- [ ] tests: `npm run build`, typecheck, lint and vitest pass.

# 016: Trust graph

- Date: 2026-08-24
- Status: accepted
- Source: product owner

- The wallet builds a local trust graph by crawling outward from its own
  identities' trust lists through witnesses: the people I trust, who they
  trust, and so on, to a configurable depth.
- Looking up an identity shows degrees of separation and the path (I trust
  Theo, Theo trusts Alice, Alice trusts Bob, Bob trusts Clarabel: four
  levels), plus best-effort "who trusts them" computed from my crawled
  network only.
- Keep it simple first: a manual synchronize action with a staleness
  indicator (stale after 24 hours), a depth setting, cached results with
  fetched-at times. Periodic background sync can come later.
- Witnesses may also crawl ledgers referenced by trust events they hold,
  recording why each ledger was pulled (pushed directly versus referenced
  by another ledger). Optional, second priority.

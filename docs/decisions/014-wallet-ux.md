# 014: Wallet UX principles

- Date: 2026-08-24
- Status: accepted
- Source: product owner

- The wallet is an address book over a web of trust, built for a user, not
  a developer. The primary view stays clean; sequencing, hashes, caches and
  provenance live behind a developer mode toggle in a menu.
- The wallet page selects among local identities at the top. An identity
  view shows, briefly: name, copyable id, created date, contact metadata,
  verification status, and who they trust, with names resolved.
- Key-value information renders as a compact table (key and value on one
  line), never stacked label-over-value lists.
- The ledger renders as event type plus sequence, one line each,
  expandable per event for detail.
- Below the state come actions (trust someone, invite, and so on), each
  with a one-line description of what it does.
- Drill-down: expanding a trusted person shows their metadata, their
  verification, who they trust, and best-effort who trusts them, one or
  two levels deep.

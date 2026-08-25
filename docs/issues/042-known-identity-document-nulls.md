# 042: a never-checked handle reads `unverified` and triggers a DNS lookup

- Status: open
- Depends on: 041

## Goal

Every ledger this node stores projects its folded profile, its advertised
endpoints and whatever verdict this node has cached, and a handle nobody has
checked says so in its own word instead of borrowing the word for a lookup
that found nothing. Reading an identity queries no zone.

## Problem

A wallet and a witness at v0.14.0 reported `profile`, `endpoints` and
`verification` empty on a stored ledger whose `GET
/api/identities/{id}/ledger` returned the `profile_update` and the
`endpoint_advertisement`. Two separate things are behind that report.

The projection is not one of them. `LoadedLedger::identity_document` folds the
stored chain for any ledger under `ledgers/`, controlled or not, and
`Names::resolve` folds the same copy for the `known` row. A ledger pushed over
the sync path and never crawled projects its name, handle, email and endpoints
today; the new `node_routes` test asserts it against the unchanged builder.
`endpoints` is a `Vec` and `verification` is not an `Option`, so neither can
serialize as `null` at all.

What is real is the verdict:

- `Verification::unchecked` spelled "this node has never looked" as `status:
  "unverified"`, the word for a lookup that ran and found no `mabel=` record.
  `KnownIdentity` and `ResolvedIdentity` carry the status string and no
  timestamp, so in a list the two states were one word.
- That same constructor set `stale: true`, and `NodeApiService::identity`
  re-checks a stale verdict in the background. So the first read of any
  stranger's identity sent a DNS query for their handle. Reproduced against a
  live pair: a witness that had never run a check answered `unverified`, then
  resolved `_mabel.waddles.mabel.reamde.dev` unprompted and cached
  `mismatched`. Decision 018 keeps exposure explicit; this told a stranger's
  zone that somebody here was reading their card.

The reported nulls themselves reproduce on neither storage path. The mechanism
that produces them against a deployed node is issue 043: with no
`Cache-Control` on any API response, an intermediary may serve a heuristically
cached `/api/identities/{id}` from before the profile update landed.

## Scope

- `VerificationStatus` gains `Unchecked`, wire spelling `unchecked`: a handle
  this node has never looked up. `Verification::unchecked` uses it and sets
  `stale: false`, since a verdict that does not exist cannot be out of date.
- `Names::status` answers `Unchecked` rather than `Unverified` when the cache
  holds no entry bound to the claimed handle.
- `NodeApiService::identity` starts a background re-check only for a verdict
  that exists and has gone stale. `POST
  /api/identities/{id}/verification` stays the one thing that runs a check.
- `contracts/README.md` documents six statuses and which two mean no lookup
  ran. `contracts/http/wallet-get-known-identities.json` gains a third row so
  absence and failure are both pinned; `contracts/cli/dev-seed.json` follows.
- UI: `VerificationStatus` gains `unchecked` with a neutral marker beside the
  handle, and the handle screen says it has not been checked from this wallet
  yet, above the existing check control.
- Story 007 and its Playwright spec.

## Acceptance criteria

- [ ] A ledger stored over the sync push path and never crawled reads its
      display name, handle, email and endpoints from `GET
      /api/identities/{id}` and from its `known` row.
- [ ] A verdict this node cached reaches both the identity document and the
      `known` row.
- [ ] `GET /api/identities/{id}` on a handle with no cached verdict answers
      `unchecked` and sends no DNS query.
- [ ] A list row tells a handle nobody checked from a handle whose records
      name nobody, on the status string alone.
- [ ] tests: `cargo fmt`, `clippy`, the workspace suite, the UI suite, the UI
      build and lint, and the full Playwright suite are green.

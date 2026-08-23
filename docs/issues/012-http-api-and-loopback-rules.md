# 012: axum HTTP APIs, loopback rules and UI serving

- Status: open
- Depends on: 010, 011

## Goal

Both node roles serve their JSON API on loopback with the three hardening rules
of proposal 001 section 10, plus the static UI assets, so the browser app has a
backend to call.

## Scope

- Wallet API under `/api`, exactly the routes section 10 lists: `GET /node`;
  `GET|POST /identities`; `GET /identities/:id` and
  `/identities/:id/ledger?since=`; `POST /identities/:id/witnesses`; `POST
  /trust` and `/trust/:event_id/revoke`; `POST /orgs`, `/orgs/:id/invites`,
  `/orgs/:id/acceptances`, `/orgs/:id/removals`; `POST /sync/push`; `POST
  /verify`.
- Witness API, read-only: `GET /node`, `/ledgers`, `/ledgers/:id`,
  `/ledgers/:id/events?since=`, `/forks` (section 10).
- Every `?since=` parameter is inclusive, matching `Get.since` (ticket 009).
- One axum middleware layer with the three rules of section 10: `Host` must be
  `127.0.0.1` or `localhost` with the expected port; a mutating request's
  `Origin` must match that host; mutating routes require `content-type:
  application/json`.
- Bind `127.0.0.1` by default in both roles and warn at startup when bound
  elsewhere; no authentication (section 10).
- Static assets embedded with `rust-embed`, served from disk with `--ui-dir`
  (section 10).
- `wallet serve [--http <addr>]` and the `--http` surface of `witness run`.
- Verification responses use the same "as of seq N from source S" struct as the
  CLI, carrying `source`, `head_seq`, `head_event`, `fetched_at` (sections 6
  and 10).

## Acceptance criteria

- [ ] The route lists match section 10 exactly: orgs appear in `GET
      /identities` and there is no `GET /orgs`; the witness API is read-only.
- [ ] Both roles default to `127.0.0.1` and print a warning when bound
      elsewhere.
- [ ] `--ui-dir` serves the bundle from disk; without it the embedded bundle is
      served.
- [ ] All logic stays in the node: the API returns rendered results and the UI
      does no crypto (section 10).
- [ ] tests: the middleware rejects `Host: evil.example`, the right host on the
      wrong port, `localhost.example`, an absent and a mismatched `Origin` on a
      mutating route, and a non-JSON content type, and accepts the correct
      versions; a table test asserts every mutating route is covered.
- [ ] tests: one happy-path test per wallet route group and per witness route
      against an in-process server, including `?since=` at the head sequence.

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
- One axum middleware layer with the three rules: reject a request whose `Host`
  is not `127.0.0.1` or `localhost` with the expected port; reject a mutating
  request whose `Origin` does not match that host; require `content-type:
  application/json` on mutating routes (section 10).
- Bind `127.0.0.1` by default in both roles and print a warning when bound
  elsewhere; no authentication (section 10).
- Static assets embedded with `rust-embed` and served from disk with `--ui-dir`
  in development (section 10).
- `wallet serve [--http <addr>]` and the `--http` surface of `witness run`
  (section 9).
- Verification responses use the same "as of seq N from source S" struct as the
  CLI, carrying `source`, `head_seq`, `head_event`, `fetched_at` (sections 6
  and 10).

## Acceptance criteria

- [ ] The route lists match section 10 exactly, and the witness API is
      read-only.
- [ ] All logic stays in the node: the API returns rendered results and the UI
      does no crypto (section 10).
- [ ] tests: API tests assert a bad `Host`, a mismatched `Origin` on a mutating
      route and a missing content type are each rejected, and that the same
      requests succeed with correct headers (section 11, API tests bullet).
- [ ] tests: one happy-path test per wallet route group and per witness route
      against an in-process server.

# 012: axum HTTP APIs over the contract fixtures, loopback rules and UI serving

- Status: open
- Depends on: 020

## Goal

Both node roles serve every route in `contracts/http/` on loopback with the
three hardening rules of proposal 001 section 10, plus the static UI assets.
Handlers call service traits and a stub implementation answers from the
fixtures, so the UI has a real backend before the runtimes exist. Being
implemented concurrently with tickets 010, 011 and 013.

## Scope

- Service traits in `crates/mabel-node/src/api/`, one per surface: node info,
  wallet identity and trust, sync, verify, and the witness read side. The traits
  take and return the document types of the fixtures; handlers hold no node
  state and touch no storage directly. Tickets 010 and 011 implement them.
- A stub implementation behind a flag, answering each route with its fixture
  document, so ticket 013 can be built and tested against a running node.
- Wallet API under `/api`, exactly the routes `contracts/README.md` indexes:
  `GET /node`; `GET|POST /identities`; `GET /identities/:identity_id` and
  `/identities/:identity_id/ledger?since=`; `POST
  /identities/:identity_id/witnesses`; `POST /trust` and
  `/trust/:event_id/revoke`; `POST /sync/push`; `POST /verify`. The `/orgs`
  routes of proposal 001 section 10 are deleted (proposal 002 section 6).
- Membership routes (`/identities/:identity_id/memberships/invitations`,
  `/acceptances`, `/removals`) answer 501 with code 70 until ticket 021
  freezes and implements them.
- Witness API, read-only: `GET /node`, `/ledgers`, `/ledgers/:ledger_id`,
  `/ledgers/:ledger_id/events?since=`, `/forks`. `LedgerSummary.kind` renders
  as `declared_kind` (proposal 002 section 10).
- Every `?since=` is inclusive; `offset`, `limit` and `more` are echoed back.
- One axum middleware layer with the three rules of section 10: `Host` must be
  `127.0.0.1` or `localhost` with the expected port; a mutating request's
  `Origin` must match that host; mutating routes require `content-type:
  application/json`. All three reject with code 2 and no layer prefix.
- Errors use the shared envelope with the CLI exit code as `code` and the HTTP
  status of the table in `contracts/README.md`.
- Bind `127.0.0.1` by default in both roles and warn at startup when bound
  elsewhere; no authentication. Static assets embedded with `rust-embed`,
  served from disk with `--ui-dir`. `wallet serve [--http <addr>]` and the
  `--http` surface of `witness run`.

## Acceptance criteria

- [ ] Every `contracts/http/*.json` route exists, and its success and error
      bodies match the fixture key for key.
- [ ] No handler reads storage or signs; the traits are the only path to node
      state (section 10).
- [ ] The membership routes answer 501 with `code: 70`.
- [ ] Both roles default to `127.0.0.1` and warn when bound elsewhere;
      `--ui-dir` serves the bundle from disk, otherwise the embed serves it.
- [ ] tests: the middleware rejects `Host: evil.example`, the right host on the
      wrong port, `localhost.example`, an absent and a mismatched `Origin` on a
      mutating route, and a non-JSON content type; a table test asserts every
      mutating route is covered.
- [ ] tests: one test per fixture asserts the stub response equals the fixture,
      including `?since=` at the head sequence.

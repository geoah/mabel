# 009: `mabel-net` client, server and protocol caps

- Status: done
- Depends on: 003

## Goal

`mabel-net` speaks the ALPN `mabel/ledger/0` protocol of proposal 001 section 5
in both directions: a client that issues the five requests and a
`ProtocolHandler` server that answers them over a store trait, with every cap
and byte budget enforced.

## Scope

- ALPN `mabel/ledger/0`, one request per bidirectional stream, the server
  looping on `accept_bi`; the client writes the encoded request, calls
  `send.finish()` and reads to EOF under a hard byte cap, and the server mirrors
  that, with no length prefix (section 5).
- Request and response encode and decode for `Head`, `Get`, `Push`, `List`,
  `Forks` and the seven response variants, with descriptors for both registered
  with the ticket 003 validator and run on every received frame.
- `Get.since` is inclusive: a `Get` at `since = head_seq` returns that event.
- Caps: 4 MiB frames, 4 KiB single event, `Push` at most 512 events and 2 MiB,
  `Get.limit` clamped to 512, `List.limit` to 256, `Forks.limit` to 64, all
  checked before allocation (section 5, pitfall 7).
- Byte budgets: a response fills to `min(count limit, byte budget)` and sets
  `more`; `List` orders by ascending ledger id (section 5).
- Concurrency: 32 connections, 64 requests per connection, 8 concurrent
  verifications behind a semaphore, answering `RejectedResp { BUSY }` rather
  than queueing without bound (section 5).
- A store trait the server is generic over, so the witness (ticket 010) and a
  wallet serving reads plug in; address lookup with `MemoryLookup` and
  `iroh-tickets` for `--peer` (sections 5 and 9).
- Endpoint construction honouring `node.json.relay` (`n0` or `disabled`) and
  iroh default features in application crates (sections 4 and 12).

Out of scope: admission, push storage semantics and fork recording (ticket 010).

## Acceptance criteria

- [ ] Transport identity is never used for authorization; `remote_id()` is
      passed to the store as provenance only (section 4).
- [ ] Every `RejectCode` of section 5 round-trips through encode and decode.
- [ ] `mabel-net` itself returns `MALFORMED`, `TOO_LARGE`, `UNSUPPORTED` and
      `BUSY`, with an unrecognised `Request` variant answering `UNSUPPORTED`;
      `INVALID`, `FORK` and `NOT_ADMITTED` come from the store and are tested
      in ticket 010.
- [ ] tests: two in-process endpoints with `presets::Minimal` and
      `RelayMode::Disabled` dialling the loopback `EndpointAddr` exercise every
      request type, oversize, truncated and garbage input (section 11).
- [ ] tests: boundary cases for the frame cap, the single-event cap, the push
      count and byte caps, limit clamping for `Get`, `List` and `Forks`, the
      byte budget setting `more`, and `Get` at `since = head_seq`.

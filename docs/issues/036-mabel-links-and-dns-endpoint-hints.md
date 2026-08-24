# 036: `mabel://` links and `mabel-endpoints=` DNS hints

- Status: open
- Depends on: 033

## Goal

An identity is shareable as one string and reachable from its zone:
`mabel://<identity id>[?endpoints=...]` parsed in core with vectors, a second
recognised TXT key beside `mabel=`, `GET /api/resolve?input=` taking all three
input kinds, and `mabel identity share` printing the link, a QR square and a
file (proposal 006 sections 6 and 7).

## Scope

- A new module in `mabel-core` owning the grammar of section 7: one identity id
  as the authority, an empty or single-slash path, at most one `endpoints` key
  with 1 to 4 ids, no fragment, no port, no userinfo, no percent-encoding, no
  whitespace. Refused whole with code 2 and reason `invalid_mabel_link`, never
  trimmed. Parsing is case-insensitive, rendering is lowercase.
- `GET /api/resolve/:hostname` becomes `GET /api/resolve?input=<value>`,
  answering `{ok, input_kind, identity_id, hostname, endpoints, status}` with
  `input_kind` in `identity | hostname | link`. The HTTP layer percent-decodes
  `input` exactly once and the core parser refuses percent-encoding, so `%252f`
  is refused rather than decoded twice. A repeated or unknown query key is
  `unknown_query_parameter`. The route writes nothing, touches no verification
  cache, and `ResolveStatus` gains no value.
- `crates/mabel-node/src/verification/verify.rs`: the `mabel-endpoints=` prefix
  compared case-insensitively, comma splitting with no whitespace, the point
  check, and the one overflow rule, discard whole, at both the record and the
  label level. Surviving endpoints across records at one label are unioned and
  sorted ascending by rendered base32. `MAX_CNAME_LINKS` is unchanged.
- The applicability matrix of section 6: a caller-supplied hostname may yield
  endpoints for the identity that response resolved to; a hostname taken from a
  ledger's own claim, a stale local copy or a stored crawl generation yields
  endpoints only when the same response also carries `mabel=<that identity>`.
  Nothing in this ticket touches the five verification statuses or
  `verification/<identity_id>.json`.
- `mabel identity share <alias|id> [--endpoints auto|<endpoint,...>] [--out
  <file>] [--qr]`, with `auto` taking the identity's advertised endpoints or this
  node's endpoint id when the home can sign and the chain advertises nothing.
  The `.mabel` file holds one line, UTF-8, trailing newline, no BOM. Pin the two
  QR crates, a CLI encoder and an SVG encoder for the UI, against the registry
  on the day.
- Every CLI operand that takes `<alias|id>` also takes a link, under the matrix
  of section 7: hints used on the fetched subject, ignored with a warning naming
  the flag on a local signer.

## Acceptance criteria

- [ ] Every refusal rule of section 7 is refused whole with
      `invalid_mabel_link` and `details.input` holding the string as given,
      including a link with three good endpoints and one bad one.
- [ ] A DNS record with 9 endpoints, a duplicate, an empty element or an
      unparseable element is discarded whole; a label whose surviving records
      name more than 8 distinct endpoints reads as absent.
- [ ] A zone with an endpoints record and no `mabel=` record is still
      `unverified`.
- [ ] tests: golden link vectors in `mabel-core` for parse and render, both
      directions, plus the `%252f` case through the HTTP layer.
- [ ] tests: the applicability matrix, both rows, including a zone naming other
      endpoints and not the identity being resolved.
- [ ] tests: two records at one label produce the same sorted set whatever order
      the resolver returns them in, including an id split across two
      character-strings.
- [ ] tests: `mabel identity share` round-trips through `mabel sync fetch
      <link>`; a link on `--identity` warns and is ignored.
- [ ] tests: push path unbroken. This ticket changes no admission rule, so
      `crates/mabel-cli/tests/sync.rs` passes unmodified, with `cargo fmt`,
      `clippy` and the workspace suite.

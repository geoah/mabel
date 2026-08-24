# 033: `WitnessSet` and `EndpointAdvertisement` payloads

- Status: open
- Depends on: none

## Goal

A ledger names its witnesses by identity id and publishes the endpoints that
answer for it: payload tag 19 `WitnessSet` and tag 18 `EndpointAdvertisement`,
validated, folded, built, written by the CLI and the routes, and pushable,
because `witness_for` and the tag-19 admission clause land in the same ticket
(proposal 006 sections 1, 2, 3).

## Scope

- `proto/mabel/v0/ledger.proto`: the two messages, `EventBody.payload` variants
  18 and 19, a comment on `WitnessConfig` saying it is readable and never
  written, and the header note that tags 10 to 19 are spent.
- `crates/mabel-core/src/validate.rs`: `WITNESS_SET` and
  `ENDPOINT_ADVERTISEMENT` descriptors and the two field-table rows of section
  3, `MAX_ENDPOINTS` of 8, `MAX_WITNESSES` reused for tag 19, no new
  `WireError` codes. `WITNESS_CONFIG` is unchanged.
- `crates/mabel-core/src/fold.rs`: `state.witnesses()` is removed for the three
  accessors of section 3, `witness_identities()`, `witness_endpoints()` and
  `endpoints()`, the first and third carrying the event, seq and signing
  principal, folded independently of each other in any event order.
- `crates/mabel-core/src/sign.rs`: `build_witness_set` and
  `build_endpoint_advertisement`; `build_witness_config` moves behind a
  test-only gate so the tag-11 vectors keep their exact bytes.
- `crates/mabel-node/src/config.rs`: `witness_for: Vec<IdentityId>`, at most 16,
  no duplicate, empty by default, no local key required (section 4).
- `crates/mabel-node/src/witness/storage.rs`: admit a push when this home holds
  a signing key for the ledger or when the stored or pushed state's
  `witness_identities()` intersects `witness_for`; otherwise `NOT_ADMITTED`.
  The rest of section 4 is ticket 034.
- `crates/mabel-node/src/api/parse.rs::witnesses` takes identity ids and
  `malformed_endpoint_id` becomes `malformed_identity_id`; `POST
  /api/identities/:identity_id/witnesses` writes tag 19; `POST
  /api/identities/:identity_id/endpoints` is new and refuses a no-op the way the
  profile route does; `api/documents.rs` gains the two `payload_kind` values.
- `mabel witness add`, `mabel identity endpoints replace --endpoints
  auto|<endpoint,...>`, and `dev seed` creating a witness identity, advertising
  the seeding node's endpoint on it and naming it in each seeded `WitnessSet`.
- Not here: `docker/entrypoint.sh` and `docker/smoke.sh` still name a witness by
  endpoint id and move to identity ids in ticket 040, so the push path is proved
  by the cargo suites until then.

## Acceptance criteria

- [ ] Both lists may be empty, both refuse duplicates, and a ledger may name
      itself in its own `WitnessSet` (sections 1 and 3).
- [ ] No existing golden or rejection vector's bytes change.
- [ ] `build_witness_config` is unreachable from any route, command or UI
      action; `cargo build` without test features does not compile a caller.
- [ ] tests: golden vectors for a witness set, an empty witness set, an
      advertisement and an empty advertisement.
- [ ] tests: rejection vectors for 17 witnesses, 9 endpoints, a duplicate in
      either list, a wrong-length entry, and an endpoint that is not a valid
      ed25519 point.
- [ ] tests: the fold reports the tag-11 and tag-19 fields independently in
      either event order and records `signing_principal` on tags 18 and 19.
- [ ] tests: push path unbroken. `crates/mabel-node/tests/witness.rs` pushes a
      ledger whose `WitnessSet` names a witness identity to a home whose
      `witness_for` names it, and a home with an empty `witness_for` answers
      `NOT_ADMITTED`. `crates/mabel-cli/tests/sync.rs`, `cargo fmt`, `clippy`
      and the workspace suite pass.

# 002: canonical encoding, digests and the golden vector harness

- Status: done
- Depends on: 001

## Goal

`mabel-core` can produce the one canonical encoding of every signed message,
compute the four domain-separated digests, render and parse identity ids, and
the repository has a golden vector harness that pins those bytes.

## Scope

- `mabel-core` canonical encoder emitting the form defined in proposal 001
  section 3.1: ascending field numbers, minimal varints, no proto3 default
  serialized, one occurrence per non-repeated field, no packed repeated fields.
- The normative canonical-encoding prose written next to the schemas, in
  `proto/mabel/v0/README.md`, matching section 3.1 word for word in content.
- Digest and signing-input functions for `event_id`, `sign_input`,
  `accept_input` and `reserve_commit` exactly as section 3.1 spells them,
  BLAKE3-256 with the given domain separators.
- Id codec: lowercase RFC 4648 base32, no padding, 52 characters, no type
  prefix (section 3.1).
- Signing path: the only function that produces event bytes, returning the
  encoded `EventBody` bytes and a `SignedEvent`; nothing else may re-encode an
  event (byte authority, section 3.1 and pitfall 1). It sets `timestamp_ms =
  max(now_ms, prev.timestamp_ms)` and enforces the `4102444800000` ceiling
  (section 3.2).
- `test-vectors/`: checked-in literal bytes, ids and signatures, one vector per
  payload variant, produced under fixed keys, nonces and timestamps, each
  carrying encoded body hex, `event_id`, the signature and a JSON rendering
  (section 11, golden vectors bullet). Regeneration is a separate command run
  by a human for review; tests never regenerate.

## Acceptance criteria

- [x] The encoder reproduces the canonical form of section 3.1 for every
      message type in sections 3.2, 3.4 and 3.5.
- [x] The four digest and input formulas match section 3.1 byte for byte,
      including the trailing newline in each domain separator.
- [x] Ids render as 52 lowercase base32 characters and round-trip to 32 bytes.
- [x] `test-vectors/` covers every payload variant in sections 3.2, 3.4 and
      3.5, and no test writes to it.
- [x] tests: the golden test compares the encoder's output against the
      checked-in bytes, `event_id` and signature; mutating any single byte of a
      vector body makes signature verification fail.
- [x] tests: appending with a clock behind `prev.timestamp_ms` produces
      `prev`'s timestamp and a valid event, not a rejection (section 3.2).
- [x] tests: the base32 round trip and each digest formula have a unit test.

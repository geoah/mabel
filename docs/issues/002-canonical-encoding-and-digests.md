# 002: canonical encoding, digests and the golden vector harness

- Status: open
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
  event (byte authority, section 3.1 and pitfall 1).
- `test-vectors/` harness: a generator plus checked-in vectors, one per event
  type, each carrying encoded body hex, `event_id`, the signature under a fixed
  test key and a JSON rendering (section 11, golden vectors bullet).

## Acceptance criteria

- [ ] The encoder reproduces the canonical form of section 3.1 for every
      message type in sections 3.2, 3.4 and 3.5.
- [ ] The four digest and input formulas match section 3.1 byte for byte,
      including the trailing newline in each domain separator.
- [ ] Ids render as 52 lowercase base32 characters and round-trip to 32 bytes.
- [ ] `test-vectors/` holds one golden vector per event type with the four
      fields listed in section 11.
- [ ] tests: a golden test asserts the encoder reproduces every vector's bytes,
      `event_id` and signature; flipping one byte of a vector body makes
      signature verification fail; the base32 round trip and each digest
      formula have a unit test.

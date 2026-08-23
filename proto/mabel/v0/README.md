# mabel v0 wire format

The `.proto` files in this directory are normative, and so is this page. An
implementation that follows the schemas but not the rules below produces
events other implementations reject (proposal 001 section 3.1).

## Byte authority

A signer serializes a message once. Those exact bytes are hashed, signed,
stored and shipped. A verifier hashes and checks the bytes it received and
decodes only to read fields. Re-serializing a decoded message invalidates its
signature and changes its id, so only the signing path produces event bytes.

## Canonical encoding

Every signed or hashed message uses this form:

- fields in ascending field-number order;
- minimal varints;
- no field set to its proto3 default value is serialized;
- each non-repeated field appears exactly once, and every field the field
  table (proposal 001 section 3.4) marks required is present;
- no packed repeated fields; every repeated field in a signed message holds
  `bytes` or message elements, which are length-delimited per entry.

Received bytes are checked against these rules directly, before decoding: a
verifier rejects unknown field numbers, duplicate non-repeated fields,
out-of-order fields, non-minimal varints, wrong wire types, unrecognised
`oneof` variants and `*_UNSPECIFIED` enum values.

## Digests

BLAKE3-256 with a domain separator, each ending in a newline:

```text
event_id       = BLAKE3(b"mabel/event/v0\n"   || event_body_bytes)
sign_input     =        b"mabel/sig/v0\n"     || event_body_bytes
accept_input   =        b"mabel/accept/v0\n"  || acceptance_bytes
reserve_commit = BLAKE3(b"mabel/reserve/v0\n" || reserve_public_key)
```

`sign_input` and `accept_input` are signed with ed25519. An identity id, a
ledger id and an event id are 32 bytes and display as lowercase RFC 4648
base32 without padding, 52 characters, with no type prefix.

## Versioning

Field numbers and `oneof` tags are append-only. A breaking change means a
`v1` directory, a new envelope `version` and a new ALPN.

## Conformance

`test-vectors/` at the repository root holds one vector per payload variant:
the encoded body, the encoded `SignedEvent`, the event id and the signature,
under fixed keys, nonces and timestamps. A second implementation is correct
when it reproduces those bytes.

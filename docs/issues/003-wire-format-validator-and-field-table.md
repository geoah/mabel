# 003: wire-format validator, field table and rejection vectors

- Status: open
- Depends on: 002

## Goal

Every byte string mabel accepts from a peer, a file or disk passes one
stateless gate before any semantic rule runs: the byte-scanning wire-format
validator, then the stateless rows of the field table in proposal 001
section 3.4.

## Scope

- `mabel-core` wire-format validator that scans received bytes directly, before
  and independently of prost decoding, rejecting the seven classes listed in
  section 3.1: unknown field numbers, duplicate non-repeated fields,
  out-of-order fields, non-minimal varints, wrong wire types, unrecognised
  `oneof` variants, `*_UNSPECIFIED` enum values.
- The validator is descriptor-driven: each message type registers a descriptor
  naming its fields, wire types, cardinality and caps, so the sync frames of
  section 5 and the file artifacts of section 3.8 register their own.
- Stateless field-table rows of section 3.4: presence, exact byte length,
  uniqueness, enum agreement and intra-message cross-field rules.
- `verify_inception_standalone`: the check section 3.4 requires of an embedded
  `founder_inception` or `invitee_inception` (recomputed `event_id`, canonical
  form, self-consistency, `kind == PERSON`, valid self-signature).
- Caps checked before allocation: encoded `SignedEvent` 4096 bytes, `sig`
  exactly 64, `Acceptance` blob <= 1024, embedded inception <= 1024
  (section 3.4 table as clarified, pitfall 7).

Out of scope, because they need folded state: `author_key` authorization, the
`ledger`/`prev`/`seq` chain equalities and `TrustRevocation.target` liveness
(ticket 004), and `OrgRemoval.target` validity (ticket 005).

## Acceptance criteria

- [ ] The validator's entry points take `&[u8]` and return before any prost
      decode, so no code path decodes first (section 3.1).
- [ ] `encoded_len() == len` appears only as a debug assertion, if at all, and
      is not the gate (section 3.1).
- [ ] Every stateless row of the section 3.4 table is enforced with the exact
      byte lengths that table states.
- [ ] `verify_inception_standalone` enforces all four conditions section 3.4
      lists and rejects an inception whose digest does not equal the recorded
      id and one whose `active_key` differs from the recorded key.
- [ ] tests: one negative unit test per validator class and per stateless
      field-table row (section 11, core unit tests bullet); every rejection
      vector in `test-vectors/` is rejected with the expected reason; a valid
      golden vector from ticket 002 passes.

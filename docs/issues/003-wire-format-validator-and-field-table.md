# 003: wire-format validator, field table and rejection vectors

- Status: open
- Depends on: 002

## Goal

Every byte string mabel accepts from a peer, a file or disk passes one
stateless gate before any semantic rule runs: the byte-scanning wire-format
validator, then the field table of proposal 001 section 3.4.

## Scope

- `mabel-core` wire-format validator that scans received bytes directly, before
  and independently of prost decoding, rejecting the seven classes listed in
  section 3.1: unknown field numbers, duplicate non-repeated fields,
  out-of-order fields, non-minimal varints, wrong wire types, unrecognised
  `oneof` variants, `*_UNSPECIFIED` enum values.
- Field-table validation for every row of the table in section 3.4, including
  exact byte lengths, presence, uniqueness and the cross-field rules.
- `verify_inception_standalone`: the check section 3.4 requires of an embedded
  `founder_inception` or `invitee_inception` (recomputed `event_id`, canonical
  form, self-consistency, `kind == PERSON`, valid self-signature).
- Size caps checked before allocation: `SignedEvent.body` 4096, `sig` 64,
  `Acceptance` blob 1024 (section 3.4 table, pitfall 7).
- Rejection vectors in `test-vectors/`, one per validator rule and one per
  field-table rule (section 11, golden vectors bullet).

Out of scope: chain, authority and policy rules, which belong to the fold
(tickets 004 and 005).

## Acceptance criteria

- [ ] The validator's entry points take `&[u8]` and return before any prost
      decode, so no code path decodes first (section 3.1).
- [ ] `encoded_len() == len` appears only as a debug assertion, if at all, and
      is not the gate (section 3.1).
- [ ] Every row of the section 3.4 table is enforced, with the byte lengths
      exact as that table states.
- [ ] `verify_inception_standalone` enforces all four conditions section 3.4
      lists and rejects an inception whose digest does not equal the recorded
      id and one whose `active_key` differs from the recorded key.
- [ ] tests: one negative unit test per validator class and per field-table
      row (section 11, core unit tests bullet); every rejection vector in
      `test-vectors/` is rejected with the expected reason; a valid golden
      vector from ticket 002 passes.

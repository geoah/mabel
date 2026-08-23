# 006: file artifacts, caps and fork-record validation

- Status: open
- Depends on: 005

## Goal

`mabel-core` parses the three file artifacts of proposal 001 section 3.8 under
their size caps and exposes the shared fork-record validation function that
section 5 requires of the witness and of any reader.

## Scope

- `InviteBundle`, `AcceptanceFile` and `IdentityDescriptor` parsing, each
  running the wire-format validator and the field table and enforcing its cap
  before any allocation sized by the input: 1 MiB, 4 KiB, 64 KiB (section 3.8,
  pitfall 7).
- `InviteBundle` verification: fold `org_prefix` from inception, locate the
  named invite, and return the summary `org accept` displays (the org, its
  controllers and the offered role) without signing anything (section 3.8).
- `IdentityDescriptor` build and parse: the inception `SignedEvent` plus the
  witness endpoint list, the artifact `identity export` writes (section 3.8).
- `validate_fork_record`: a conflicting event is valid only if it fully
  verifies against the shared prefix (canonical form, field table, sequence,
  ledger id, authorized signer at that position, valid signature); anything
  else is invalid and must not be stored (section 5, fork records).

## Acceptance criteria

- [ ] Each artifact's cap is checked before allocation and an oversize input is
      rejected without reading the remainder (section 3.8).
- [ ] Artifacts go through the same validator and field table as network input
      (section 3.8).
- [ ] `validate_fork_record` takes the shared prefix plus both `SignedEvent`s
      and is the single implementation the witness and readers call (section
      5).
- [ ] tests: fork-record validation accepts a real conflict at a sequence,
      rejects a malformed conflicting event and rejects one signed by a key not
      authorized at that position (section 11, policy bullet).
- [ ] tests: an over-cap `InviteBundle`, `AcceptanceFile` and
      `IdentityDescriptor` are each rejected; a bundle whose prefix fails to
      fold reports the violation rather than the invite summary.

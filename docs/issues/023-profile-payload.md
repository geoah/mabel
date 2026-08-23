# 023: `ProfileUpdate` payload at tag 17

- Status: done
- Depends on: none

## Goal

A ledger carries a display name and a hostname: one `ProfileUpdate` payload at
tag 17, validated, folded latest-wins, and appendable by any current
`CONTROLLER`, per proposal 003 section 1.

## Scope

- `proto/mabel/v0/ledger.proto`: `ProfileUpdate { display_name = 1, hostname =
  2 }` at payload tag 17. Tags 18 and 19 stay unassigned; `reserved 20 to 29`
  stands.
- Descriptor and field-table row in `crates/mabel-core/src/validate.rs`, with
  the caps of section 1 (64 bytes, 246 bytes).
- `FieldKind::String { max }` already exists and checks the byte cap and UTF-8,
  and `invalid_utf8` is already a `WireError`. This ticket adds the codepoint
  policy its doc comment defers to proposal 003, plus the `invalid_display_name`
  and `invalid_hostname` codes.
- Codepoint rules exactly as section 1 lists them, and hostname syntax exactly
  as section 2 lists it, both checked by the descriptor.
- Fold: `LedgerState.profile` holding `{display_name, hostname, event, seq,
  signing_principal}`, replaced whole by each `ProfileUpdate`.
- Builder plus the node-side `no_op_profile_update` guard, refused before
  signing.
- Golden and rejection vectors, including the zero-length payload body that
  clears both fields.

## Acceptance criteria

- [ ] The tag assignment, caps and clearing semantics match section 1; the
      hostname syntax check matches section 2.
- [ ] No existing golden vector's bytes change (section 6); the additions are
      new vectors.
- [ ] `no_op_profile_update` is a node guard only: the fold accepts a no-op
      `ProfileUpdate` that is present in a valid chain.
- [ ] tests: golden vectors for both fields set, one field set, and the
      zero-length body.
- [ ] tests: one rejection vector per codepoint rule in section 1, plus an
      over-cap `display_name`, an over-cap `hostname`, an explicitly encoded
      empty string (`DefaultValueEncoded`), and a `display_name` that parses as
      an identity id.
- [ ] tests: one rejection vector per hostname syntax rule in section 2
      (uppercase, trailing dot, no dot, over-long label, bad edge character).
- [ ] tests: the fold replaces the whole profile, records `signing_principal`,
      and accepts an append by a delegate `CONTROLLER`.
- [ ] tests: `cargo fmt`, `clippy` and the workspace suite pass.

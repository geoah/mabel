# Golden vectors

One file per event, covering every payload variant of `EventBody`. These are
the cross-language contract for the canonical encoding, the event ids and the
signatures (proposal 001 sections 3.1 and 11): a non-Rust client that emits
different bytes for the same inputs is wrong.

The files are literals. `cargo test -p mabel-core` reads them and compares;
no test writes them.

## Fields

| Field | Meaning |
|---|---|
| `inputs` | what the signing path was called with, including the test secret keys |
| `body_hex` | the encoded `EventBody`, the bytes that are hashed and signed |
| `signed_event_hex` | the encoded `SignedEvent` carrying `body_hex` verbatim |
| `event_id` | `BLAKE3("mabel/event/v0\n" \|\| body)` in lowercase base32 |
| `event_id_hex` | the same digest in hex |
| `signature_hex` | ed25519 over `"mabel/sig/v0\n" \|\| body` under the author key |
| `body` | a decoding of `body_hex` for human review, byte fields in hex |

`body_hex` and `signed_event_hex` are authoritative; every other field is
derived from them.

## Rejection vectors

`rejections/` holds one byte string per wire-format class (proposal 001
section 3.1) and per stateless field-table rule (section 3.4), each with the
rejection the validator must produce. A client that accepts any of them is
wrong.

| Field | Meaning |
|---|---|
| `class` | `wire-format` or `field-table` |
| `rule` | the proposal section and rule the vector pins |
| `entry` | the validator entry point that reads the bytes: `signed_event` or `acceptance` |
| `input_hex` | the bytes to feed that entry point |
| `code` | the stable snake-case name of the rejection class |
| `reason` | the message `mabel-core` returns, for human review |

`code` is the contract; `reason` is English and may be reworded. The
generator is an ignored test in `crates/mabel-core/tests/rejections.rs`:

```sh
cargo test -p mabel-core --features gen-vectors -- --ignored gen_rejections
```

## The scenario

Alice (secret key `0x11` repeated) creates a person ledger, configures two
witnesses, attests trust in Bob (secret key `0x22` repeated) and revokes it.
She then founds an org, invites Bob as a controller, admits him with the
acceptance he signed, and removes him. `09-embedded-person-inception.json` is
Bob's own inception, which the invite and the org events embed.

## Regenerating

The generator is an ignored test in `crates/mabel-core/tests/golden.rs`:

```sh
cargo test -p mabel-core --features gen-vectors -- --ignored gen_vectors
```

Run it only when a byte change is intended, and review the resulting diff:
it is the record of what changed for every other implementation.

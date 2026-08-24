# Golden vectors

One file per event, covering every payload variant of `EventBody` and both
variants of `Inception.root`. These are the cross-language contract for the
canonical encoding, the event ids and the signatures (proposal 001 sections
3.1 and 11, proposal 002 section 7): a non-Rust client that emits different
bytes for the same inputs is wrong.

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

`rejections/` holds one case per wire-format class (proposal 001 section 3.1),
per stateless field-table rule (proposal 002 section 8) and per membership
rule of the fold (proposal 002 section 4), each with the rejection an
implementation must produce. A client that accepts any of them is wrong.

| Field | Meaning |
|---|---|
| `class` | `wire-format`, `field-table` or `fold` |
| `rule` | the proposal section and rule the vector pins |
| `entry` | what reads the bytes: `signed_event`, `acceptance` or `fold` |
| `input_hex` | the bytes to feed that entry point, on the first two entries |
| `events_hex` | the whole chain to fold, on the `fold` entry |
| `at_seq` | the position the fold must reject, on the `fold` entry |
| `code` | the stable snake-case name of the rejection class |
| `reason` | the message `mabel-core` returns, for human review |

A `fold` vector carries a chain because its rule needs the state folded from
the events before the rejected one: an acceptance is only wrong given the
invitation it names, and a removal is only wrong given the principals it would
leave behind.

`code` is the contract; `reason` is English and may be reworded. The
generator is an ignored test in `crates/mabel-core/tests/rejections.rs`:

```sh
cargo test -p mabel-core --features gen-vectors -- --ignored gen_rejections
```

## Link vectors

`links.json` holds the `mabel://` grammar of proposal 006 section 7, which is
a pure function over a string and carries no event. `accepted` pins what one
input parses to and what that parse renders back as; `refused` pins one case
per refusal rule, every one refused whole.

| Field | Meaning |
|---|---|
| `accepted[].input` | the string handed to the parser |
| `accepted[].identity_id` | the identity it names |
| `accepted[].endpoints` | the endpoint hints, in the order the link names them |
| `accepted[].rendered` | the canonical form, always lowercase |
| `refused[].input` | the string handed to the parser, as given |
| `refused[].code` | `invalid_mabel_link`, the one refusal spelling |
| `refused[].reason` | the clause naming the rule, for human review |

`rendered` is authoritative for the render direction: an uppercased input and
an input with a trailing slash render as the same one string, and that string
parses back to the same link. The generator is an ignored test in
`crates/mabel-core/tests/links.rs`:

```sh
cargo test -p mabel-core --features gen-vectors -- --ignored gen_links
```

## The scenario

Alice (secret key `0x11` repeated) creates a raw-rooted ledger, configures two
witnesses, attests trust in Bob (secret key `0x22` repeated) and revokes it,
then invites Bob as a second controller of her own ledger and admits him:
vectors 10 and 11 are the delegation a raw root allows (proposal 002
section 4). Vectors 12 to 15 are the profile of proposal 003 section 1 and
proposal 005, replaced whole four times: a name and a hostname, then a display
name alone, which clears the hostname, then a zero-length payload body, which
clears everything, then all three fields at once, the public email included.
Vectors 16 to 19 are the two payloads of proposal 006, each replaced whole
twice: a witness set naming Bob and Alice herself, which is what a ledger that
keeps its own chain says, then an empty one, which says no witness keeps it;
then an advertisement naming the two endpoints vector 02 named as raw endpoint
ids, then an empty one, which says nothing answers for her right now. Vector 02
is the retired tag-11 `WitnessConfig`, which the fold accepts forever and no
node writes.
She also founds an organization, an identity-rooted ledger whose
inception embeds her own, invites Bob as a controller there, admits him with
the acceptance he signed, and removes him.
`09-embedded-raw-root-inception.json` is Bob's own inception, which vectors
06, 07, 10 and 11 embed. The rejection vectors add Carol (secret key `0x33`
repeated), who signs the transplanted acceptances.

## Regenerating

The generator is an ignored test in `crates/mabel-core/tests/golden.rs`:

```sh
cargo test -p mabel-core --features gen-vectors -- --ignored gen_vectors
```

Run it only when a byte change is intended, and review the resulting diff:
it is the record of what changed for every other implementation.

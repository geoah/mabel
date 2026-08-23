# The demo

`demo/run-demo.sh` runs the whole mabel story once, non-interactively, over the
compose topology of `docker/compose.yaml`: one witness and two wallets, alice
and bob, on one bridge network that needs no internet. It takes about 17
seconds, prints every command it runs and the output that command produced, and
exits 0 or says what broke.

```sh
demo/run-demo.sh          # up, the story, down -v
demo/run-demo.sh --keep   # leaves the topology and the homes running
```

It needs `docker`, `curl` and `jq` on the host and nothing else. The image is
built by `docker compose up` on the first run, which takes a few minutes; every
run after that reuses it.

Each wallet's home lives in its own container volume, so every command runs
through `docker compose exec` and the membership artifacts travel between the
two homes as files, by `docker cp` through a directory on the host. Two homes
that share no disk is the point: they still admit one member, with two
signatures.

## The eleven phases

1. **The topology comes up.** `docker compose up -d --wait`, then the three
   containers and the witness's endpoint id. A witness stores and serves
   ledgers and signs nothing of its own (decision 001, passive witnesses): it
   cannot admit anyone, attest to anyone or revoke anything.
2. **Alice and bob create person identities.** `identity create` prints the new
   id, which is the digest of the inception event that made it, so the id and
   the first key are one fact (proposal 001 section 3.3). The alias is a local
   label and is never signed.
3. **Both name the witness and push.** `witness add` appends an event, so who
   was asked to hold a copy is part of the record a verifier reads. `sync push`
   carries the ledger to that witness. The `--peer` ticket is an address hint,
   never authorization (section 4).
4. **Bob exports his descriptor.** The `IdentityDescriptor` carries bob's
   inception byte for byte, which is what proves his id and his key belong
   together (proposal 002 section 8). The host carries the file to alice.
5. **Alice founds a shared ledger.** `identity create --founder alice --kind
   organization` makes a ledger with an identity root: it holds no key of its
   own and its controllers sign for it (decision 002). It is the same ledger
   type a person gets, folded by the same rules (unified ledgers, decision
   003).
6. **Alice invites bob, bob accepts, alice admits.** Three commands, three
   files, two signatures. Nobody is added to a ledger without their own
   signature (membership by invitation, decision 004). `membership accept`
   prints the accept surface first: the ledger, its declared kind, the role
   offered and the current controllers, folded from the bundle rather than read
   off it. `membership list` then shows two controllers and the invitation
   marked accepted.
7. **Alice and the shared ledger both attest trust in bob.** Trust is one-way
   and it is a statement, not a permission (decision 005): the attestation
   names a subject and grants nothing, and the CLI says so on every line it
   prints. `verify trust --issuer mabel-demo-co` shows the delegation: the
   report names alice as the principal that signed, so a delegate's signature
   is not read as the ledger's own (proposal 002 section 5).
8. **Push what the witness will take.** Alice's ledger goes up with the
   attestation. The shared ledger first names the witness on its own chain,
   because a witness only admits a ledger whose witness config names it
   (admission, proposal 001 section 5), and then its push is accepted too.
9. **A stranger verifies from an empty home.** A throwaway container with no
   identities, no aliases and no keys but the node key it makes on the spot
   reads alice's ledger from the witness and reports `trusted: true`. The
   report names its source and how far it read, and claims nothing about a
   chain it did not read (flag R, proposal 001 section 6).
10. **Alice revokes, and the stranger reads the revocation.** `trust revoke`
    appends to alice's ledger, one more push carries it, and a second fresh
    container reports `trusted: false` with the revoking sequence number.
11. **What the witness holds.** `GET /api/ledgers` on the witness's read-only
    debug API, from the host on `127.0.0.1:9080`, lists each ledger with its
    head sequence and head event.

## A gap this demo found, now fixed

An earlier build rejected every identity-rooted ledger at push time with
`MALFORMED`, "a message nests more than 8 levels deep": a pushed event was
scanned inside `Frame` -> `PushReq` -> `SignedEvent`, two levels below where
the same bytes are scanned locally, so the founder inception an identity root
embeds landed past `MAX_NESTING`. Embedded events now carry their own nesting
budget (`FieldKind::Detached`), and phase 8 asserts the push is accepted.

## Reading the output

A line starting with `[alice] $ mabel ...` is the command, run inside that
container. The indented lines under it are that command's output, verbatim. A
line starting with two spaces and no `$` is narration from this script, not
from mabel.

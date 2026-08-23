# 007: profile and verification

- Status: implemented
- Surfaces: wallet UI (alice and bob), CLI, wallet HTTP API
- Test: `tests/e2e/specs/007-profile-and-verification.spec.ts`

Alice gives her ledger a display name and a hostname, a TXT record backs the
hostname, and the wallet shows the verification state. Alice then looks bob's
contact up and sees how she knows him.

This is the one story that runs against `docker/compose.dns.yaml`, the test
resolver overlay of ticket 032. The spec brings the topology down and up again
with that overlay in its step 1, so the suite's global setup stays base-only
and stories 001 to 006 keep running on their own.

## Actors

- alice: wallet node, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`.
- bob: wallet node, compose service `bob`, API and UI on
  `http://127.0.0.1:9082`.
- carol: a third identity in bob's home, trusted by bob and unknown to alice
  except through the crawl.
- witness: compose service `witness`, the only place alice can read bob's and
  carol's ledgers from.
- resolver: compose service `resolver` in `docker/compose.dns.yaml`, serving
  TXT records for `example` names to the wallets at `172.29.0.53` and
  refusing every other name. Nothing reaches the public internet.

`dc` stands for `docker compose -f docker/compose.yaml -f
docker/compose.dns.yaml`, run from the repository root.

## Story

1. Run story 001 steps 1 to 12 against the overlay, then create carol in bob's
   home, name the witness on her ledger and push it, and have bob attest her:
   ```sh
   dc exec -T bob mabel identity create --alias carol --kind person
   dc exec -T bob mabel witness add --identity carol --endpoint "$witness_id"
   dc exec -T bob sh -c 'mabel sync push --identity carol \
     --peer "$(cat /shared/witness.ticket)"'
   dc exec -T bob mabel trust add --issuer bob --subject "$carol_id"
   dc exec -T bob sh -c 'mabel sync push --identity bob \
     --peer "$(cat /shared/witness.ticket)"'
   ```
   The witness add is not optional: a witness refuses a ledger whose chain does
   not name it, so without it carol's push answers `NOT_ADMITTED` and the crawl
   has nothing to read. The trust chain is alice trusts bob, bob trusts carol.
2. Wire the resolver and the crawl sources. Both come from the overlay, and
   the bring-up runs in two phases because a witness's endpoint id only exists
   once the witness has started:
   ```sh
   dc up -d --wait witness resolver
   witness_id="$(dc exec -T witness cat /shared/witness.id)"
   MABEL_WITNESSES="$witness_id" dc up -d --wait
   ```
   The overlay reads `MABEL_WITNESSES` from the environment for both wallets,
   the entrypoint runs `mabel witness set-default` with it, and that is the
   node-wide witness the crawler's third source asks (proposal 003 section 3).
   `GET /api/node` on alice answers `witnesses: ["<witness_id>"]`.
3. Publish the TXT records on the test resolver, by writing the zone file into
   the resolver's zone volume with the ids this run minted:
   - `_mabel.alice.example. IN TXT "mabel=<alice_id>"`
   - `_mabel.bob.example. IN TXT "mabel=<carol_id>"`, a record that names the
     wrong identity on purpose
   - nothing at `_mabel.nobody.example.`

   The SOA serial has to rise or CoreDNS keeps serving what it loaded, and the
   `_mabel.health` record has to stay or the container goes unhealthy. The
   `file` plugin rereads the zone within five seconds, so no restart is needed.
   Both TTLs are one second: a wallet's resolver caches for the TTL it is
   given, and a longer one would have a check taken seconds after the resolver
   stopped still answering from that cache.
4. Alice replaces her profile. The operation is replacement, not patch, so both
   fields are given and an omitted flag clears that field:
   ```sh
   dc exec -T alice mabel profile replace --identity alice \
     --display-name "Alice Example" --hostname alice.example --yes
   ```
   Without `--yes` the command prints the before-and-after diff and asks for
   confirmation.
5. Run the same command again, unchanged.
6. Force a verification and read the result:
   ```sh
   curl -fsS -X POST -H 'Origin: http://127.0.0.1:9081' \
     -H 'Content-Type: application/json' --data '{}' \
     "http://127.0.0.1:9081/api/identities/$alice_id/verification"
   curl -fsS "http://127.0.0.1:9081/api/identities/$alice_id"
   ```
7. Repeat steps 4 and 6 in bob's wallet with `--hostname bob.example`, whose
   TXT record names carol, and once more with `--hostname nobody.example`,
   which has no record.
8. Open alice's identity view in the wallet UI. The overview is one compact
   key-value table (`identity-detail`): name, copyable id, declared kind,
   alias, created, hostname with its verification mark, contact, and the
   counts. Read the `identity-detail-hostname` row for each of the three cases
   above and for carol, who claims no hostname. The mark sits inside that row
   as `identity-detail-hostname-verification`, and carol's row carries no
   mark at all.
9. Set a private contact note on bob, which is local and never signed:
   ```sh
   dc exec -T alice mabel contact set "$bob_id" --nickname "Bob from the pub" \
     --note "met at the meetup"
   ```
   The same store answers `GET` and `PUT
   /api/identities/<bob_id>/contact`, and it accepts foreign ids.
10. Synchronize the graph from alice's wallet UI, with `graph-sync-button` in
    the header, and look carol up:
    ```sh
    dc exec -T alice sh -c 'mabel graph sync --peer "$(cat /shared/witness.ticket)"'
    dc exec -T alice mabel lookup "$carol_id" --from alice
    curl -fsS "http://127.0.0.1:9081/api/lookup/$carol_id?from=$alice_id"
    ```
    The first graph sync shows the consent panel (`graph-sync-consent`),
    stating what becomes observable, and is remembered per node home. A CLI
    sync needs `--peer`: that process holds no seeded peer address, while the
    running wallet started with the witness's ticket.
11. Look up an identity nobody in the crawl trusts, for the empty answer:
    `dc exec -T alice mabel lookup "$witness_id" --from alice`.

## Verified outcomes

- Step 1: carol's push is accepted, and `GET
  http://127.0.0.1:9080/api/ledgers/<carol_id>` answers `entry.head_seq: 1`
  with `witnesses` naming the witness. Bob's ledger carries an unrevoked
  attestation for carol.
- Step 4 appends one `ProfileUpdate` (payload tag 17) to alice's ledger.
  `GET /api/identities/<alice_id>` answers `profile.display_name == "Alice
  Example"`, `profile.hostname == "alice.example"`, `profile.seq` equal to the
  new head, and `profile.signing_principal.identity == alice_id`.
- Step 5 exits 20 with `details.reason == "no_op_profile_update"` and appends
  nothing: an update whose effect equals the current folded profile is refused
  before signing.
- A profile replace that omits `--hostname` clears the hostname, and the
  cleared field is absent from the wire rather than encoded empty: the ledger
  event reports `payload.hostname: null`, and an empty string would have been
  refused as an encoded default before it was signed.
- A `display_name` that parses as a valid identity id is refused with
  `invalid_display_name`, and so is one carrying a bidi or zero-width control:
  a name can never masquerade as an identifier, and one class covers every
  unacceptable name. Each refusal exits 10 and names its reason in the
  message, "it parses as an identity id", "it holds a bidi control character",
  "it holds a zero-width or invisible format character".
- Step 6 answers `verification.status == "verified"`, `verification.hostname ==
  "alice.example"`, `verification.stale == false`, and both `checked_at_ms` and
  `last_verified_at_ms` set.
- Step 7: `bob.example` answers `verification.status == "mismatched"` (records
  exist under `mabel=` and none carries bob's id), and `nobody.example` answers
  `verification.status == "unverified"`. An identity with no hostname answers
  `verification.status == "unclaimed"`, and a forced check on it answers 409
  with `no_hostname_claimed`. With the resolver stopped, a forced check answers
  `unreachable` and does not overwrite a decisive result: the earlier
  `verified` entry keeps its `checked_at_ms` and the document reports both.
- Changing the hostname invalidates the old verdict: after a profile replace
  naming a different hostname, `verification.status` is `unverified` with
  `checked_at_ms: null` until a new check runs, because the cache entry is
  bound to the hostname it verified.
- `GET /api/identities` never triggers a DNS lookup: with the resolver stopped,
  listing every identity still answers from cache with the same
  `checked_at_ms` values.
- Step 8 renders each status distinctly, on
  `identity-detail-hostname-verification`, whose `data-verification` attribute
  is the state the row draws: a check with the hostname for a fresh
  `verified`, a check with a stale marker (`stale-verified`) for one older
  than 24 hours, a warning glyph for `mismatched`, dimmed text for
  `unverified` and `unreachable`, and, for `unclaimed`, no mark at all and the
  row reading "none claimed". `identity-detail-verification-note` carries the
  same advisory sentence the declared kind carries. Verification gates
  nothing: the ledger and every verification report read the same with and
  without it.
- The name never renders like an id: the display name is plain text
  (`identity-detail-resolved-name`), the id and the hostname are monospace
  with the copy control, the id is always beside the name
  (`identity-detail-identity-id`), and two entries resolving to one name both
  show their full ids. No screen sorts, matches or deduplicates on a name.
- Step 9 writes `contacts/<bob_id>.json` in alice's home only. Bob's wallet and
  the witness show no trace of it, and nothing about it is signed or pushed.
- Step 10 answers `degrees: 2` with a path rendered as two hops
  (`lookup-hop-0-0`, `lookup-hop-0-1`), alice trusts bob and bob trusts carol,
  each hop naming the identity, its resolved name and its own `fetched_at_ms`
  and `stale`. The response also carries `graph_stale`, `graph_truncated`,
  `truncated_by`, carol's outgoing trust list (empty here, so
  `lookup-trust-empty`) and a reverse list shaped `{best_effort: true,
  entries: [...]}`, labelled best effort every time it is shown
  (`lookup-reverse-label`).
- Step 11 answers 200 with `degrees: null` and an empty path list, stated as
  "shortest path found in this crawl" (`lookup-degrees` reads "none", and
  `lookup-degrees-none` says a path was not found within this crawl's caps)
  and never as "no relationship".
- `mabel graph sync` writes a new generation under
  `graph/generations/<sync_id>/` and swaps `graph/current.json` atomically. A
  lookup running during a sync reads the previous generation whole, never a
  half-written one.
- The crawl writes no stranger's ledger: after step 10, alice's `ledgers/`
  holds exactly the ledgers she controlled or fetched deliberately, and carol's
  is not among them.

## Deviations from the surface this story was drafted against

- Proposal 003 gives `invalid_utf8` for a display name carrying a bidi or
  zero-width control. The fold answers `invalid_display_name` for it, and
  keeps `invalid_utf8` for bytes that are not UTF-8 at all, which no JSON body
  and no CLI flag can carry. The spec asserts what the fold does.
- `mabel graph sync` with no `--peer` reaches no witness, so the crawl reads
  only what the home already holds. The CLI process has no seeded peer
  address, unlike the running wallet, which starts with the witness's ticket.
  The story runs the first sync through the UI and passes `--peer` to the CLI.
- A day cannot pass in a suite that runs in three minutes, so the stale case
  is set up by writing `/data/verification/<alice_id>.json` in alice's
  container with `checked_at_ms` 25 hours back. The cache is a rebuildable
  file, which is what makes that legitimate.
- Bob's ledger is pushed to the witness once more immediately before step 10.
  His profile events matter to the crawl, and alice can only read them from
  the witness.
- The spec asserts two things the story's outcomes do not name:
  `graph-sync-counts` reads `3 identities, 3 attestations` after the first
  sync, and `/data/graph/generations` holds at most two entries, because
  generations are caches collected down to the last two.
- "A lookup running during a sync reads the previous generation whole" is only
  smoke-checked here: the spec fires one lookup beside one sync and asserts it
  answers from one of the two generations. The outcome itself is pinned by the
  generation-swap unit tests in `crates/mabel-node/src/graph/tests.rs`.
- Payload tag 17 is asserted through its document spelling: the ledger route
  reports `payload_kind: "profile_update"`, which is the tag as every JSON
  surface renders it.
- "Two entries resolving to one name both show their full ids" is checked
  across two screens rather than inside one list: bob publishes alice's
  display name, and the spec reads alice's trust row for bob and alice's own
  overview. The full-id rendering of duplicates within one list is covered by
  `ui/src/test/resolved-identity.test.tsx`.
- "Verification gates nothing" is checked by rerunning the pinned trust
  verification of story 001 step 12 after the whole DNS sequence. Two of its
  fields are expected to move, so the comparison drops `fetched_at_ms` and
  masks the RFC 3339 time inside `statement`.

# 007: profile and verification

- Status: draft, blocked on tickets 023-029
- Surfaces: wallet UI (alice and bob), CLI, wallet HTTP API
- Test: `tests/e2e/007-profile-and-verification.spec.ts` (not written yet)

Alice gives her ledger a display name and a hostname, a TXT record backs the
hostname, and the wallet shows the verification state. Alice then looks bob's
contact up and sees how she knows him.

Nothing in this story is implemented. Every command, route, field and status
below is proposal 003's accepted surface, so the Playwright work can start the
moment tickets 023 to 029 land. This story names no `data-testid` at all: the
profile, verification, contact and lookup screens do not exist, so each step
names the route, the document field and the rendering rule a screen must
satisfy, and the testids arrive with tickets 027, 028 and 029.

## Actors

- alice: wallet node, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`.
- bob: wallet node, compose service `bob`, API and UI on
  `http://127.0.0.1:9082`.
- carol: a third identity in bob's home, trusted by bob and unknown to alice
  except through the crawl.
- witness: compose service `witness`, the only place alice can read bob's and
  carol's ledgers from.
- a test resolver: the container ticket 032 adds, serving TXT records for
  `example` names to the wallets. Nothing may reach the public internet.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root.

## Story

1. Run story 001 steps 1 to 12, then create carol in bob's home, name the
   witness on her ledger and push it, and have bob attest her:
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
2. Wire the resolver and the crawl sources. Blocked on ticket 032, which owns
   both: the resolver container with its zone and the wallets' resolver
   configuration, and `node.json.witnesses` becoming settable so the crawler's
   source order has a node-wide witness to query (ticket 024's `Resolver` seam
   covers unit tests only). Do not hand-wire either here; run this story
   against the overlay that ticket delivers.
3. Publish the TXT records on the test resolver:
   - `_mabel.alice.example. IN TXT "mabel=<alice_id>"`
   - `_mabel.bob.example. IN TXT "mabel=<carol_id>"`, a record that names the
     wrong identity on purpose
   - nothing at `_mabel.nobody.example.`
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
   key-value table: name, copyable id, declared kind, created, hostname with
   its verification icon, contact, and the counts. Read the hostname row for
   each of the three cases above and for an identity that claims no hostname.
9. Set a private contact note on bob, which is local and never signed:
   ```sh
   dc exec -T alice mabel contact set "$bob_id" --nickname "Bob from the pub" \
     --note "met at the meetup"
   ```
   The same store answers `GET` and `PUT
   /api/identities/<bob_id>/contact`, and it accepts foreign ids.
10. Synchronize the graph and look carol up, from alice's home:
    ```sh
    dc exec -T alice mabel graph sync
    dc exec -T alice mabel lookup "$carol_id" --from alice
    curl -fsS "http://127.0.0.1:9081/api/lookup/$carol_id?from=$alice_id"
    ```
    The first graph sync shows the consent panel in the UI, stating what
    becomes observable, and is remembered per node home.
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
  cleared field is absent from the wire rather than encoded empty.
- A `display_name` that parses as a valid identity id is refused with
  `invalid_display_name`, and one carrying a bidi or zero-width control with
  `invalid_utf8`: a name can never masquerade as an identifier.
- Step 6 answers `verification.status == "verified"`, `verification.hostname ==
  "alice.example"`, `verification.stale == false`, and both `checked_at_ms` and
  `last_verified_at_ms` set.
- Step 7: `bob.example` answers `verification.status == "mismatched"` (records
  exist under `mabel=` and none carries bob's id), and `nobody.example` answers
  `verification.status == "unverified"`. An identity with no hostname answers
  `verification.status == "unclaimed"`. With the resolver stopped, a forced
  check answers `unreachable` and does not overwrite a decisive result: the
  earlier `verified` entry keeps its `checked_at_ms` and the document reports
  both.
- Changing the hostname invalidates the old verdict: after a profile replace
  naming a different hostname, `verification.status` is not `verified` until a
  new check runs, because the cache entry is bound to the hostname it verified.
- `GET /api/identities` never triggers a DNS lookup: with the resolver stopped,
  listing every identity still answers from cache with the same
  `checked_at_ms` values.
- Step 8 renders each status distinctly: a check with the hostname for a fresh
  `verified`, a check with a stale marker for one older than 24 hours, a
  warning glyph for `mismatched`, dimmed text for `unverified` and
  `unreachable`, and nothing at all for `unclaimed`, with the same advisory
  note the declared kind carries. Verification gates nothing: the ledger and
  every verification report read the same with and without it.
- The name never renders like an id: the display name is plain text, the id and
  the hostname are monospace with the copy control, the id is always beside the
  name, and two entries resolving to one name both show their full ids. No
  screen sorts, matches or deduplicates on a name.
- Step 9 writes `contacts/<bob_id>.json` in alice's home only. Bob's wallet and
  the witness show no trace of it, and nothing about it is signed or pushed.
- Step 10 answers `degrees: 2` with a path rendered as two hops, alice trusts
  bob and bob trusts carol, each hop naming the identity, its resolved name and
  its own `fetched_at_ms` and `stale`. The response also carries
  `graph_stale`, `graph_truncated`, `truncated_by`, carol's outgoing trust list
  and a reverse list shaped `{best_effort: true, entries: [...]}`, labelled
  best effort every time it is shown.
- Step 11 answers 200 with `degrees: null` and an empty path list, stated as
  "shortest path found in this crawl" and never as "no relationship".
- `mabel graph sync` writes a new generation under
  `graph/generations/<sync_id>/` and swaps `graph/current.json` atomically. A
  lookup running during a sync reads the previous generation whole, never a
  half-written one.
- The crawl writes no stranger's ledger: after step 10, alice's `ledgers/`
  holds exactly the ledgers she controlled or fetched deliberately, and carol's
  is not among them.

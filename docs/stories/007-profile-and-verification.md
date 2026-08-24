# 007: profile and verification

- Status: implemented
- Surfaces: wallet UI (alice and bob), CLI, wallet HTTP API
- Test: `tests/e2e/specs/007-profile-and-verification.spec.ts`

Alice gives her ledger a display name and a hostname, a TXT record backs the
hostname, and the wallet shows the verification state. Alice then opens carol's
identity page and sees how she knows her, opens alice.example by hostname, and
browses what the witness holds.

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
8. Open alice's identity page at `http://127.0.0.1:9081/identities/<alice_id>`,
   which is where every identity is shown, local or foreign (proposal 004). The
   overview is one compact
   key-value table (`identity-detail`): name, copyable id, declared kind,
   alias, created, hostname with its verification mark, contact, and the
   counts. Read the `identity-detail-hostname` row for each of the three cases
   above and for carol, who claims no hostname. The mark sits inside that row
   as `identity-detail-hostname-verification`, and carol's row carries no
   mark at all. On carol's page in bob's UI open `action-verification`, which
   starts closed: `verification-status` reads `this identity claims no website`,
   and it says only that, because proposal 005 removed the DNS advisory
   sentence (`verification-note`) from every surface.
9. Set a private contact note on bob, which is local and never signed:
   ```sh
   dc exec -T alice mabel contact set "$bob_id" --nickname "Bob from the pub" \
     --note "met at the meetup"
   ```
   The same store answers `GET` and `PUT
   /api/identities/<bob_id>/contact`, and it accepts foreign ids.
10. Synchronize the graph from alice's wallet UI. A sync reads what witnesses
    hold, so the control lives on the witnesses screen (decision 017): click
    `nav-witnesses` and press `graph-sync-button` on the `graph-sync` card.
    Then open carol's page: paste `carol_id` into
    `wallet-search-input` on the wallet home and click `wallet-search-submit`,
    which navigates to `/identities/<carol_id>`. Carol's ledger is not in
    alice's home, so the page renders what the crawl read: the "How you know
    them" section (`lookup-result`) with the path, the degrees and the two
    lists. The same answer from the CLI and the route:
    ```sh
    dc exec -T alice sh -c 'mabel graph sync --peer "$(cat /shared/witness.ticket)"'
    dc exec -T alice mabel lookup "$carol_id" --from alice
    curl -fsS "http://127.0.0.1:9081/api/lookup/$carol_id?from=$alice_id"
    ```
    The first graph sync shows the consent panel (`graph-sync-consent`),
    stating what becomes observable, and is remembered per node home; its
    confirm button reads `Look now`. Before the first sync `graph-sync-state`
    reads `Your wallet has not looked yet.` and after it `Your wallet last
    looked just now.` A CLI sync needs `--peer`: that process holds no seeded
    peer address, while the running wallet started with the witness's ticket.
11. Look up an identity nobody in the crawl trusts, for the empty answer:
    `dc exec -T alice mabel lookup "$witness_id" --from alice`, and open
    `/identities/<witness_id>` through the same search box.
12. Open an identity by hostname. The search box takes a hostname as well as an
    id, resolves it through the node and navigates to what the TXT record
    names:
    ```sh
    curl -fsS http://127.0.0.1:9081/api/resolve/alice.example
    curl -fsS http://127.0.0.1:9081/api/resolve/nobody.example
    ```
    Type `alice.example` into `wallet-search-input` and click
    `wallet-search-submit`: the wallet lands on alice's own page. Type
    `nobody.example` and the wallet stays where it is and says what the lookup
    answered. Resolving is navigation, never verification: the page still draws
    alice's own advisory verdict.
13. Browse the witness. Click `nav-witnesses`, read the witness card, click
    `witness-card-link-<witness_id>` for what that witness holds, and click
    carol's card. Her ledger is not in this home, so the page offers one
    action: click `identity-fetch-button`, and the same page then renders as a
    stored ledger.
    ```sh
    curl -fsS http://127.0.0.1:9081/api/witnesses
    curl -fsS 'http://127.0.0.1:9081/api/witnesses/'"$witness_id"'/ledgers?offset=0&limit=256'
    dc exec -T alice ls /data/ledgers
    ```

## Verified outcomes

- Step 1: carol's push is accepted, and `GET
  http://127.0.0.1:9080/api/ledgers/<carol_id>` answers `entry.head_seq: 1`
  with `witnesses` naming the witness. Bob's ledger carries an unrevoked
  attestation for carol.
- Step 4 appends one `ProfileUpdate` (payload tag 17) to alice's ledger.
  `GET /api/identities/<alice_id>` answers `profile.display_name == "Alice
  Example"`, `profile.hostname == "alice.example"`, `profile.seq` equal to the
  new head, and `profile.signing_principal.identity == alice_id`. Proposal 005
  added `email` to the profile and replacement stays whole, so both the
  command's `previous` object and the ledger event's payload carry all three
  fields: `{display_name, hostname, email}`, with `email: null` for an identity
  that has published none.
- Step 5 exits 20 with `details.reason == "no_op_profile_update"` and appends
  nothing: an update whose effect equals the current folded profile is refused
  before signing.
- A profile replace that omits `--hostname` clears the hostname, and the
  cleared field is absent from the wire rather than encoded empty: the ledger
  event reports `payload.hostname: null` (and `payload.email: null` beside it),
  and an empty string would have been refused as an encoded default before it
  was signed.
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
  row reading `none`. A stale verified row also says `may be out of date`.
  Proposal 005 removed the DNS advisory sentence outright, so
  `identity-detail-verification-note` is absent from the page. Verification
  gates nothing: the ledger and every verification report read the same with
  and without it.
- The name never renders like an id: the display name is plain text
  (`identity-detail-resolved-name`), the id and the hostname are monospace
  with the copy control, the id is always beside the name (inside
  `identity-detail-resolved`, the one inline identity the page's heading
  draws), and two entries resolving to one name both show their full ids. No
  screen sorts, matches or deduplicates on a name.
- Step 9 writes `contacts/<bob_id>.json` in alice's home only. Bob's wallet and
  the witness show no trace of it, and nothing about it is signed or pushed.
- Step 10 answers `degrees: 2`, drawn as `2 steps` under the label `how far
  away`, with a path rendered as two steps (`lookup-hop-0-0`,
  `lookup-hop-0-1`), alice trusts bob and bob trusts carol, each step naming
  the identity, its resolved name and how fresh the reading is
  (`lookup-hop-0-1-fetched` reads `seen ...`). The response also carries
  `graph_stale`, `graph_truncated`, `truncated_by`, carol's outgoing trust list
  (empty here, so `lookup-trust-empty` reads `Your wallet has not seen them
  trust anyone.`) and a reverse list shaped `{best_effort: true, entries:
  [...]}`, labelled `Best effort: who your wallet has seen trusting them, not
  everyone who does` every time it is shown (`lookup-reverse-label`).
  `lookup-from` carries `alice_id`, the root the answer came from.
- Step 10's page is a foreign identity's page, so it carries no
  `identity-actions` and its pill is the crawl's distance, never ownership:
  `identity-detail-resolved-pill` carries `data-pill` `degree` and reads
  `trusted (2d)`.
  `identity-detail-ledger-summary` reads `your wallet holds no copy of it`, and
  `identity-detail-provenance` reads `nothing your wallet knows, so the id is
  the only label`: no profile and no local nickname name carol here. The crawl is
  fresh and reached everything, so neither `lookup-graph-stale` nor
  `lookup-graph-truncated` is drawn.
- Step 11 answers 200 with `degrees: null` and an empty path list, stated as
  one sentence: `lookup-degrees-none` reads `No connection found yet. Sync and
  try again.` and `lookup-degrees` inside it reads `No connection found`. It is
  never stated as "no relationship".
- Step 12: `GET /api/resolve/alice.example` answers `status: "resolved"` with
  `identity_id == alice_id`, and the search box lands on
  `/identities/<alice_id>`, which carries `identity-detail-resolved-pill`
  reading `your identity` and `identity-detail-hostname-verification` with
  `data-verification` `verified`. `GET /api/resolve/nobody.example` answers
  `status: "no_record"` with `identity_id: null`, and the search box stays on
  `/wallet` with `wallet-search-status` carrying `data-status` `no_record` and
  reading `_mabel.nobody.example.` and `names no identity`.
- Step 13: `GET /api/witnesses` answers one witness, `endpoint_id ==
  witness_id`, `named_by == [alice_id]` (alice's is the only ledger this home
  holds, and its chain names that witness) and `is_node_default: true` (the
  overlay set it). The card repeats both: `witness-card-named-by-<witness_id>`
  reads `chosen by 1 identity of yours`, `witness-card-default-<witness_id>`
  reads `this node uses it by default`, and the identifiers on the card are the
  endpoint id and `alice_id`.
- Step 13's drill-in renders what the witness holds as the identity card list:
  three cards, `alice_id`, `bob_id` and `carol_id`, in the order `GET
  /api/witnesses/<witness_id>/ledgers` answers, which reports `more: false`.
  Carol's card reads `identity-card-declared-kind-<carol_id>` `person` and
  `identity-card-head-seq-<carol_id>` `at position 1`.
- Step 13's fetch is the only thing that writes. Before it, carol's card leads
  to a page carrying `identity-fetch` and `dc exec -T alice ls /data/ledgers`
  holds `alice_id` alone: browsing a witness stores nothing. After
  `identity-fetch-button`, the same page draws `ledger-panel`,
  `identity-detail-head-seq` reads `1`, `identity-fetch` is gone, and
  `/data/ledgers` holds `alice_id` and `carol_id`. Storing a ledger is not
  controlling it: no key in this home signs for carol, so the fetch wrote no
  `identities/<carol_id>` link, `GET /api/identities` still lists `alice_id`
  alone, and the page carries no `identity-actions` and a
  `identity-detail-resolved-pill` still reading `data-pill` `degree`.
- `mabel graph sync` writes a new generation under
  `graph/generations/<sync_id>/` and swaps `graph/current.json` atomically. A
  lookup running during a sync reads the previous generation whole, never a
  half-written one.
- The crawl writes no stranger's ledger: after step 10, and until step 13
  fetches one on purpose, alice's `ledgers/` holds only `alice_id`. A crawl
  keeps what it reads in a generation, never as a replica.

## Deviations from the surface this story was drafted against

- Proposal 003 gives `invalid_utf8` for a display name carrying a bidi or
  zero-width control. The fold answers `invalid_display_name` for it, and
  keeps `invalid_utf8` for bytes that are not UTF-8 at all, which no JSON body
  and no CLI flag can carry. The spec asserts what the fold does.
- `mabel graph sync` with no `--peer` reaches no witness, so the crawl reads
  only what the home already holds. The CLI process has no seeded peer
  address, unlike the running wallet, which starts with the witness's ticket.
  The story runs the first sync through the UI and passes `--peer` to the CLI.
- Steps 12 and 13 are new with proposal 004: the hostname search box, the
  witness card list, the witness drill-in and the explicit fetch did not exist
  when this story was drafted. Step 13 runs last in the spec, because its fetch
  is the one write that would break "the crawl writes no stranger's ledger".
- A day cannot pass in a suite that runs in three minutes, so the stale case
  is set up by writing `/data/verification/<alice_id>.json` in alice's
  container with `checked_at_ms` 25 hours back. The cache is a rebuildable
  file, which is what makes that legitimate.
- Bob's ledger is pushed to the witness once more immediately before step 10.
  His profile events matter to the crawl, and alice can only read them from
  the witness.
- Step 13's fetch names no witness. `POST /api/identities/<carol_id>/fetch`
  with `from: null` asks the known witnesses in the crawler's source order,
  which is what the button sends; the running wallet holds the witness's
  address because its container started with the ticket.
- The spec asserts one thing the story's outcomes do not name:
  `/data/graph/generations` holds at most two entries, because generations are
  caches collected down to the last two. The crawl's counts left the screen with
  developer mode (decision 017), so the spec pins `node_count` and `edge_count`
  on `GET /api/graph` and reads only the last-looked sentence on the card.
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
  `ui/src/test/identity-inline.test.tsx`.
- The pill on an identity page is asserted by its `data-pill` attribute rather
  than by absence. Proposal 005 replaced `identity-own-badge` with the one pill
  both identity components draw, and a foreign page draws that pill too, so
  "nothing pretends this wallet can act for it" is checked as `data-pill`
  reading `degree`, not as a missing element. The degree comes from the stored
  crawl, which this story synchronizes before it reads carol's page.
- "Verification gates nothing" is checked by rerunning the pinned trust
  verification of story 001 step 12 after the whole DNS sequence. Two of its
  fields are expected to move, so the comparison drops `fetched_at_ms` and
  masks the RFC 3339 time inside `statement`.

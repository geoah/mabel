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
- the witness: compose service `witness`, the only place alice can read bob's
  and carol's records from. `witness_identity` is the Mabel id a record names;
  `witness_id` is the endpoint that answers for it.
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
   dc exec -T bob mabel witness add --identity carol --witness "$witness_identity"
   dc exec -T bob sh -c 'mabel sync push --identity carol \
     --peer "$(cat /shared/witness.ticket)"'
   dc exec -T bob mabel trust add --issuer bob --subject "$carol_id"
   dc exec -T bob sh -c 'mabel sync push --identity bob \
     --peer "$(cat /shared/witness.ticket)"'
   ```
   The witness add is not optional: a witness admits a ledger only when the
   pushed chain names an identity it witnesses for, so without it carol's push
   answers `NOT_ADMITTED` and the crawl has nothing to read. The trust chain is alice trusts bob, bob trusts carol.
2. Wire the resolver and the crawl sources. Both come from the overlay, and
   the bring-up runs in two phases because neither half of a witness exists
   until the witness has started: it mints its identity and advertises this
   container on it on its first start.
   ```sh
   dc up -d --wait witness resolver
   witness_identity="$(dc exec -T witness cat /shared/witness.identity)"
   witness_id="$(dc exec -T witness cat /shared/witness.id)"
   MABEL_WITNESSES="$witness_identity=$witness_id" dc up -d --wait
   ```
   The overlay reads `MABEL_WITNESSES` from the environment for both wallets,
   one `<mabel id>=<endpoint id>` entry per witness, and the entrypoint runs
   `mabel witness set-default` with both halves: `node.json` names an identity
   and the endpoints that answer for it (proposal 006 section 5.4). That is the
   node-wide witness the crawler's third source asks (proposal 003 section 3).
3. Publish the TXT records on the test resolver, by writing the zone file into
   the resolver's zone volume with the ids this run minted:
   - `_mabel.alice.example. IN TXT "mabel=<alice_id>"`, and beside it
     `_mabel.alice.example. IN TXT "mabel-endpoints=<alice's endpoint>,<witness_id>"`,
     the endpoints that answer for whatever identity that label claims (proposal
     006 section 6). The record is split across two character-strings, which a
     reader joins with no separator before it parses anything.
   - `_mabel.bob.example. IN TXT "mabel=<carol_id>"`, a record that names the
     wrong identity on purpose, with no endpoints beside it
   - `_mabel.many-machines.example.`, kept from
     `docker/dns/zones/example.zone`: a `mabel=` record and five endpoints split
     across two character-strings, because `mabel-endpoints=` plus four ids is
     227 of the 255 bytes a character-string holds. No container answers at any
     of the five, which is the point: what the label proves is the parsing rule.
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
   overview is the same card a list draws, opened and without its toggle
   (`identity-detail`): the declared kind as a badge, the name with the copyable
   id under it, then rows labelled in lowercase for the email, the nickname, the
   note, when it was created, the handle with its verification mark, the record
   and the counts. The row is labelled `handle`, because round 4 of proposal 005 calls
   it that everywhere a reader sees it; the testid keeps the document's own word
   for the field, `identity-detail-hostname`. Read that row for each of the
   three cases above and for carol, who claims no handle. The mark sits inside
   the row as `identity-detail-hostname-verification`, and carol's row carries
   no mark at all. On carol's page in bob's UI open `action-handle`, which
   starts closed: it holds `handle-current` reading `none`, the form that sets
   one, the sentence `Set a handle to see the line your DNS records need.` and
   the check. `verification-status` there reads `this identity claims no
   handle`, and it says only that, because proposal 005 removed the DNS advisory
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
    them" section (`lookup-result`) with the verdict, the path as a vertical
    chain of identity cards, and the two collapsed lists. Then go back to the
    wallet home and read the third section, `known-identities`: bob and carol are
    both there now, because the crawl reached them and this wallet controls
    neither. The same answer from the CLI and the route:
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
    curl -fsS 'http://127.0.0.1:9081/api/resolve?input=alice.example'
    curl -fsS 'http://127.0.0.1:9081/api/resolve?input=nobody.example'
    ```
    Type `alice.example` into `wallet-search-input` and click
    `wallet-search-submit`: the wallet lands on alice's own page, carrying the
    endpoints her label named on the query string. Type `nobody.example` and the
    wallet stays where it is and says what the lookup answered. Resolving is
    navigation, never verification: the page still draws alice's own advisory
    verdict.
12a. Read a label whole, and resolve a link. The same route takes three kinds of
    input, and the browser parses none of them (proposal 006 section 7):
    ```sh
    curl -fsS 'http://127.0.0.1:9081/api/resolve?input=many-machines.example'
    curl -fsS 'http://127.0.0.1:9081/api/resolve?input=bob.example'
    link="$(dc exec -T bob mabel identity share carol --endpoints "$witness_id" --json | jq -r .link)"
    curl -fsS "http://127.0.0.1:9081/api/resolve?input=$link"
    ```
    Paste that link into `wallet-search-input`: the wallet lands on carol's page
    with the link's endpoints on the query string, `identity-fetch-link-note`
    says what asking them does, and nothing is fetched until the button is
    pressed.
13. Browse the witness. Click `nav-witnesses`. A witness is an identity, so
    `witness-cards` draws the identity card every other screen draws, with
    `witness-default-<witness_identity>` reading `this node uses it by default`
    and one row per endpoint that answers for it, labelled `endpoint`, inside
    the card's
    record. Click `identity-card-link-<witness_identity>`: its page is the
    identity page, and what it keeps for other people is a section of it,
    `witness-holdings`, asked live over the sync protocol. `witness-chosen-by`
    and `witness-node-default` are the two facts the card used to carry, and one
    flat card list sits under the tab row `witness-holdings-filter`, whose
    three tabs are `witness-holdings-all`, `witness-holdings-trusted` and
    `witness-holdings-ours`. Put it back on `All`
    and click carol's card. Her record is not in this home, so the page offers
    one action: click `identity-fetch-button`, and the same page then renders as
    a stored record.
    ```sh
    curl -fsS http://127.0.0.1:9081/api/witnesses
    curl -fsS 'http://127.0.0.1:9081/api/witnesses/'"$witness_identity"'/holdings?offset=0&limit=256'
    curl -fsS 'http://127.0.0.1:9081/api/witnesses/'"$witness_id"'/holdings'
    curl -fsS http://127.0.0.1:9081/api/identities/known
    dc exec -T alice ls /data/ledgers
    ```
14. Set a handle in the UI, on bob's own identity in bob's wallet. Open
    `identity-card-link-<bob_id>`, click `action-handle-summary`, type
    `bob.example` into `handle-input` and click `handle-submit`. The first
    handle this node home publishes asks for consent first: `handle-consent`
    states that a name, an email and a handle set here stay readable forever,
    and its confirm button reads `Publish the handle`. Click it.
    `handle-result` reads `Saved at position <new head>.`, `handle-current` reads
    `bob.example`, and the panel shows the line to add to DNS,
    `_mabel.bob.example. IN TXT "mabel=<bob_id>"`. Then click
    `verification-check` in the same action: `_mabel.bob.example` names carol, so
    the verdict is `mismatched`, which is what step 7 read on the route.

## Verified outcomes

- Step 1: carol's push is accepted, and `GET
  http://127.0.0.1:9080/api/identities/<carol_id>` answers `identity.head_seq: 1`
  with `identity.witnesses` naming the witness identity. Bob's ledger carries an unrevoked
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
  before signing. Its `message` reads `Policy error: this profile is already the
  profile of mabel://<alice_id>: nothing would change`, with the prefix on the
  sentence a person reads and `details.ledger_id` beside it bare.
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
  naming a different hostname, `verification.status` is `unchecked` with
  `checked_at_ms: null` and `stale: false` until a new check runs, because the
  cache entry is bound to the hostname it verified. `unchecked` is the absence
  of a verdict and `unverified` is a lookup that found no `mabel=` record; a
  row carries the status alone, so the two are separate words (issue 042).
- Reading an identity never starts a lookup. `GET
  /api/identities/:identity_id` on a hostname this node has never checked
  answers `unchecked` and queries nothing, so opening a stranger's card does
  not tell that stranger's zone somebody here is reading it (decision 018).
- `GET /api/identities` never triggers a DNS lookup: with the resolver stopped,
  listing every identity still answers from cache with the same
  `checked_at_ms` values.
- Step 8 renders each status distinctly, on
  `identity-detail-hostname-verification`, whose `data-verification` attribute
  is the state the row draws: a check with the hostname for a fresh
  `verified`, a check with a stale marker (`stale-verified`) for one older
  than 24 hours, a warning glyph for `mismatched`, dimmed text for
  `unverified`, `unchecked` and `unreachable`, and, for `unclaimed`, no mark at
  all and the row reading `none`. An `unchecked` handle also says, on the
  handle screen, that it has not been checked from this wallet yet. A stale verified row also says `may be out of date`.
  Proposal 005 removed the DNS advisory sentence outright, so
  `identity-detail-verification-note` is absent from the page. Verification
  gates nothing: the ledger and every verification report read the same with
  and without it.
- The name never renders like an id: the display name is plain text
  (`identity-detail-resolved-name`), the id and the handle are monospace
  with the copy control, the id is always beside the name (inside
  `identity-detail-resolved`, the one inline identity the page's heading
  draws), and two entries resolving to one name both show their full ids. No
  screen sorts, matches or deduplicates on a name.
- Round 6 of proposal 005 draws the nickname this device keeps after the name an
  identity publishes, in parentheses, so a stolen public name and the name you
  gave them are both readable and tellable apart. When bob publishes alice's
  name, his card in alice's trust list reads
  `identity-card-name-<bob_id>-name` `Alice Example` and
  `identity-card-name-<bob_id>-nickname` `(Bob at the print shop)`, the nickname
  step 9 set, and it carries his whole Mabel ID: a card has the width for one, so
  `data-truncated` is `false`.
- Step 9 writes `contacts/<bob_id>.json` in alice's home only. Bob's wallet and
  the witness show no trace of it, and nothing about it is signed or pushed.
- Step 10 answers `degrees: 2`, stated as the sentence `lookup-degrees`
  reading `Connected through 2 steps` beside `lookup-verdict-pill`, which carries
  `data-pill` `degree` and says the same thing shorter. Round 5 of proposal 005
  made the verdict a sentence rather than a number in a labelled row, so no
  `lookup-degrees-row` exists.
- Step 10's path is a vertical chain of the same identity cards every other
  screen draws, `lookup-path-0`: the root you asked from
  (`lookup-hop-0-0-from-name` reading `Alice Example`), then one card per step
  (`lookup-hop-0-0` and `lookup-hop-0-1`), each under the word that links them.
  `lookup-hop-0-0-to-name` reads `Bob Example` and
  `lookup-hop-0-1-fetched` reads `seen ...`.
- Step 10's response also carries `graph_stale`, `graph_truncated`,
  `truncated_by`, carol's outgoing trust list and a reverse list shaped
  `{best_effort: true, entries: [...]}`. Both are collapsed cards, and a closed
  block holds none of its content, so each is opened to read it: opening
  `lookup-trust-toggle` draws `lookup-trust-empty` reading `Your wallet has not
  seen them trust anyone.`, and opening `lookup-reverse-toggle` draws bob's
  identity card, `identity-card-<bob_id>`, inside `lookup-reverse`. Round 5
  removed the per-entry rows and expanders, so no `lookup-reverse-row-<bob_id>`
  and no `lookup-reverse-expand-<bob_id>` exist. `lookup-trust-label` reads `Who
  they trust` and `lookup-reverse-label` reads `Who your wallet has seen trusting
  them`, with the caveat moved into the sentence its info tip holds:
  `lookup-reverse-note` has `aria-label` `Best effort: who your wallet has seen
  trusting them, not everyone who does`. `lookup-from` carries `alice_id`, the
  root the answer came from.
- Step 10's page is a foreign identity's page, so it carries no
  `identity-actions` and its pill is the crawl's distance, never ownership:
  `identity-detail-resolved-pill` carries `data-pill` `degree` and reads
  `trusted (2d)`. Round 4 of proposal 005 draws that pill in the card's top
  right corner rather than inside the name, so it is read by its own testid, and
  `identity-detail-unheld` beside it reads `not stored here`.
  `identity-detail-ledger-summary` reads `your wallet holds no copy of it`. Round
  5 removed the name-provenance row outright, so `identity-detail-provenance` is
  absent: which of the three sources a label came from is a fact about the label,
  and the card already shows what it has. The crawl is fresh and reached
  everything, so neither `lookup-graph-stale` nor `lookup-graph-truncated` is
  drawn. The one action the page offers besides fetching is `lookup-contact`,
  named `Update local info`, holding one `contact-save` reading `Save` that
  writes the nickname and the note together.
- Step 10 on the wallet home: `GET /api/identities/known` answers exactly
  `bob_id`, `carol_id` and `witness_identity`, sorted by the rendered id. The
  witness is a row by the same rule as the others: naming it on a chain meant
  resolving it first, and this home kept the copy it read, so it reads
  `stored: true` and `declared_kind: "service"`. Choosing
  `known-identities-trusted`
  drops it, because holding a record is not a reason to trust its subject. Bob reads `trusted: true`,
  `degrees: 1`, `stored: false`, `head_seq: null` and `alias: "Bob at the print
  shop"`, the nickname step 9 set; carol reads `trusted: false`, `degrees: 2` and
  `stored: false`. `known-identity-cards` draws one card each,
  `identity-card-name-<bob_id>-pill` carrying `data-pill` `trusted` and
  `identity-card-name-<carol_id>-pill` carrying `degree`, and both cards carry
  `not stored here` in `identity-card-unheld-<id>`. Pressing
  `known-identities-trusted` sets its own `aria-selected` to `true` and
  `known-identities-all` to `false`, and keeps both cards, because carol is
  reachable through bob: the tab covers direct trust and crawl distances alike.
  Both are `role="tab"` inside the `tablist` `known-identities-filter`.
- Step 11 answers 200 with `degrees: null` and an empty path list, stated as
  one sentence: `lookup-degrees-none` reads `No connection found yet.` and
  `lookup-degrees` inside it reads `No connection found`. It is never stated as
  "no relationship". No distance means nothing to say in a pill either, so
  `lookup-verdict-pill` and `lookup-paths` are both absent.
- Step 12: `GET /api/resolve?input=alice.example` answers `input_kind:
  "hostname"`, `status: "resolved"`, `identity_id == alice_id` and `endpoints`
  holding alice's endpoint and the witness's, sorted. The search box lands on
  `/identities/<alice_id>?machines=<those two>`, which carries
  `identity-detail-resolved-pill` reading `your identity` and
  `identity-detail-hostname-verification` with `data-verification` `verified`.
  `GET /api/resolve?input=nobody.example` answers `status: "no_record"` with
  `identity_id: null`, and the search box stays on `/wallet` with
  `wallet-search-status` carrying `data-status` `no_record` and reading
  `_mabel.nobody.example.` and `names no identity`.
- Step 12a: `?input=many-machines.example` answers `status: "resolved"` with
  that label's claimed id and all five endpoints, sorted by their rendered form,
  which is only possible if the two character-strings were joined first.
  `?input=bob.example` answers `status: "resolved"` with `endpoints: []`: a
  label with no `mabel-endpoints=` record names no endpoint, which is an answer
  and not a failure. `?input=<the link>` answers `input_kind: "link"`,
  `identity_id == carol_id`, `endpoints == [witness_id]`, `status: null` and
  `hostname: null`, because a link queries nothing. Pasting it navigates to
  `/identities/<carol_id>?machines=<witness_id>` and writes nothing:
  `/data/ledgers` still holds `alice_id` and `witness_identity` alone.
- Step 13: `GET /api/witnesses` answers one witness, `identity_id ==
  witness_identity`, `named_by == [alice_id]` (alice's is the only record this
  home signs for, and its chain names that witness) and `is_node_default: true`
  (the overlay set it). Its `endpoints` holds one entry, `{endpoint_id:
  <witness_id>, binding: "hinted"}`: the only chain this home ever read for the
  witness was served by that same endpoint, and an endpoint that served its own
  evidence proves nothing (proposal 006 section 4.2).
- Step 13's card is the identity card: `witness-cards` holds exactly
  `identity-card-<witness_identity>`,
  `witness-default-<witness_identity>` reads `this node uses it by default`,
  and opening the card with `identity-card-expand-<witness_identity>` draws one
  `endpoint` row, `identity-card-machine-<witness_id>-<witness_identity>`, whose
  sentence `identity-card-machine-<witness_id>-note-<witness_identity>` reads
  `No record we have confirms that this endpoint answers for it.` The testids
  keep the older spelling of the row; only the label a reader sees changed.
- Step 13's page is the identity page, and the section on it reads
  `witness-chosen-by` `1 of your identities` and `witness-node-default` `yes,
  for the identities that chose no witness of their own`.
- Step 13's section renders what the witness holds as the identity card list:
  four cards, `alice_id`, `bob_id`, `carol_id` and `witness_identity`, because
  a witness serves its own record like any other, in the order `GET
  /api/witnesses/<witness_identity>/holdings` answers, which reports `more:
  false`. An endpoint id at that path is refused by name: `GET
  /api/witnesses/<witness_id>/holdings` answers 404 with `details.reason ==
  "endpoint_not_identity"` and the message `<witness_id> is an endpoint this home
  knows, not a witness identity`.
  Carol's card reads `identity-card-declared-kind-<carol_id>` `person` and
  `identity-card-entries-<carol_id>` `2 entries`: how much of a record this
  witness holds is what the listing is about, and round 5 of proposal 005 took
  the position off the cards.
- Step 13's three tabs narrow that one list, `All` chosen when the page
  opens with `aria-selected` `true`, and the sentence under the heading says
  which is chosen: `Every record this witness holds.` for `All`, `The records
  your own identities control.` for `Yours`, which leaves `alice_id` alone, and
  `The people you trust, and the ones your wallet reaches through them.` for
  `Trusted`, which holds `bob_id`, whom alice trusts outright, and `carol_id`,
  whom the crawl reached through him, and not `alice_id`. Clicking `All` again
  restores the three cards in their original order.
- Step 13's fetch is the only thing that writes. Before it, carol's card leads
  to a page carrying `identity-fetch` and `dc exec -T alice ls /data/ledgers`
  holds no `carol_id`: browsing a witness stores nothing. After
  `identity-fetch-button`, the same page draws `ledger-panel`,
  `identity-detail-event-count` reads `2`, `identity-detail-unheld` is gone
  because the record is stored now, `GET /api/identities/<carol_id>` answers
  `head_seq: 1`, `identity-fetch` is gone, and
  `/data/ledgers` holds `alice_id`, `witness_identity` and `carol_id`. Storing a ledger is not
  controlling it: no key in this home signs for carol, so the fetch wrote no
  `identities/<carol_id>` link, `GET /api/identities` still lists `alice_id`
  alone, and the page carries no `identity-actions` and a
  `identity-detail-resolved-pill` still reading `data-pill` `degree`.
- `mabel graph sync` writes a new generation under
  `graph/generations/<sync_id>/` and swaps `graph/current.json` atomically. A
  lookup running during a sync reads the previous generation whole, never a
  half-written one.
- The crawl writes no stranger's ledger: after step 10, and until step 13
  fetches one on purpose, alice's `ledgers/` holds `alice_id` and
  `witness_identity` and nothing else. A crawl keeps what it reads in a
  generation, never as a replica; the witness's record is there because naming a
  witness resolved it.
- Step 14 appends one `ProfileUpdate` to bob's ledger and changes nothing else
  about it: `GET /api/identities/<bob_id>` answers `profile.hostname ==
  "bob.example"`, `profile.display_name` unchanged, and `profile.seq` equal to
  the position `handle-result` reported. Setting a handle replaces the whole
  profile, so the public name and email travel with it untouched.
- Step 14's check reports the same verdict the route reported in step 7:
  `verification-mark` carries `data-verification` `mismatched`,
  `verification-detail` reads `the mabel= record at _mabel.bob.example. names
  another identity`, and `verification-checked-at-ms` no longer reads `never`.

## Deviations from the surface this story was drafted against

- Proposal 003 gives `invalid_utf8` for a display name carrying a bidi or
  zero-width control. The fold answers `invalid_display_name` for it, and
  keeps `invalid_utf8` for bytes that are not UTF-8 at all, which no JSON body
  and no CLI flag can carry. The spec asserts what the fold does.
- `mabel graph sync` with no `--peer` reaches no witness, so the crawl reads
  only what the home already holds. The CLI process has no seeded peer
  address, unlike the running wallet, which starts with the witness's ticket.
  The story runs the first sync through the UI and passes `--peer` to the CLI.
- Steps 12 and 13 are new with proposal 004: the handle search box, the
  witness card list, the witness drill-in and the explicit fetch did not exist
  when this story was drafted. Step 13 runs last but one in the spec, because
  its fetch is the one write that would break "the crawl writes no stranger's
  ledger".
- Step 14 is new with round 4 of proposal 005, which gave the handle its own
  action. Steps 4 and 7 still set every handle on the CLI, because step 5 needs
  the same command run twice for `no_op_profile_update` and steps 6 to 12 pin
  exact positions on alice's and bob's chains. Step 14 runs last, on bob's own
  identity, where the append it makes is read by nothing after it.
- A day cannot pass in a suite that runs in three minutes, so the stale case
  is set up by writing `/data/verification/<alice_id>.json` in alice's
  container with `checked_at_ms` 25 hours back. The cache is a rebuildable
  file, which is what makes that legitimate.
- Bob's ledger is pushed to the witness once more immediately before step 10.
  His profile events matter to the crawl, and alice can only read them from
  the witness.
- Step 10 opens the two crawl lists by clicking `lookup-trust-label` and
  `lookup-reverse-label` rather than the toggle row itself. The info icon beside
  the reverse heading is a button inside the toggle's button, and it stops the
  click so a tap on it opens its sentence and nothing else; the sections lost
  their borders in the final round of proposal 005, which widened the row enough
  that its centre point falls on that icon. Clicking the heading is what a reader
  aims at anyway.
- Step 13's `Trusted` tab is asserted per card rather than as a set. Whether
  an id counts as trusted here depends on the stored crawl, so the spec waits on
  `identity-card-<bob_id>` and `identity-card-<carol_id>` being drawn and on
  `identity-card-<alice_id>` being absent, which retries while the page's
  lookups settle.
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
  display name, and the spec reads bob's card in alice's trust list and alice's
  own overview. Round 4 of proposal 005 keys that list by the subject, so the
  card is `identity-card-name-<bob_id>` and the entry that said it is pinned on
  `GET /api/identities/<alice_id>` instead. The full-id rendering of duplicates
  within one list is covered by `ui/src/test/identity-inline.test.tsx`.
- Step 10's known-identities half is a separate test in the spec, run straight
  after the lookup one. It reads `GET /api/identities/known` and the third section
  of the wallet home, both of which round 6 of proposal 005 added, on the state
  the crawl of step 10 left behind.
- The positions this story used to read on the screen are read on `GET
  /api/identities/<id>` through the shared `expectHeadSeq` helper. Round 5 of
  proposal 005 removed `identity-detail-head-seq` and
  `identity-card-head-seq-<id>`; where the screen still has something to say, the
  spec reads `identity-detail-event-count` on a page and
  `identity-card-entries-<id>` on a witness listing.
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

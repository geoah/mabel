# 001: two people meet

- Status: implemented
- Surfaces: wallet UI (alice and bob), CLI, the node HTTP API
- Test: `tests/e2e/specs/001-two-people-meet.spec.ts`

Two strangers create identities in two wallet UIs, exchange `mabel://` links out
of band, name the same witness identity, push, and each attests trust in the
other. A third party with an empty home reads the result from the machine that
answers for the witness.

## Actors

- alice: a node holding one key, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`.
- bob: the same, compose service `bob`, on `http://127.0.0.1:9082`.
- the witness: a node that keeps other people's records, compose service
  `witness`, API and UI on `http://127.0.0.1:9080`. It is not a different
  program: it mints a witness identity on its first start, publishes the machine
  that answers for it on that identity's own record, and lists the identity in
  `node.json.witness_for` (proposal 006 sections 1, 2 and 4).
- a stranger: one throwaway container with an empty home, holding no identity
  and no key but the node key it makes on the spot.

Host and port must be `127.0.0.1` (or `localhost`) on the port the node bound,
or the API answers 403 `host_not_loopback` (proposal 001 section 10). `dc`
below stands for `docker compose -f docker/compose.yaml`, run from the
repository root.

## Story

1. Bring the topology up from nothing: `dc down -v && dc up -d --wait`. All
   three services report healthy. Read the witness's two ids, both 52-character
   lowercase base32 strings and both published by the entrypoint beside the
   ticket: `witness_identity="$(dc exec -T witness cat /shared/witness.identity)"`,
   the Mabel id a record names, and `witness_id="$(dc exec -T witness cat
   /shared/witness.id)"`, the machine a wallet dials for it. `GET
   http://127.0.0.1:9080/api/node` answers `witness_for` holding one entry,
   `{identity: <witness_identity>, advertised: true, reason: null}`.
2. Open `http://127.0.0.1:9081/wallet`. The nav holds three entries and no
   fourth, `nav-wallet`, `nav-witnesses` and `nav-node`: every node serves the
   same three, whatever else it does (proposal 006 section 8). The page is three
   flat sections under three headings (round 6 of proposal 005): the search box
   `wallet-search`, whose field is labelled `Mabel ID, handle or link` and whose
   placeholder reads `alice.example, or paste a Mabel ID or a link`, then
   `identity-list` with `identity-list-empty` reading `You have no identities
   yet. Create one below.`, then `known-identities` with its
   `known-trusted-only` switch off and `known-identities-empty` reading `Your
   wallet knows of no other identity yet.`, because this wallet has fetched,
   crawled and noted nobody. `GET /api/node` carries no `role` field at all:
   what a node can do is read from what it holds, so this one answers
   `identity_count: 0` and an empty `witness_for`.
3. Click `identity-create-summary` to unfold the create form, which the wallet
   home keeps closed. Type `alice` into `identity-create-alias`, labelled
   `Private nickname (only this device sees it)` because it never leaves this
   device (proposal 005). Leave the two optional public fields
   `identity-create-display-name` and `identity-create-email` empty, so this
   identity publishes nothing and its head stays at position 0. Leave
   `identity-create-declared-kind` at `person`, leave `identity-create-founder`
   empty, click `identity-create-submit`. `identity-create-result-identity-id`
   appears; record its identifier `data-value` as `alice_id`.
4. Repeat step 3 at `http://127.0.0.1:9082/wallet` with alias `bob`; record
   `bob_id`.
5. Exchange links out of band. A `mabel://` link carries an identity's Mabel id
   and up to four machines that answer for it (proposal 006 section 7); neither
   half is proof of anything, which is what the flag-L sentence in every report
   says.
   ```sh
   dc exec -T bob mabel identity share bob --json
   dc exec -T alice mabel identity share alice --json
   ```
   Neither has advertised a machine yet, so `--endpoints auto` falls back to the
   machine this home runs on and each document reads `endpoints_from: "node"`
   with a `link` of `mabel://<id>?endpoints=<that machine>`. Paste bob's link
   into `wallet-search-input` in alice's UI and click `wallet-search-submit`.
   The browser parses no link: the box hands the string to the node, which owns
   the grammar. Alice lands on `/identities/<bob_id>?machines=<bob's machine>`,
   where `identity-fetch` offers to fetch the record this home does not hold and
   `identity-fetch-link-note` says first what asking those machines does. Paste
   alice's link into bob's UI the same way.
6. In alice's UI click `identity-card-link-<alice_id>`: the whole card is one
   link to `/identities/<alice_id>`. On the identity page click
   `action-witnesses-summary` to open the action, which starts closed, put
   `$witness_identity` into `witness-add-identity` and click
   `witness-add-submit`. A witness is an identity, so the box takes its Mabel
   ID and the event names that id rather than the machine behind it.
   `witness-add-head-seq` reads `Saved at position 1.` and the row
   `witness-row-<witness_identity>` appears, with
   `witness-row-<witness_identity>-link` opening that identity's own page and
   the id written out whole. Do the same in bob's UI.
7. In each UI click `action-push-summary`, leave `sync-push-to` empty and click
   `sync-push-submit`. `sync-push-report` appears with
   `push-status-<witness_id>` reading `accepted`,
   `push-stored-<witness_id>` reading `2` and `sync-push-head-seq` reading `1`.
8. In alice's UI click `action-trust-summary`, paste `bob_id` into
   `trust-add-subject` and click `trust-add-submit`. `trust-appended-event`
   shows the new event id, a card `identity-card-<bob_id>` appears in
   `trust-list`, and `identity-detail-event-count` reads `3`. Record the event id
   as `alice_attestation`. Round 5 of proposal 005 counts a record's entries and
   never names the position its newest one sits at, so the head is read on `GET
   /api/identities/<alice_id>`, which answers `head_seq: 2`. The list is keyed by
   the identity trusted, not by the entry that said it: the entry is read on the
   record.
9. In bob's UI do the same with `alice_id` as the subject. Trust is one-way
   (decision 003), so this is a second event in a second ledger, not a
   handshake.
10. Click `sync-push-submit` in both UIs again. Each report reads
    `push-status-<witness_id>` `accepted` and `push-stored-<witness_id>` `1`.
11. A stranger verifies from an empty home, reading the witness's copy and
    nothing else:
    ```sh
    docker run --rm --network mabel_mabel \
      --volume mabel_witness-ticket:/shared:ro \
      --env MABEL_WAIT_FOR_TICKET=/shared/witness \
      mabel:dev verify trust --issuer "$alice_id" --subject "$bob_id" \
      --from "$witness_id"
    ```
    The compose project is named `mabel`, so the bridge is `mabel_mabel` and
    the ticket volume is `mabel_witness-ticket`; `docker network ls` and
    `docker volume ls` confirm both.
12. Run step 11 again with `--json` for the document assertions below.
13. The subject nobody can read. Alice creates a third identity that never
    reaches the witness, and attests it:
    ```sh
    dc exec -T alice mabel identity create --alias carol --kind person
    ```
    Record `carol_id`. In alice's UI open `action-trust`, paste `carol_id` into
    `trust-add-subject`, click `trust-add-submit` (`identity-detail-event-count`
    reads `4` and the route answers `head_seq: 3`), then open `action-push` and
    click `sync-push-submit`.
14. Verify that attestation from an empty home. Carol's ledger is in nobody's
    reach: alice pushed her own ledger, not carol's.
    ```sh
    docker run --rm --network mabel_mabel \
      --volume mabel_witness-ticket:/shared:ro \
      --env MABEL_WAIT_FOR_TICKET=/shared/witness \
      mabel:dev verify trust --issuer "$alice_id" --subject "$carol_id" \
      --from "$witness_id" --json
    ```
    The subject's participation is deliberately not required (decision 003), so
    this is an answer, not a failure.
15. Alice saves her keys. Open `identity-card-link-<alice_id>`. What she can do
    to this identity is four groups under four headings, `action-group-profile`
    (`Profile`), `action-group-trust` (`Trust`), `action-group-witnesses`
    (`Witnesses and sync`) and `action-group-control` (`Control and keys`), and
    saving keys is in the last of them. Click `action-keys-summary`.
    `identity-keys-active` and `identity-keys-reserve` each hold a 52-character
    lowercase base32 secret key, and `identity-keys-warning` says what holding
    them and losing them means. The same two values are what `GET
    http://127.0.0.1:9081/api/identities/<alice_id>/keys` answers (decision
    017).
16. A new identity that publishes something from birth. In alice's UI unfold
    the create form again, type `dana` into `identity-create-alias`, `Dana
    Example` into `identity-create-display-name`, `dana@dana.example` into
    `identity-create-email`, leave the kind at `person` and click
    `identity-create-submit`. Record `dana_id`. Dana is never witnessed and
    never pushed, so nothing earlier in this story moves. Back on the wallet
    home her card reads `Dana Example (dana)`, and her public email is in the
    opened card, one click into `identity-card-expand-<dana_id>`.
17. Read the node page. Click `nav-node`, the third nav entry. It draws short
    rows of what `GET /api/node` answers about the program doing the work: the
    Iroh ID other nodes dial it by, how it is reachable, how many identities it
    signs for, whose records it keeps, how many records and conflicts it holds,
    the space it uses and the build running. There is no role row and no role
    field: what a node can do is read from what it holds (proposal 006 section
    8). Where the API listens left the page with round 5 of proposal 005, so
    there is no `node-http-bind` row. `node-endpoint-id` is what `dc exec -T
    alice mabel node id` prints, written out whole because it is the only name a
    node has. Under the rows, `node-witnesses` lists the witnesses this node
    uses by default, one identity card each.
18. Read the witnesses screen. Click `nav-witnesses`. `witness-cards` holds one
    card, `identity-card-<witness_identity>`, because a witness is an identity
    and its card is the identity card every other screen draws.
    `witness-default-<witness_identity>` reads `this node uses it by default`.
    Open the card with `identity-card-expand-<witness_identity>` and the record
    carries one `machine` row per machine that answers for it,
    `identity-card-machine-<witness_id>-<witness_identity>`. `/witness` is not a
    route at all and `/witnesses/<witness_identity>` redirects to
    `/identities/<witness_identity>`, so a saved link still opens something.

## Verified outcomes

- Step 3: `identity-create-result-identity-id` and
  `identity-create-result-inception-event` carry the same `data-value`: an
  identity is the digest of its own inception event.
- Step 6: `GET http://127.0.0.1:9081/api/identities/<alice_id>` answers
  `identity.witnesses == ["<witness_identity>"]`, `identity.head_seq: 1`,
  `identity.event_count: 2`. Naming a witness resolves it first, so alice's home
  now holds a copy of the witness's own record.
- Step 8: `trust-list` holds one card, `identity-card-<bob_id>`, and the
  identity document carries `identity.trust[0].subject == bob_id`,
  `identity.trust[0].revoked == false` and `identity.trust[0].attestation_event
  == alice_attestation`.
- Step 11 exits 0. Its stdout is five lines in this order:
  - `trusted: true`
  - `valid as of seq 2 of <alice_id>, fetched from <witness_id> at <RFC 3339
    UTC>; no revocation up to seq 2`
  - `signed by principal <alice_id> (<alice active key>)`
  - `subject control was not proven to this verifier; the issuer is
    responsible for out-of-band confirmation`
  - `Verified means this identity signed this statement at this position in
    its chain. It is not proof that the statement is true, not proof of legal
    identity, and not proof of unique humanity.`
- Step 12's document has `ok: true`, `kind: "trust"`, `trusted: true`,
  `subject_resolution: "resolved"`, `subject_note: null`,
  `attestation_event == alice_attestation`, `attestation_seq: 2`,
  `signing_principal.identity == alice_id`, `revoked_count: 0`,
  `source == witness_id`, `sources_queried == [witness_id]`, `head_seq: 2`.
- The mirrored verification, `--issuer "$bob_id" --subject "$alice_id"`, also
  exits 0 with `trusted: true`: two ledgers, two events.
- After step 10, `GET http://127.0.0.1:9080/api/identities/known?offset=0&limit=256`
  on the witness lists exactly two rows, `alice_id` and `bob_id`, each with
  `declared_kind: "person"`, `head_seq: 2` and `stored: true`. A witness's
  holdings are the records it stores and cannot sign for, which is the route
  every node answers: `/api/ledgers` is gone and a witness needs no route of its
  own (proposal 006 section 8). How many entries each record holds is on `GET
  /api/identities/<id>`, which answers `event_count: 3`, and a conflict is on
  `GET /api/forks`, which answers an empty `entries`. The rows are sorted by the
  rendered id, which orders the digits before the letters, where `GET
  /api/identities` sorts by the bytes they encode.
- Step 14 exits 0, and its document reads `trusted: true` with
  `subject_resolution: "unresolved"` and `subject_note` exactly `subject:
  unresolved (not held by any queried source)`. The text form prints that
  sentence as its own line, after `signed by principal ...` and before the two
  standing sentences.
- Step 14's `head_seq` is 3 and its statement reads `valid as of seq 3 of
  <alice_id>, fetched from <witness_id> at <RFC 3339 UTC>; no revocation up to
  seq 3`. An unresolved subject changes what is reported, never the exit code:
  only chain, signature and equivocation failures exit 20.
- `GET http://127.0.0.1:9080/api/identities/<carol_id>` answers 404 with
  `details.reason == "unknown_ledger"`: the witness holds no copy of the
  subject, which is exactly what step 14 reported. One node, one spelling.
- After step 13 alice's wallet home draws one card per identity, in the
  ascending identity id order `GET /api/identities` answers in:
  `identity-cards` holds `identity-card-<alice_id>` and
  `identity-card-<carol_id>`. Alice's card reads
  `identity-card-name-<alice_id>-name` `alice` and
  `identity-card-declared-kind-<alice_id>` `person`. She publishes no name, so
  the nickname is the name and `identity-card-name-<alice_id>-nickname` is absent.
  Her Mabel ID is on the card whole, `data-truncated="false"`.
  `identity-card-link-<alice_id>` points at `/identities/<alice_id>`: the card
  is the page, and there is no selection state anywhere. No card names a position
  at all, so no `identity-card-head-seq-*` element exists and the heads are read
  on `GET /api/identities/<id>`: alice at `head_seq: 3`, carol at `head_seq: 0`,
  because she was created and never appended to.
- Each card carries the one expand affordance this app draws,
  `identity-card-expand-<alice_id>`, a small icon button whose `aria-label` is
  `Show the record` closed and `Hide the record` open, holding a chevron that
  turns over rather than sideways. Opening it draws
  `identity-card-details-<alice_id>`: the row labels are lowercase (`nickname`
  reading `alice`), `identity-card-event-count-<alice_id>` reads `4`, and there is
  no `identity-card-principals-<alice_id>` row, because alice holds her own key
  and nothing else can act for her.
- Alice and carol are both identities this wallet signs for, so the one row in
  `known-identity-cards` is the witness: naming it on a chain meant resolving it
  first, and this home kept the copy it read. Bob is not there. Opening his link
  read nothing into this home, which is what makes viewing safe (proposal 004).
- Step 15: `GET /api/identities/<alice_id>/keys` answers 200 with `identity_id
  == alice_id`, an `active_secret_key` and a `reserve_secret_key` matching what
  the two boxes hold, and an `active_key` equal to `identity.active_key` of the
  identity document.
- Step 16: giving a public name or email makes the node append one
  `ProfileUpdate` at seq 1 right after the inception, so a new identity's first
  two entries are what it is and what it shows the world (proposal 005). The
  create result draws `identity-create-result-profile`, whose
  `identity-create-result-display-name` reads `Dana Example` and
  `identity-create-result-email` reads `dana@dana.example`. `GET
  /api/identities/<dana_id>` answers `head_seq: 1`, `event_count: 2`,
  `profile.display_name == "Dana Example"`, `profile.email ==
  "dana@dana.example"`, `profile.hostname: null` and `profile.seq: 1`. `GET
  /api/identities/<dana_id>/ledger?since=0&limit=8` answers two events whose
  `payload_kind` values are `inception` then `profile_update`, the second
  carrying `{display_name: "Dana Example", hostname: null, email:
  "dana@dana.example"}`.
- Step 16 on the wallet home: a card is named by the name the identity
  publishes, with the nickname only this device sees in parentheses after it, so
  `identity-card-name-<dana_id>-name` reads `Dana Example` and
  `identity-card-name-<dana_id>-nickname` reads `(dana)`. Round 6 of proposal 005
  put the public email in the opened card alone, so
  `identity-card-email-<dana_id>` is absent until
  `identity-card-expand-<dana_id>` is pressed, and then reads
  `dana@dana.example` under the lowercase label `email`.
- Step 17: no screen names a role, so `node-role` does not exist. `node-relay`
  reads `direct connections only` (the topology sets `MABEL_RELAY=disabled`),
  `node-endpoint-id` carries what `mabel node id` prints and is not truncated,
  under the label `Iroh ID`. `node-version` repeats the document's own value,
  `node-storage` ends `of 2.1 GB` (the topology's `MABEL_STORAGE_CAPACITY`), and
  the four counts are bare numbers under their own labels:
  `node-identity-count` (`identities`), `node-ledger-count` (`records`),
  `node-fork-count` (`conflicts`) and `node-witness-for` (`keeps records for`),
  which reads `none` on a home that keeps nobody else's. `node-witness-cards`
  draws the witness this node uses by default, because the entrypoint recorded
  it from the ticket volume. No node draws `node-http-bind`, though the document
  still carries `http_bind`.
- Step 18: `/witness` draws `route-not-found`, and `/witnesses/<id>` lands on
  `/identities/<id>`.

## Deviations

Where `tests/e2e/specs/001-two-people-meet.spec.ts` departs from or exceeds the
story text above.

- The spec asserts two testids the story never names. `identity-detail` is how
  the shared `openIdentity` helper knows an identity page opened, and
  `identity-detail-resolved` is read in a test that checks the whole
  52-character value is what `data-value` holds. Proposal 005 draws the page's
  heading through the one inline identity component, so the id sits inside that
  element rather than in a row of its own.
- Step 16 is where the create-with-a-profile capability of proposal 005 is
  pinned, rather than in step 3. Steps 6 to 14 depend on exact positions
  (`witness_config` at 1, the first attestation at 2), and a profile entry at
  seq 1 would move every one of them. A third identity created last, asserted
  and left unwitnessed, changes nothing those steps read. The spec runs it after
  the wallet-home card test for the same reason: that test pins the card list as
  exactly `alice_id` and `carol_id`.
- Step 13 creates carol with `--json` added, so the spec can read `carol_id`
  from the document instead of parsing the text form.
- Step 2 asserts what `GET /api/node` does *not* carry: there is no `role` key
  on any document and no testid for one. The three nav entries and the search
  box are what the spec reads on the screen.
- The shared `createIdentity` helper clicks `identity-create-summary` only when
  the form is not already on the screen: a summary click toggles, so a second
  one would close the form the previous step opened. The shared `openAction`
  helper does the same for each closed action of steps 6, 7, 8 and 15, reading
  the block's `data-state` to decide.
- The spec runs step 15 straight after step 7, where it needs nothing that
  steps 8 to 14 add, so the whole keys assertion sits in one test.
- Step 17 runs last in the spec, after step 16, so `node-identity-count` is
  read against `GET /api/node` rather than against a number this story would
  have to keep in step with every identity it creates.
- Every position this story used to read on the screen is read on `GET
  /api/identities/<id>` instead, through the shared `expectHeadSeq` helper. Round
  5 of proposal 005 removed `identity-detail-head-seq` and
  `identity-card-head-seq-<id>` outright; where the UI still has something to
  say, the spec reads `identity-detail-event-count`, which counts the entries a
  record holds.
- The spec asserts the chevron's `data-state` inside
  `identity-card-expand-<alice_id>`, which the story does not name. It is how
  "the chevron turns over rather than sideways" is checked from outside the
  stylesheet; the rotation itself is pinned by
  `ui/src/test/identity-card.test.tsx`.

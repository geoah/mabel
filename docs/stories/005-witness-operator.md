# 005: the witness operator

- Status: implemented
- Surfaces: witness UI, CLI, witness HTTP API
- Test: `tests/e2e/specs/005-witness-operator.spec.ts`

Someone runs a witness and wants to know what it holds. The debug route is the
same two screens the wallet has, read-only: the card list of its holdings and
one page per ledger. It names the fork it recorded and issues nothing but
reads.

Proposal 004 took the node card, the ledger table, its paging controls and the
event form out of that route. Everything they showed is still answered by the
witness API, so this story reads on the screen what the screen draws and on the
route what the route reports.

## Actors

- witness one: compose service `witness`, API and UI on
  `http://127.0.0.1:9080`. The node being inspected.
- alice: wallet node, compose service `alice`, the source of every ledger the
  witness holds.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root. This story starts from story 004, so it inherits story 004's
hand-started second witness and second machine (an operator would use ticket
032's `compose.two-witnesses.yaml` overlay instead; the spec hand-starts them
for the reason story 004 states), and step 11 tears them down.

## Story

1. Run story 004 steps 1 to 7, stopping before its teardown. Witness one now
   holds alice's ledger at seq 3 and has recorded one fork record for it. Keep
   `alice_id` and `witness_id`.
2. Give the witness five ledgers. Carol and dave exist in alice's home from
   story 004 but were never pushed:
   ```sh
   dc exec -T alice mabel identity create --alias erin --kind person
   dc exec -T alice mabel identity create --alias mabel-demo-co \
     --kind organization --founder alice
   for name in carol dave erin mabel-demo-co; do
     dc exec -T alice mabel witness add --identity "$name" --endpoint "$witness_id"
     dc exec -T alice sh -c 'mabel sync push --identity '"$name"' \
       --peer "$(cat /shared/witness.ticket)"'
   done
   ```
   Record `org_id`. The witness now holds five ledgers, four `person` and one
   `organization`.
3. Read the node facts, which the UI no longer draws:
   `curl -fsS http://127.0.0.1:9080/api/node`.
4. Open `http://127.0.0.1:9080/witness`. It is one card list, `identity-cards`,
   holding all five ledgers with no paging control anywhere: the route asks for
   every ledger it holds at once. Read the two standing notes above the list.
   Ledger ids are digests, so which card is where is whatever the ordering
   says: read the ids out of the DOM and assert per card, never "the org is
   first".
5. Read the declared kind of each card,
   `identity-card-declared-kind-<ledger_id>`.
6. Read the fork counts. A card carries
   `identity-card-fork-count-<ledger_id>` only when the witness recorded a fork
   for that ledger, so exactly one card carries it.
7. Page the route instead of the screen. Paging is still how the store is read
   over HTTP, and the order is what makes it stable:
   ```sh
   curl -fsS 'http://127.0.0.1:9080/api/ledgers?offset=0&limit=4'
   curl -fsS 'http://127.0.0.1:9080/api/ledgers?offset=4&limit=4'
   ```
8. Click `identity-card-link-<alice_id>`. On the identity page read the summary
   card, then the Ledger card, which draws one line per event: click
   `event-expand-3` to open the head event. Then page the events on the route,
   which is where `since` and `limit` live now:
   ```sh
   curl -fsS 'http://127.0.0.1:9080/api/ledgers/'"$alice_id"'/events?since=2&limit=1'
   ```
9. Read the Forks card on the same page: it holds the records of this ledger
   and no other. Open `http://127.0.0.1:9080/witness/ledgers/<org_id>`, a
   ledger with no fork record, which draws no Forks card at all.
10. Try to write. Record the store first, so the "nothing changed" claim is
    checked rather than asserted, then send three requests the witness must
    refuse:
    ```sh
    curl -fsS 'http://127.0.0.1:9080/api/ledgers?offset=0&limit=256' > /tmp/before.json
    curl -i -X POST -H 'Origin: http://127.0.0.1:9080' \
      -H 'Content-Type: application/json' --data '{}' \
      http://127.0.0.1:9080/api/ledgers
    curl -i -X POST -H 'Origin: http://127.0.0.1:9080' \
      -H 'Content-Type: application/json' --data '{}' \
      http://127.0.0.1:9080/api/trust
    curl -i -H 'Host: evil.example' http://127.0.0.1:9080/api/node
    curl -fsS 'http://127.0.0.1:9080/api/ledgers?offset=0&limit=256' > /tmp/after.json
    ```
11. Tear down what story 004 left running, then the topology:
    ```sh
    docker rm -f mabel-alice-two mabel-witness-two
    docker volume rm mabel-alice-second
    dc down -v
    ```

## Verified outcomes

- Step 3: the node document answers `role: "witness"`, `relay: "disabled"`,
  `endpoint_id == witness_id`, `ledger_count: 5`, `fork_count: 1` and
  `storage_capacity: 2147483648`.
- Step 4: `witness-read-only-note` reads `This page only reads. Nothing here
  changes anything.` and `witness-holdings-note` reads `This is what this one
  witness holds. A record missing here may still be on another witness.` There
  is no global discovery and no "who trusts B" query (flag D).
- Step 4: five `identity-card-link-*` elements are present, their ledger ids in
  ascending order, and that order is the order `GET
  /api/ledgers?offset=0&limit=256` answers in.
- Step 5: `identity-card-declared-kind-<org_id>` reads `organization` and the
  four person cards read `person`. Which card falls where follows from the
  digest order, so the assertion is per card.
- Step 6: `identity-card-fork-count-<alice_id>` reads `1 conflict`, and it is
  the only `identity-card-fork-count-*` element on the page.
- Step 7: the first request answers `offset: 0`, `limit: 4`, `more: true` and
  four entries; the second answers `offset: 4`, `more: false` and one entry.
  The two pages together name every ledger exactly once, in the same ascending
  order the cards are drawn in.
- Step 8's summary: `witness-detail-ledger-id` carries `alice_id`,
  `witness-detail-declared-kind` reads `person`, `witness-detail-head-seq`
  reads `3`, `witness-detail-event-count` reads `4`,
  `witness-detail-fork-count` reads `1`, and `witness-detail-witnesses` lists
  two endpoint ids, witness one and witness two, because that is what alice's
  chain says. `witness-detail-source-endpoint` carries the endpoint that
  pushed, which is provenance, not authorization.
  Proposal 005 removed the declared-kind advisory sentence outright, so
  `witness-detail-declared-kind-note` is absent from the page, and
  `witness-detail-holdings-note` repeats the holdings sentence.
- Step 8's chain: `ledger-event-count` reads `4`, `ledger-head-seq` reads `3`,
  and four `ledger-event-*` rows are drawn whose `event-payload-kind-*` values
  are, in order, `inception`, `witness_config`, `witness_config`,
  `trust_attestation`. Opening `event-expand-3` shows `event-detail-3`, whose
  `event-id-3` carries the `entry.head_event` the ledger route reports. The
  wallet's ledger and a witness's copy of it render through the same component,
  because the chain is the same chain.
- Step 8's event page answers `since: 2`, `limit: 1`, `more: true` and one
  event whose `seq` is 2: `since` is inclusive.
- Step 9: `witness-forks` is present with exactly one `fork-record-*` element,
  `fork-record-<alice_id>-3`. On `<org_id>`'s page `witness-forks` is absent
  and `GET /api/forks?ledger_id=<org_id>` answers `entries: []`: a ledger with
  no fork record has nothing to say.
- Step 10, first request: HTTP 405, body `{"ok": false, "code": 2, ...}` with
  `message` exactly `POST is not allowed on /api/ledgers` and
  `details.reason == "method_not_allowed"`.
- Step 10, second request: HTTP 404 with `message` exactly `no route for POST
  /api/trust` and `details.reason == "unknown_route"`. The wallet's mutating
  routes do not exist on a witness.
- Step 10, third request: HTTP 403 with `code: 2`, `details.reason ==
  "host_not_loopback"` and `message` exactly `request rejected: Host header
  must be 127.0.0.1:9080 or localhost:9080`.
- Nothing in step 10 changed the store. Every mutating request answered 405 or
  404, and `/tmp/after.json` holds the same five entries as `/tmp/before.json`:
  the same `head_seq`, `head_event`, `event_count` and `fork_count` per ledger.
  `GET /api/forks` still answers one record with the same `kept.event_id` and
  `conflicting.event_id`.

## Deviations

Where `tests/e2e/specs/005-witness-operator.spec.ts` departs from or exceeds
the story text above.

- Step 1 runs story 004 steps 1 to 7 only when the state they leave is
  missing. The suite runs story 004 first, so the usual path inherits its
  containers; running this spec on its own rebuilds them.
- Step 8 asserts the two summary rows by value, which the story states in
  words. `witness-detail-witnesses` holds exactly `witness_id` and
  `witness_two_id`, and `witness-detail-source-endpoint` holds alice's node
  endpoint id: the second machine's push of the conflicting branch was
  rejected, so the endpoint that stored this ledger is alice's own.
- Steps 3, 7, 8 and 10 read the API through `apiGet` rather than through `curl`
  and `/tmp/*.json` files. The three refused requests of step 10 are the
  story's `curl` commands, because a refusal is about headers and status codes.
- The spec also waits on the containers the story does not name:
  `identity-cards`, `witness-ledger-detail` and `ledger-events`.
- Step 8 counts the chain's rows as `li[data-testid^="ledger-event-"]` under
  `ledger-events`. Proposal 005 draws the ledger as compact rows rather than a
  table, so a line is a list item; the wallet's own ledger and this witness's
  copy still render through the one component.
- `forks_truncated` is asserted nowhere on the screen. The redesigned route
  draws the flag in `witness-detail-fork-count`'s sentence only when a witness
  stopped recording, which this witness did not, so the flag is pinned on the
  ledger route in story 004 instead.

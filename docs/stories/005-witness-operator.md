# 005: the witness operator

- Status: implemented
- Surfaces: witness UI, CLI, witness HTTP API
- Test: `tests/e2e/specs/005-witness-operator.spec.ts`

Someone runs a witness and wants to know what it holds. The debug route
enumerates one witness's own store, pages through it, names the fork it
recorded, and issues nothing but reads.

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
2. Give the witness enough ledgers to page. Carol and dave exist in alice's
   home from story 004 but were never pushed:
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
3. Open `http://127.0.0.1:9080/witness`. Read the Node card.
4. Read the Ledgers card. Four rows are shown, the first four ledger ids in
   ascending order, because the page size is 4 and the list orders by ledger
   id so paging is stable. Ledger ids are digests, so which four they are is
   whatever the ordering says: read the row ids from the DOM and assert per
   page, never "the org is on page one".
5. Click `witness-ledger-next`, then `witness-ledger-previous`.
6. Read the declared kind of each visible row on both pages, and the note under
   the table.
7. Read the fork count of each visible row on both pages.
8. Click `witness-ledger-link-<alice_id>`, from whichever page holds it. On the
   detail page read the summary
   card, then set `witness-events-since` to `2`, `witness-events-limit` to `1`
   and click `witness-events-load`.
9. Read the Forks card on the same page: it is filtered to this ledger.
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

- Step 3: `witness-read-only-note` reads `every request this route issues is a
  read`. `witness-node-role` reads `witness`, `witness-node-relay` reads
  `disabled`, `witness-node-endpoint-id` carries `witness_id`,
  `witness-node-ledger-count` reads `5`, `witness-node-fork-count` reads `1`,
  and `witness-node-storage-capacity` reads `2147483648`.
- Step 4: `witness-ledger-offset` reads `offset 0`, `witness-ledger-limit`
  reads `limit 4`, `witness-ledger-more` reads `more true`,
  `witness-ledger-previous` is disabled, and exactly four
  `witness-ledger-row-*` elements are present, in ascending ledger id order.
- `witness-holdings-note` reads `this is what this one witness holds, a
  diagnostic and not an index: a ledger missing here may still exist on another
  witness`. There is no global discovery and no "who trusts B" query (flag D).
- Step 5: after Next, `witness-ledger-offset` reads `offset 4`, one row is
  shown, `witness-ledger-more` reads `more false`, `witness-ledger-next` is
  disabled and `witness-ledger-previous` is enabled. After Previous, the first
  page returns unchanged.
- Step 6, across both pages: `witness-ledger-declared-kind-<org_id>` reads
  `organization` and the four person rows read `person`. Which page each falls
  on follows from the digest order, so the assertion is per visible row.
  `witness-ledger-declared-kind-note` reads `declared kind is advisory: it
  gates no authorization, no payload validity and no verification outcome`.
- Step 7, across both pages: `witness-ledger-fork-count-<alice_id>` reads `1`,
  every other row reads `0`, and every `witness-ledger-forks-truncated-*`
  visible reads `forks_truncated false`.
- Step 8's summary: `witness-detail-head-seq` reads `3`,
  `witness-detail-event-count` reads `4`, `witness-detail-fork-count` reads
  `1`, `witness-detail-forks-truncated` reads `false`, and
  `witness-detail-witnesses` lists two endpoint ids, witness one and witness
  two, because that is what alice's chain says. `witness-detail-source-endpoint`
  carries the endpoint that pushed, which is provenance, not authorization.
- Step 8's events: `witness-events-page-since` reads `2`,
  `witness-events-page-limit` reads `1`, `witness-events-more` reads `true`,
  and exactly one row, `witness-event-2`, is shown:
  `since` is inclusive. Loading with `since` 0 and `limit` 8 shows four rows
  whose `witness-event-payload-kind-*` values are, in order, `inception`,
  `witness_config`, `witness_config`, `trust_attestation`.
- Step 9: `witness-forks-filter` is present and names `alice_id`, one record
  `fork-record-<alice_id>-3` is shown, and `witness-forks-more` reads `more
  false`. Filtering to a ledger with no fork
  (`http://127.0.0.1:9080/witness/ledgers/<org_id>`) shows
  `witness-forks-empty` reading `this witness recorded no fork for this
  ledger`.
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

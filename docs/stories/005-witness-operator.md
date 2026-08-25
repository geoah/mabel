# 005: the witness operator

- Status: implemented
- Surfaces: the node UI, CLI, the node HTTP API
- Test: `tests/e2e/specs/005-witness-operator.spec.ts`

Someone runs a witness and wants to know what it holds. There is no witness
screen and no witness route: one node serves one API, so its holdings are the
"Known identities" section of the same home every node draws, and one record is
the same identity page a wallet draws (proposal 006 section 8). This story reads
what that surface says, checks the conflict on the route that reports it, and
proves an old home is refused rather than misread.

## Actors

- witness one: compose service `witness`, API and UI on
  `http://127.0.0.1:9080`. The node being inspected. It holds one identity, the
  witness identity it minted on its first start, and keeps other people's
  records under it.
- alice: a node holding alice's key, compose service `alice`, the source of
  every record the witness keeps.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root. This story starts from story 004, so it inherits story 004's
second witness (a service of `docker/compose.two-witnesses.yaml`) and its
hand-started second machine, and step 12 tears both down.

## Story

1. Run story 004 steps 1 to 7, stopping before its teardown. Witness one now
   keeps alice's record at seq 3 and has recorded one conflict for it. Keep
   `alice_id`, `witness_identity`, `witness_id` and `witness_two_identity`.
2. Give the witness five records to keep. Carol and dave exist in alice's home
   from story 004 but were never pushed:
   ```sh
   dc exec -T alice mabel identity create --alias erin --kind person
   dc exec -T alice mabel identity create --alias mabel-demo-co \
     --kind organization --founder alice
   for name in carol dave erin mabel-demo-co; do
     dc exec -T alice mabel witness add --identity "$name" --witness "$witness_identity"
     dc exec -T alice sh -c 'mabel sync push --identity '"$name"' \
       --peer "$(cat /shared/witness.ticket)"'
   done
   ```
   Record `org_id`. The witness now keeps five records, four `person` and one
   `organization`, none of which it can sign for.
3. Read the node facts, on the route and on the page that draws them:
   `curl -fsS http://127.0.0.1:9080/api/node`. Then open
   `http://127.0.0.1:9080/wallet` and click `nav-node`. The nav is the same
   three entries every node serves, `nav-wallet`, `nav-witnesses` and
   `nav-node`: a node that keeps other people's records is not a different
   program, and `nav-witness` does not exist.
4. Open `http://127.0.0.1:9080/wallet`. `identity-cards` holds one card, the
   witness identity this home minted and signs for. Everything else it holds is
   a known identity, a record it has and does not control, so
   `known-identity-cards` holds five, under the standing note
   `known-identities-note`. Record ids are digests, so which card is where is
   whatever the ordering says: read the ids out of the DOM and assert per card,
   never "the org is first".
5. Read the declared kind of each known row on `GET /api/identities/known`.
6. Page that route. Paging is how a home with up to 10000 records is read, and
   the order is what makes it stable:
   ```sh
   curl -fsS 'http://127.0.0.1:9080/api/identities/known?offset=0&limit=4'
   curl -fsS 'http://127.0.0.1:9080/api/identities/known?offset=4&limit=4'
   ```
7. Click `identity-card-link-<alice_id>` in the known list. It opens
   `/identities/<alice_id>`, the same identity page a wallet draws, because a
   record is a record. Read the card, then the Ledger card, which draws one line
   per event: a closed line carries the position and a plain gloss, so click
   `event-expand-<seq>` to read the raw kind, the entry id and the payload. Then
   page the events on the route, which is where `since` and `limit` live:
   ```sh
   curl -fsS 'http://127.0.0.1:9080/api/identities/'"$alice_id"'/ledger?since=2&limit=1'
   ```
8. Read the conflict. `GET /api/forks` is the one route that reports one, on
   every node:
   ```sh
   curl -fsS 'http://127.0.0.1:9080/api/forks?ledger_id='"$alice_id"
   curl -fsS 'http://127.0.0.1:9080/api/forks?ledger_id='"$org_id"
   curl -fsS 'http://127.0.0.1:9080/api/forks?offset=0&limit=64'
   ```
9. Ask for the routes the witness screens used. Record the store first, so the
   "nothing changed" claim is checked rather than asserted:
    ```sh
    curl -fsS 'http://127.0.0.1:9080/api/identities/known?offset=0&limit=256' > /tmp/before.json
    curl -i http://127.0.0.1:9080/api/ledgers
    curl -i -X POST -H 'Origin: http://127.0.0.1:9080' \
      -H 'Content-Type: application/json' --data '{}' \
      http://127.0.0.1:9080/api/ledgers
    curl -i -H 'Host: evil.example' http://127.0.0.1:9080/api/node
    curl -fsS 'http://127.0.0.1:9080/api/identities/known?offset=0&limit=256' > /tmp/after.json
    ```
10. Start a container on a home written before witnesses were identities. Its
    `node.json` carries a `role` line and a 64-character hex endpoint id under
    `witnesses`, which is the shape proposal 006 section 5.4 replaced:
    ```sh
    docker volume create mabel-legacy-home
    docker run --rm --volume mabel-legacy-home:/data --entrypoint sh mabel:dev -c \
      'mabel node id >/dev/null && printf "%s\n" "{\"role\":\"witness\",\"witnesses\":[\"<64 hex>\"]}" > /data/node.json'
    docker run --rm --volume mabel-legacy-home:/data --entrypoint mabel mabel:dev \
      serve --http 127.0.0.1:9099 --iroh-port 9098
    ```
    That second command starts past the entrypoint, so nothing rewrites the file
    first, and it fails to load.
11. Start the same volume through the image's entrypoint, which writes
    `node.json` on every start before anything loads it:
    ```sh
    docker run --rm --volume mabel-legacy-home:/data mabel:dev node id
    docker run --rm --volume mabel-legacy-home:/data --entrypoint cat mabel:dev /data/node.json
    docker volume rm -f mabel-legacy-home
    ```
12. Tear down what story 004 left running, then the topology:
    ```sh
    docker rm -f mabel-alice-two
    docker volume rm mabel-alice-second
    dc -f docker/compose.two-witnesses.yaml down -v
    ```

## Verified outcomes

- Step 3: the node document carries no `role` key at all, and answers `relay:
  "disabled"`, `endpoint_id == witness_id`, `identity_count: 1`,
  `ledger_count: 6`, `fork_count: 1` and `storage_capacity: 2147483648`. Six
  records: the five it keeps for other people and its own.
- Step 3's page says the same in short rows. There is no `node-role` element:
  what a node can do is read from what it holds. `node-relay` reads `direct
  connections only`, `node-endpoint-id` carries `witness_id` under the label
  `Iroh ID` and is not truncated, `node-identity-count` reads `1` under the
  label `identities`, `node-witness-for` holds one inline identity,
  `node-witness-for-<witness_identity>`, under the label `keeps records for`,
  `node-ledger-count` reads `6` under `records`, `node-fork-count` reads `1`
  under `conflicts`, `node-storage` ends `of 2.1 GB` and `node-version` repeats
  the document's own value. This home holds a key, so `node-no-keys` is absent.
  Round 5 of proposal 005 dropped `node-http-bind` from the page.
- Step 4: `identity-cards` holds exactly `identity-card-<witness_identity>`,
  and `known-identity-cards` holds five links whose ids are the ids `GET
  /api/identities/known?offset=0&limit=256` answers, in the same order.
  `known-identities-note` reads `This is what this home holds. A record missing
  here may still be on another witness.`, the sentence that came off the witness
  route with it (proposal 006 section 8). There is no global discovery and no
  "who trusts B" query (flag D). The witness identity is not a known row: a
  home's holdings are what it stores and cannot sign for.
- Step 5: the row for `org_id` reads `declared_kind: "organization"`, the four
  others read `"person"`, and every row reads `stored: true`.
- Step 6: the first request answers `offset: 0`, `limit: 4`, `more: true` and
  four rows; the second answers `offset: 4`, `more: false` and one row. The two
  pages together name every record exactly once, in the order the route sorts
  by, which is the rendered id: the digits sort before the letters.
- Step 7's page: `identity-detail-resolved` carries `alice_id`,
  `identity-detail-declared-kind` reads `person` and
  `identity-detail-event-count` reads `4`. This home holds no key for alice, so
  `identity-actions` is absent from the page entirely. `GET
  /api/identities/<alice_id>` answers `head_seq: 3`, `event_count: 4` and
  `witnesses` holding `witness_identity` and `witness_two_identity`, because
  that is what alice's chain says.
- Step 7's chain: `ledger-event-count` reads `4` and four `ledger-event-*` rows
  are drawn. A closed line carries `event-seq-*` and `event-gloss-*` only,
  reading in order `created this identity`, `chose who keeps a copy`, `chose who
  keeps a copy` and `said it trusts someone`, with no `event-payload-kind-*`
  element on the page at all. Opening `event-expand-<seq>` shows
  `event-detail-<seq>`, and `event-payload-kind-*` then reads `inception`,
  `witness_set`, `witness_set` and `trust_attestation` in the same order: a
  witness set is tag 19 and names identities. `event-id-3` in the open head
  entry carries the `head_event` the identity route reports.
- Step 7's event page answers `since: 2`, `limit: 1`, `more: true` and one
  event whose `seq` is 2: `since` is inclusive.
- Step 8: `?ledger_id=<alice_id>` answers one entry at `seq: 3` whose
  `source_endpoint` is not alice's own node, because the branch witness one
  refused came from her second machine. `?ledger_id=<org_id>` answers `entries:
  []`, and the unfiltered page answers one entry with `more: false`.
- Step 9: both requests to `/api/ledgers` answer HTTP 404 with `code: 2`,
  `details.reason == "unknown_route"` and `message` exactly `no route for GET
  /api/ledgers` and `no route for POST /api/ledgers`. That path was the
  witness's own read-only route, and one node serves one API now, so it is not a
  route at all for any method.
- Step 9's third request answers HTTP 403 with `code: 2`, `details.reason ==
  "host_not_loopback"` and `message` exactly `request rejected: Host header
  must be 127.0.0.1:9080 or localhost:9080`.
- Nothing in step 9 changed the store: `/tmp/after.json` holds the same five
  rows as `/tmp/before.json`, with the same `head_seq` and `declared_kind` each,
  and `GET /api/forks` still answers one record with the same `kept.event_id`
  and `conflicting.event_id`.
- Step 10 exits 10 and says what to run: `Schema error: node.json is not valid:
  node.json names the endpoint id <64 hex> under witnesses, which proposal 006
  replaced with {"identity", "endpoints"} objects; run mabel witness set-default
  --witness <mabel-id> --endpoints <endpoint,...>`. A hex endpoint id is 64
  characters and a base32 identity id is 52, so the loader tells the two apart
  and refuses rather than configuring a witness that is not one.
- Step 11 exits 0, and the rewritten `node.json` carries no `role` key, no
  `accept_legacy_witness_config` key, `witnesses: []` and `witness_for: []`.
  `mabel node id` reads `node.key` and not `node.json`, so it runs before the
  rewrite on a volume the old file would otherwise stop.

## Deviations

Where `tests/e2e/specs/005-witness-operator.spec.ts` departs from or exceeds
the story text above.

- Step 1 runs story 004 steps 1 to 7 only when the state they leave is
  missing. The suite runs story 004 first, so the usual path inherits its
  containers; running this spec on its own rebuilds them.
- Steps 3 and 5 to 9 read the API through `apiGet` rather than through `curl`
  and `/tmp/*.json` files. The three refused requests of step 9 are the story's
  `curl` commands, because a refusal is about headers and status codes.
- Step 7 counts the chain's rows as `li[data-testid^="ledger-event-"]` under
  `ledger-events`. Proposal 005 draws the ledger as compact rows rather than a
  table, so a line is a list item.
- Step 7 opens all four lines rather than only the head, and closes the first
  three again. The final round of proposal 005 moved the raw kind string into
  the opened entry, so reading four kinds means four clicks; the head entry is
  left open, which is the one state the story asks for.
- Step 10 writes the whole `node.json` the old shape had, five keys, rather
  than the two the story abbreviates to.

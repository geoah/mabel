# 008: a link with no witness

- Status: implemented
- Surfaces: wallet UI (bob and a borrowed home), CLI, the node HTTP API
- Test: `tests/e2e/specs/008-link-with-no-witness.spec.ts`

Bob publishes the machine that answers for him on his own record, hands over a
`mabel://` link, and somebody who has never heard of him reads that record with
every witness container stopped. An identity is reachable with no witness in the
topology at all (proposal 006 section 2).

The link carries who and which machine. It does not carry a route to that
machine, and never will: an address is not on any ledger. So the ticket travels
beside it, out of band, the way the first address always does (proposal 006
section 5.4). Both halves are handed over by the same person in the same
message; neither is authorization.

## Actors

- bob: a node holding bob's key, compose service `bob`, API and UI on
  `http://127.0.0.1:9082`. The identity being reached.
- the witness: compose service `witness`, stopped in step 4 and never started
  again in this story.
- dana: a borrowed home, container `mabel-dana` on the compose network, API and
  UI on `http://127.0.0.1:9085`. It holds no key, knows nobody, and configures
  no witness. The one thing it is told is how to route to bob's machine.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root.

## Story

1. `dc down -v && dc up -d --wait`. In bob's UI at
   `http://127.0.0.1:9082/wallet`, create an identity with alias `bob` and kind
   `person`. He names no witness and pushes nothing: this story is about an
   identity no witness keeps a copy of. `bob_node="$(dc exec -T bob mabel node
   id)"` is the machine his home runs on.
2. Publish that machine on bob's own record. Open
   `identity-card-link-<bob_id>`, then `action-endpoints-summary` under the
   group `action-group-reach`, headed `Reaching this identity`.
   `endpoints-empty` reads `This identity's record names no machine yet.` Click
   `endpoints-use-this-node`, which fills `endpoints-input` with `bob_node`,
   then `endpoints-submit`. The first machine this home publishes asks for
   consent first: `endpoints-consent` states the three facts publishing one
   costs, and its confirm button reads `Publish the machine`. Click it.
3. Make the link. Open `action-share-summary`, beside the endpoints action in
   the same group. `share-panel` holds the link with a copy control,
   `share-machine-count` saying how many machines it names, `share-qr` holding
   the same string as a square to scan, `share-download` offering it as a file,
   and `share-disclosure` saying what handing it over gives away. The CLI builds
   the same string from the same record, and `mabel node ticket` prints the
   address to go with it:
   ```sh
   dc exec -T bob mabel identity share bob --json
   dc exec -T bob mabel node ticket --port 9072
   ```
4. Stop every witness and start a home that has none: `dc stop witness`. Then
   ```sh
   docker volume create mabel-dana-home
   docker run -d --name mabel-dana --network mabel_mabel \
     --volume mabel-dana-home:/data \
     --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9085 --env MABEL_IROH_PORT=9075 \
     --publish 9085:9085 \
     mabel:dev serve --http 0.0.0.0:9085 --iroh-port 9075 --peer "$bob_ticket"
   until curl -fsS http://127.0.0.1:9085/api/node >/dev/null; do sleep 1; done
   ```
   No `MABEL_WAIT_FOR_TICKET`, so this home configures no witness at all.
5. Open `http://127.0.0.1:9085/wallet` and paste the link into
   `wallet-search-input`, then click `wallet-search-submit`. The browser parses
   no link: the box hands the string to the node, which owns the grammar
   (proposal 006 section 7).
6. Fetch. Click `identity-fetch-button`.
7. Tear down: `docker rm -f mabel-dana && docker volume rm mabel-dana-home`,
   then `dc down -v`.

## Verified outcomes

- Step 1: `GET http://127.0.0.1:9082/api/identities/<bob_id>` answers
  `witnesses: []` and `endpoints: []`.
- Step 2's consent panel carries all three sentences, in this order: `The
  machine's id stays readable forever by anyone who can name this identity.`,
  `Anyone who reads it can dial that machine directly, which shows the machine's
  address to them and to the relay that connects them.`, and `Once this home
  answers at a published address, anyone who dials it can list the identities it
  signs for and, if it keeps records for other people, the records it keeps.`
- Step 2 appends one entry and nothing else. `endpoints-head-seq` reads `Saved
  at position 1.`, `endpoints-list` holds `bob_node`, `GET
  /api/identities/<bob_id>` answers `endpoints == [bob_node]` and `head_seq: 1`,
  and `GET /api/identities/<bob_id>/ledger?since=1&limit=1` answers one event
  with `payload_kind: "endpoint_advertisement"` and payload `{"endpoints":
  ["<bob_node>"]}`. On the screen that entry's closed line reads `published the
  machines that answer for it`.
- Step 3: the link is exactly `mabel://<bob_id>?endpoints=<bob_node>`,
  `share-machine-count` reads `The link names 1 machine.`, `share-download`
  carries `download="<first 8 characters of bob_id>.mabel"`, and
  `share-disclosure` holds the three sentences of proposal 006 section 7. `mabel
  identity share bob --json` answers the same `link`, `endpoints ==
  [bob_node]` and `endpoints_from: "advertised"`, because the record now names
  a machine and `auto` reads the record first.
- Step 4: `mabel-witness` is not running and
  `http://127.0.0.1:9080/api/node` answers nothing. Dana's `GET /api/node`
  answers `identity_count: 0`, `ledger_count: 0`, `witness_for: []` and
  `witnesses: []`, `GET /api/witnesses` answers an empty list, and its node page
  says so in one sentence: `node-no-keys` reads `This home holds no keys, so it
  signs for nothing and adds nothing to any record. It keeps 0 records.`, with
  `node-witness-for` reading `none` and `node-witnesses-empty` reading `none`.
- Step 5: `GET /api/resolve?input=<the link>` on dana answers `input_kind:
  "link"`, `identity_id == bob_id` and `endpoints == [bob_node]`. The box lands
  on `/identities/<bob_id>?machines=<bob_node>`, where `identity-fetch` offers
  the one action and `identity-fetch-link-note` reads `This link names the
  machines to ask for this record. Asking them tells those machines this home's
  network address and which identity it is looking for.` The section's own
  description reads `Asks the machines the link named, in order, and keeps what
  they send.`
- Step 6 lands with no witness in the topology. The page draws `ledger-panel`,
  `identity-fetch` is gone, `identity-detail-event-count` reads `2`, and `GET
  /api/identities/<bob_id>` on dana answers `head_seq: 1`, `event_count: 2`,
  `endpoints == [bob_node]` and `witnesses: []`. The record was verified from
  nothing the way any other source's copy is (proposal 001 section 3.7).
- After step 6 dana's `GET /api/node` answers `ledger_count: 1` and
  `identity_count: 0`, and `GET /api/witnesses` is still empty: storing a record
  is not controlling it, so the page carries no `identity-actions`.

## Deviations

Where `tests/e2e/specs/008-link-with-no-witness.spec.ts` departs from or
exceeds the story text above.

- The spec reads the link out of `share-panel`'s identifier rather than off the
  clipboard: a copy control is pinned by `ui/src/test/identifier.test.tsx`,
  which can hold the two-second confirmation clock still.
- The spec asserts container testids the story does not name, because the
  shared UI helpers wait on them: `identity-detail`, `identity-cards` and
  `wallet-search`.
- Step 4's wait is `expect.poll` on `GET /api/node` rather than a shell loop,
  and the spec asserts `mabel-witness` is not running through `docker inspect`.

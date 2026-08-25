# 009: a witness moves machines

- Status: implemented
- Surfaces: CLI, wallet UI, the node HTTP API
- Test: `tests/e2e/specs/009-endpoint-rotation.spec.ts`

A witness identity moves from one machine to another through the four steps of
proposal 006 section 5.5, and a client that was never handed the out-of-band
update reaches nothing once the old machine stops. It cannot learn the new
machine from inside mabel, because the only copy of the new advertisement sits
on a machine it cannot dial. Recovery is a fresh record, handed over the way the
first one was.

Every ledger event that named this witness still stands: a witness is named by
its Mabel id, not by the machine behind it (proposal 006 section 1). What has to
move is the reachability, and step 3 of section 5.5 is out of band by
construction, because none of those records is on a ledger.

## Actors

- the witness: compose service `witness`, API and UI on
  `http://127.0.0.1:9080`. `witness_identity` is the Mabel id it witnesses for
  and holds the keys of; `old_machine` is the container answering for it now.
- the new machine: container `mabel-witness-new`, API on
  `http://127.0.0.1:9086`, joining the fleet. It answers for the same witness
  identity and holds none of its keys, which is what a fleet is (proposal 006
  section 5.4).
- carla: the stale client, container `mabel-carla`, API and UI on
  `http://127.0.0.1:9087`. Its one bootstrap record is the ticket the witness
  published before the rotation.
- alice: a node holding alice's key, compose service `alice`, whose record the
  witness keeps.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root.

## Story

1. `dc down -v && dc up -d --wait`. Read `witness_identity` and `old_machine`
   from `/shared/witness.identity` and `/shared/witness.id`. Alice creates an
   identity, names the witness identity on her chain and pushes:
   ```sh
   dc exec -T alice mabel identity create --alias alice --kind person
   dc exec -T alice mabel witness add --identity alice --witness "$witness_identity"
   dc exec -T alice sh -c 'mabel sync push --identity alice \
     --peer "$(cat /shared/witness.ticket)"'
   ```
2. Start carla, with the witness's ticket as its only bootstrap record, and let
   it take its own copy of the witness's record:
   ```sh
   docker volume create mabel-carla-home
   docker run -d --name mabel-carla --network mabel_mabel \
     --volume mabel-carla-home:/data --volume mabel_witness-ticket:/shared:ro \
     --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9087 --env MABEL_IROH_PORT=9077 \
     --env MABEL_WAIT_FOR_TICKET=/shared/witness \
     --publish 9087:9087 \
     mabel:dev serve --http 0.0.0.0:9087 --iroh-port 9077
   docker exec mabel-carla sh -c 'mabel sync fetch '"$witness_identity"' \
     --from '"$old_machine"' --peer "$(cat /shared/witness.ticket)"'
   docker exec mabel-carla sh -c 'mabel sync fetch '"$alice_id"' \
     --from-witness '"$witness_identity"' --peer "$(cat /shared/witness.ticket)"'
   ```
   The entrypoint read `/shared/witness.identity` beside `/shared/witness.id`
   and ran `mabel witness set-default`, so carla's `node.json` names the witness
   identity and the one machine that answers for it.
3. Section 5.5 step 1: bring the new machine up and read its endpoint id. It is
   told where the machine already in the fleet is, and publishes a ticket of its
   own, which is the record step 3 of section 5.5 has to hand over:
   ```sh
   docker volume create mabel-witness-new-home
   docker run -d --name mabel-witness-new --network mabel_mabel \
     --volume mabel-witness-new-home:/data --volume mabel_witness-ticket:/shared \
     --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9086 --env MABEL_IROH_PORT=9076 \
     --env MABEL_WAIT_FOR_TICKET=/shared/witness \
     --env MABEL_PUBLISH_TICKET=/shared/witness-new \
     --publish 9086:9086 \
     mabel:dev serve --http 0.0.0.0:9086 --iroh-port 9076
   new_machine="$(docker exec mabel-witness-new mabel node id)"
   new_ticket="$(docker exec mabel-witness-new cat /shared/witness-new.ticket)"
   curl -sS -X POST -H 'Origin: http://127.0.0.1:9086' -H 'Content-Type: application/json' \
     --data '{"from":"'"$old_machine"'"}' \
     "http://127.0.0.1:9086/api/identities/$witness_identity/fetch"
   curl -sS -X POST -H 'Origin: http://127.0.0.1:9086' -H 'Content-Type: application/json' \
     --data '{"from":"'"$old_machine"'"}' \
     "http://127.0.0.1:9086/api/identities/$alice_id/fetch"
   ```
   The fetches go through the new machine's own node rather than its CLI: a node
   serves what its own process wrote, so a copy written behind its back would
   not be served over the sync protocol until it restarted.
4. Section 5.5 step 2: a controller of the witness identity appends one
   advertisement naming **both** machines. Whole replacement means the old one
   has to be repeated here or it is dropped in this step:
   ```sh
   curl -sS -X POST -H 'Origin: http://127.0.0.1:9080' -H 'Content-Type: application/json' \
     --data '{"endpoints":["'"$old_machine"'","'"$new_machine"'"]}' \
     "http://127.0.0.1:9080/api/identities/$witness_identity/endpoints"
   ```
5. Section 5.5 step 3: update every bootstrap record that names the witness.
   This story does not: carla is the client nobody told. Its `node.json` still
   names the old machine alone, and it still holds the advertisement from before
   step 2.
6. Section 5.5 step 4: once readers have had a chance to fetch step 2, a second
   advertisement names the new machine alone. The new machine takes that copy
   while the old one is still up, and then the old one stops:
   ```sh
   curl -sS -X POST -H 'Origin: http://127.0.0.1:9080' -H 'Content-Type: application/json' \
     --data '{"endpoints":["'"$new_machine"'"]}' \
     "http://127.0.0.1:9080/api/identities/$witness_identity/endpoints"
   curl -sS -X POST -H 'Origin: http://127.0.0.1:9086' -H 'Content-Type: application/json' \
     --data '{"from":"'"$old_machine"'"}' \
     "http://127.0.0.1:9086/api/identities/$witness_identity/fetch"
   dc stop witness
   ```
7. Carla reaches nothing:
   ```sh
   docker exec mabel-carla mabel sync fetch "$witness_identity" \
     --from-witness "$witness_identity" --json
   docker exec mabel-carla mabel sync fetch "$alice_id" \
     --from-witness "$witness_identity" --json
   ```
8. Recovery: hand carla the new ticket, out of band, the way the first one was
   handed over, and let it read the record that names the new machine:
   ```sh
   docker exec mabel-carla mabel sync fetch "$witness_identity" \
     --from "$new_machine" --peer "$new_ticket" --json
   docker exec mabel-carla mabel sync fetch "$alice_id" \
     --from-witness "$witness_identity" --peer "$new_ticket" --json
   ```
9. Read carla's screens: open `http://127.0.0.1:9087/identities/<witness_identity>`.
10. Tear down: `docker rm -f mabel-carla mabel-witness-new`, `docker volume rm
    mabel-carla-home mabel-witness-new-home`, then `dc down -v`.

## Verified outcomes

- Step 1: `GET http://127.0.0.1:9080/api/identities/<witness_identity>` answers
  `endpoints == [old_machine]` and `head_seq: 1`, and `GET /api/node` on the
  witness answers `witness_for` holding `{identity: <witness_identity>,
  advertised: true, reason: null}`: the container mints the identity and
  publishes itself on it on its first start.
- Step 2: `GET http://127.0.0.1:9087/api/witnesses` on carla answers one
  witness, `identity_id == witness_identity`, with `endpoints` naming
  `old_machine` alone. Its own copy answers `endpoints == [old_machine]` at
  `head_seq: 1`, and the fetch of alice's record reports `source ==
  old_machine`: naming the witness identity is enough while that machine is up.
- Step 3: `new_machine != old_machine`, and `GET
  http://127.0.0.1:9086/api/identities/<alice_id>` answers `head_seq: 1`.
- Step 4 answers 200 with `head_seq: 2` and an event whose `payload_kind` is
  `endpoint_advertisement` and whose payload is `{"endpoints":
  ["<old_machine>","<new_machine>"]}`. The witness's own copy then answers
  `endpoints == [old_machine, new_machine]`.
- Step 5: carla's `GET /api/witnesses` still names `old_machine` alone and its
  copy of the witness is still at `head_seq: 1` with `endpoints ==
  [old_machine]`. The new ticket exists and this client was not handed it.
- Step 6 answers 200 with `head_seq: 3` and payload `{"endpoints":
  ["<new_machine>"]}`. The witness's own copy answers `endpoints ==
  [new_machine]`, and `GET /api/node` on it now answers `witness_for[0]`
  `advertised: false` with `reason` exactly `that identity's ledger advertises
  other endpoints and not this one`: a machine that no longer answers for the
  identity it witnesses for stops taking records it does not already store
  (proposal 006 section 4.1). The new machine's copy answers `endpoints ==
  [new_machine]` before the old one stops.
- Step 7: both commands exit 30. The failure names `old_machine` and never
  names `new_machine`, because that is the only machine carla's two sources
  know. Nothing changed on disk: its copy of the witness is still at `head_seq:
  1` with `endpoints == [old_machine]`.
- Step 8's first command exits 0 with `source == new_machine` and `head_seq:
  3`, and carla's copy then answers `endpoints == [new_machine]`. The second
  exits 0 with `source == new_machine`: with the new advertisement stored,
  naming the witness identity resolves to the new machine, and the ticket is
  what routes to it.
- Step 9: carla's `GET /api/witnesses` reports both machines and both `hinted`.
  The only chain it ever read for the witness came from the machine that
  advertisement names, and an endpoint that served its own evidence proves
  nothing (proposal 006 section 4.2); somebody other than the new machine would
  have to serve the same chain for it to count as `verified`.
- Step 9's page draws one `machine` row per machine, labelled `machine`:
  `identity-detail-machine-<new_machine>` with
  `identity-detail-machine-<new_machine>-note` reading `This machine is listed
  on this identity's own record.`, and
  `identity-detail-machine-<old_machine>` with its note reading `No record we
  have confirms that this machine answers for it.` The record is what backs a
  machine up; this home's own configuration is not.

## Deviations

Where `tests/e2e/specs/009-endpoint-rotation.spec.ts` departs from or exceeds
the story text above.

- Steps 4 and 6 run through `POST /api/identities/<id>/endpoints` rather than
  `mabel identity endpoints replace`, and step 3's copies through `POST
  /api/identities/<id>/fetch` rather than `mabel sync fetch`. Both do the same
  thing to the same home. A node keeps its folded state in memory and updates it
  when its own process writes, so a CLI append beside a running `mabel serve`
  is not served over the sync protocol until that node restarts, and this story
  needs each machine to serve what was just written to it.
- Carla's own fetches stay on the CLI, because a `--peer` ticket is how a
  process is handed an address and the fetch route takes none. Its screens are
  read on the HTTP API, which loads from disk.
- The spec asserts `identity-detail` before reading the machine rows, the
  container the shared helpers wait on.

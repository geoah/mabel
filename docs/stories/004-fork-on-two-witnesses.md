# 004: the fork

- Status: implemented
- Surfaces: CLI, witness UI, witness HTTP API, wallet HTTP API
- Test: `tests/e2e/specs/004-fork-on-two-witnesses.spec.ts`

One wallet on two machines signs two different events at one sequence. Each
branch reaches a different witness, one witness meets both and records the
evidence, and a verifier that asks both exits 20 naming both sources. Nothing
here forges a signature: both events are valid.

## Actors

- alice: wallet node, compose service `alice`, the first machine holding
  alice's key. API and UI on `http://127.0.0.1:9081`.
- alice's second machine: container `mabel-alice-two`, a byte-for-byte copy of
  alice's home with a fresh node key. It runs CLI commands only.
- witness one: compose service `witness`, API and UI on
  `http://127.0.0.1:9080`. It meets both branches.
- witness two: container `mabel-witness-two`, API and UI on
  `http://127.0.0.1:9083`. It meets one branch.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root. `compose.yaml` defines one witness. An operator brings the
second one up with the `docker/compose.two-witnesses.yaml` overlay (ticket
032); the Playwright spec instead starts it by hand with `docker run` in step
2, because the e2e harness owns the base compose lifecycle and a per-spec
overlay would recreate the shared network under the other specs.

## Story

1. `dc down -v && dc up -d --wait`, then
   `witness_id="$(dc exec -T witness cat /shared/witness.id)"`.
2. Start the second witness, publishing its ticket beside the first one's on
   the shared volume:
   ```sh
   docker run -d --name mabel-witness-two --network mabel_mabel \
     --volume mabel_witness-ticket:/shared \
     --env MABEL_ROLE=witness --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9083 --env MABEL_IROH_PORT=9073 \
     --env MABEL_PUBLISH_TICKET=/shared/witness-two \
     --publish 9083:9083 --publish 9073:9073/udp \
     mabel:dev witness run --http 0.0.0.0:9083 --iroh-port 9073
   ```
   `docker run -d` returns before the node is up, so wait for both signals the
   entrypoint and the API give, then read the id and the ticket:
   ```sh
   until dc exec -T alice test -f /shared/witness-two.ticket; do sleep 1; done
   until curl -fsS http://127.0.0.1:9083/api/node >/dev/null; do sleep 1; done
   witness_two_id="$(dc exec -T alice cat /shared/witness-two.id)"
   witness_two_ticket="$(dc exec -T alice cat /shared/witness-two.ticket)"
   ```
3. Alice creates one identity and two subjects to attest, and names both
   witnesses on her chain:
   ```sh
   dc exec -T alice mabel identity create --alias alice --kind person
   dc exec -T alice mabel identity create --alias carol --kind person
   dc exec -T alice mabel identity create --alias dave --kind person
   dc exec -T alice mabel witness add --identity alice --endpoint "$witness_id"
   dc exec -T alice mabel witness add --identity alice --endpoint "$witness_two_id"
   dc exec -T alice sh -c 'mabel sync push --identity alice \
     --peer "$(cat /shared/witness.ticket)" --peer "$(cat /shared/witness-two.ticket)"'
   ```
   Record `alice_id`, `carol_id` and `dave_id`. Alice's ledger is at seq 2 and
   both witnesses hold it.
4. Copy alice's home to the second machine, dropping `node.json` and `node.key`
   so it gets its own endpoint id and keeps alice's identity keys:
   ```sh
   docker volume create mabel-alice-second
   docker run --rm --user 0 --volumes-from mabel-alice \
     --volume mabel-alice-second:/copy --entrypoint sh mabel:dev \
     -c 'cp -a /data/. /copy/ && rm -f /copy/node.json /copy/node.key'
   docker run -d --name mabel-alice-two --network mabel_mabel \
     --volume mabel-alice-second:/data --volume mabel_witness-ticket:/shared:ro \
     --env MABEL_ROLE=wallet --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9084 --env MABEL_IROH_PORT=9074 \
     --publish 9084:9084 \
     mabel:dev wallet serve --http 0.0.0.0:9084 --iroh-port 9074
   until curl -fsS http://127.0.0.1:9084/api/node >/dev/null; do sleep 1; done
   ```
   The wait matters: `docker run -d` returns before the home is prepared, and
   an `exec` that lands first sees no `node.json`. The published port is host
   port equals container port, as every other node's is, so the loopback rules
   accept a request from the host.
5. Both machines append offline, at the same sequence, on the same previous
   event:
   ```sh
   kept_event="$(dc exec -T alice mabel trust add --issuer alice \
     --subject "$carol_id" --no-sync --json | jq -r .attestation_event)"
   conflicting_event="$(docker exec mabel-alice-two mabel trust add --issuer alice \
     --subject "$dave_id" --no-sync --json | jq -r .attestation_event)"
   ```
   Both documents carry `attestation_seq: 3`. Each machine's `prev` is the
   other's `prev`, which the ledger route on each wallet reports:
   ```sh
   curl -fsS "http://127.0.0.1:9081/api/identities/$alice_id/ledger?since=3" | jq -r .events[0].prev
   curl -fsS "http://127.0.0.1:9084/api/identities/$alice_id/ledger?since=3" | jq -r .events[0].prev
   ```
6. One branch to each witness:
   ```sh
   dc exec -T alice sh -c 'mabel sync push --identity alice --to '"$witness_id"' \
     --peer "$(cat /shared/witness.ticket)"'
   docker exec mabel-alice-two sh -c 'mabel sync push --identity alice --to '"$witness_two_id"' \
     --peer "$(cat /shared/witness-two.ticket)"'
   ```
   Both are accepted, `stored 1` each. Neither witness has seen the other
   event.
7. The second machine also pushes its branch to witness one, which already
   holds a different valid event at seq 3:
   ```sh
   docker exec mabel-alice-two sh -c 'mabel sync push --identity alice --to '"$witness_id"' \
     --peer "$(cat /shared/witness.ticket)" --json'
   ```
8. Open witness one's UI at `http://127.0.0.1:9080/witness`, which is the
   identity card list of what this witness holds. Alice's card carries
   `identity-card-fork-count-<alice_id>` reading `1 fork record`; no other card
   carries that element at all. Click `identity-card-link-<alice_id>` and read
   the summary and the Forks card on the identity page.
9. A fresh verifier asks both witnesses:
   ```sh
   docker run --rm --network mabel_mabel \
     --volume mabel_witness-ticket:/shared:ro \
     --env MABEL_WAIT_FOR_TICKET=/shared/witness \
     mabel:dev verify ledger "$alice_id" --peer "$witness_two_ticket" --json
   ```
   The entrypoint appends witness one's ticket as a second `--peer`, so the
   home that holds nothing knows two places to look and pins neither.
10. Tear the extra containers down:
    `docker rm -f mabel-alice-two mabel-witness-two && docker volume rm mabel-alice-second`,
    then `dc down -v`.

## Verified outcomes

- Step 5: `kept_event != conflicting_event`, both documents read
  `attestation_seq: 3`, and the two ledger routes report the same
  `events[0].prev`, which is alice's seq-2 event id. Both branches verify,
  each where it landed:
  ```sh
  dc exec -T alice sh -c 'mabel verify ledger alice --from '"$witness_id"' \
    --peer "$(cat /shared/witness.ticket)" --json'
  docker exec mabel-alice-two sh -c 'mabel verify ledger alice \
    --from '"$witness_two_id"' --peer "$(cat /shared/witness-two.ticket)" --json'
  ```
  The first exits 0 with `valid: true` and `head_event == kept_event`, the
  second exits 0 with `valid: true` and `head_event == conflicting_event`.
  Nothing here forges a signature.
- Step 7 exits 30. Its document has `ok: false`, `code: 30`, `message` starting
  `Network error: `, `details.reason == "all_witnesses_failed"`,
  `details.results[0].status == "rejected"`, `details.results[0].reject_code ==
  "FORK"` and `details.results[0].at_seq == 3`. First seen wins: nothing
  overwrote the stored event.
- Witness one still serves the first branch: `GET
  http://127.0.0.1:9080/api/ledgers/<alice_id>` answers `entry.head_seq: 3`,
  `entry.head_event == kept_event`, `entry.fork_count: 1`,
  `entry.forks_truncated: false`, and `witnesses` listing both endpoint ids.
- Step 8's card list draws a fork count only where the witness recorded one:
  exactly one `identity-card-fork-count-*` element exists on the page, and it
  is alice's. The page shows no `forks_truncated` flag, because the redesigned
  route does not render one; `GET /api/ledgers/<alice_id>` above is where that
  flag is read.
- Step 8's identity page reads `witness-detail-fork-count` `1`.
- Step 8's fork record, at `fork-record-<alice_id>-3`:
  - `fork-statement-<alice_id>-3` reads exactly `two distinct validly signed
    events exist at seq 3 of <alice_id>, produced by whoever held signing
    authority there; this is evidence of equivocation or of a lost race between
    honest controllers`.
  - `fork-evidence-note` reads `a fork record proves two distinct validly
    signed events exist at one sequence, produced by whoever held signing
    authority there: it is evidence of equivocation or of a lost race between
    honest controllers, and it authorizes nothing`. No surface names a culprit.
  - `fork-kept-<alice_id>-3-event-id` carries `kept_event` and
    `fork-conflicting-<alice_id>-3-event-id` carries `conflicting_event`; both
    panes show `payload_kind` `trust_attestation`, the same `prev`, the same
    `author_key`, and `seq` 3. A reader checks the conflict without a second
    request.
  - `fork-source-endpoint-<alice_id>-3` carries the second machine's endpoint
    id, which is provenance and not authorization.
- Witness two recorded nothing: `GET http://127.0.0.1:9083/api/forks` answers
  `entries: []`, and `GET http://127.0.0.1:9083/api/ledgers/<alice_id>` answers
  `entry.head_event == conflicting_event` and `entry.fork_count: 0`.
- Step 9 exits 20 with `ok: false`, `code: 20`, `message` exactly `Ledger
  error: two sources hold divergent events at seq 3 of <alice_id>`,
  `details.reason == "equivocation"`, `details.at_seq == 3`, and two
  `details.candidates` entries: one `{source: <witness_id>, event_id:
  <kept_event>}` and one `{source: <witness_two_id>, event_id:
  <conflicting_event>}`. The verifier picks no winner.

## Deviations

Where `tests/e2e/specs/004-fork-on-two-witnesses.spec.ts` departs from or
exceeds the story text above.

- Step 5's `verify ledger` commands were rewritten to the forms the spec runs.
  `mabel verify ledger` reads its copy from a source over the network, and a
  CLI process in either container holds no address for one, so each command
  names the witness that holds that branch and passes its ticket.
- The spec does not run step 10. Story 005 opens on what this story leaves
  running and tears it down in its own step 11; the suite's global teardown
  clears it either way.
- Step 5's two `prev` readings go through `GET
  /api/identities/<alice_id>/ledger?since=3` on both wallets rather than
  through `curl` and `jq`, and step 8 also waits on `witness-ledger-detail`,
  the container the story does not name.
- The `forks_truncated` flag left the witness UI with the operator table
  (proposal 004). The spec keeps it pinned on `GET
  /api/ledgers/<alice_id>`, which is the only surface that reports it for the
  list, and reads `witness-detail-fork-count` on the identity page.

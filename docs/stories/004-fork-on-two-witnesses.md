# 004: the fork

- Status: implemented
- Surfaces: CLI, wallet UI, the node HTTP API
- Test: `tests/e2e/specs/004-fork-on-two-witnesses.spec.ts`

One wallet on two machines signs two different events at one sequence. Each
branch reaches a different witness identity, one of them meets both and records
the evidence, and a verifier that asks both exits 20 naming both sources.
Nothing here forges a signature: both events are valid.

## Actors

- alice: a node holding alice's key, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`. The first machine.
- alice's second machine: container `mabel-alice-two`, a byte-for-byte copy of
  alice's home with a fresh node key. It runs CLI commands only.
- witness one: compose service `witness`, API and UI on
  `http://127.0.0.1:9080`. It meets both branches. `witness_identity` is the
  Mabel id it witnesses for; `witness_id` is the machine that answers for it.
- witness two: compose service `witness-two`, API and UI on
  `http://127.0.0.1:9083`. It meets one branch. It is a second witness
  identity, not a second machine answering for the first: each home mints its
  own and witnesses for that one alone (proposal 006 sections 1 and 4).

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root. The second witness is a service of
`docker/compose.two-witnesses.yaml`, so compose starts it, waits for it and
wires both wallets to both witnesses; no step here hand-wires a `docker run`
for it (ticket 032).

## Story

1. Bring the topology up from nothing with the second witness over it:
   ```sh
   dc -f docker/compose.two-witnesses.yaml down -v
   dc -f docker/compose.two-witnesses.yaml up -d --wait
   ```
2. Read the two ids each witness published beside its ticket:
   ```sh
   witness_identity="$(dc exec -T witness cat /shared/witness.identity)"
   witness_id="$(dc exec -T witness cat /shared/witness.id)"
   witness_two_identity="$(dc exec -T witness cat /shared/witness-two.identity)"
   witness_two_id="$(dc exec -T witness cat /shared/witness-two.id)"
   witness_two_ticket="$(dc exec -T alice cat /shared/witness-two.ticket)"
   ```
   The two identities differ. `GET http://127.0.0.1:9083/api/node` answers
   `witness_for` holding `{identity: <witness_two_identity>, advertised: true,
   reason: null}`.
3. Alice creates one identity and two subjects to attest, and names both
   witness identities on her chain:
   ```sh
   dc exec -T alice mabel identity create --alias alice --kind person
   dc exec -T alice mabel identity create --alias carol --kind person
   dc exec -T alice mabel identity create --alias dave --kind person
   dc exec -T alice mabel witness add --identity alice --witness "$witness_identity"
   dc exec -T alice mabel witness add --identity alice --witness "$witness_two_identity"
   dc exec -T alice sh -c 'mabel sync push --identity alice \
     --peer "$(cat /shared/witness.ticket)" --peer "$(cat /shared/witness-two.ticket)"'
   ```
   Record `alice_id`, `carol_id` and `dave_id`. `mabel identity show alice
   --json` reports both witness identities in `witnesses`. Alice's ledger is at
   seq 2 and both witnesses hold it.
4. Copy alice's home to the second machine, dropping `node.json` and `node.key`
   so it gets its own endpoint id and keeps alice's identity keys:
   ```sh
   docker volume create mabel-alice-second
   docker run --rm --user 0 --volumes-from mabel-alice \
     --volume mabel-alice-second:/copy --entrypoint sh mabel:dev \
     -c 'cp -a /data/. /copy/ && rm -f /copy/node.json /copy/node.key'
   docker run -d --name mabel-alice-two --network mabel_mabel \
     --volume mabel-alice-second:/data --volume mabel_witness-ticket:/shared:ro \
     --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9084 --env MABEL_IROH_PORT=9074 \
     --publish 9084:9084 \
     mabel:dev serve --http 0.0.0.0:9084 --iroh-port 9074
   until curl -fsS http://127.0.0.1:9084/api/node >/dev/null; do sleep 1; done
   ```
   One command serves every home (proposal 006 section 8); `wallet serve` and
   `witness run` are hidden aliases and nothing here uses one. The wait matters:
   `docker run -d` returns before the home is prepared, and an `exec` that lands
   first sees no `node.json`. The published port is host port equals container
   port, as every other node's is, so the loopback rules accept a request from
   the host.
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
6. One branch to each witness. `--to` names the machine to dial, which is what a
   push connects to:
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
8. Read the conflict. A fork is a fact about a stored record, and `GET
   /api/forks` is the one route that reports it on every node (proposal 006
   section 8): `curl -fsS
   "http://127.0.0.1:9080/api/forks?ledger_id=$alice_id"`. On alice's own
   screens, open `http://127.0.0.1:9081/identities/<witness_identity>`: a
   witness is an identity, so what it keeps for other people is a section of its
   identity page, `witness-holdings`, asked live over the sync protocol. Alice's
   record there carries `identity-card-fork-count-<alice_id>` reading `1
   conflict`, and no other card on the page carries that element at all.
9. A fresh verifier asks both witnesses, told nothing but where to look:
   ```sh
   docker run --rm --network mabel_mabel mabel:dev \
     verify ledger "$alice_id" --peer "$witness_ticket" --peer "$witness_two_ticket" --json
   ```
   This home configures no witness at all, so the endpoints of its two tickets
   are the sources, and it pins neither.
10. Tear the extra container down: `docker rm -f mabel-alice-two && docker
    volume rm mabel-alice-second`, then `dc -f
    docker/compose.two-witnesses.yaml down -v`, which removes the second witness
    with everything else.

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
  http://127.0.0.1:9080/api/identities/<alice_id>` answers `identity.head_seq:
  3`, `identity.head_event == kept_event`, and `identity.witnesses` listing both
  witness identities. The set on the chain names identities, not machines: a
  witness that moves machines leaves those events standing.
- Step 8's fork record, the one entry `GET /api/forks?ledger_id=<alice_id>`
  answers:
  - `ledger_id == alice_id`, `seq: 3`, and `statement` exactly `two distinct
    validly signed events exist at seq 3 of <alice_id>, produced by whoever held
    signing authority there; this is evidence of equivocation or of a lost race
    between honest controllers`.
  - `kept.event_id == kept_event` and `conflicting.event_id ==
    conflicting_event`; both carry `payload_kind` `trust_attestation`, `seq` 3,
    the same `prev` and the same `author_key`. A reader checks the conflict
    without a second request, and no surface names a culprit.
  - `source_endpoint` is the second machine's endpoint id, which is provenance
    and not authorization.
- Step 8's screens: the witness's identity page draws `witness-holdings` with
  `witness-node-default` reading `yes, for the identities that chose no witness
  of their own`, `identity-card-entries-<alice_id>` reading `4 entries`, and
  exactly one `identity-card-fork-count-*` element, alice's, reading `1
  conflict`. Witness two's identity page draws the same section with no fork
  count on it at all.
- Witness two recorded nothing: `GET http://127.0.0.1:9083/api/forks` answers
  `entries: []`, and `GET http://127.0.0.1:9083/api/identities/<alice_id>`
  answers `identity.head_event == conflicting_event`.
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
  names the machine that holds that branch and passes its ticket.
- The spec does not run step 10. Story 005 opens on what this story leaves
  running and tears it down in its own step 12; the suite's global teardown
  clears it either way.
- Step 5's two `prev` readings go through `GET
  /api/identities/<alice_id>/ledger?since=3` on both wallets rather than
  through `curl` and `jq`.
- Step 9 runs its verifier with no `/shared` mount and no
  `MABEL_WAIT_FOR_TICKET`, so `node.json` configures no witness. A home that
  has one asks that witness alone, and one source cannot equivocate.
- The `forks_truncated` flag is not read here any more. It is a field of the
  store's own summary, which no HTTP document carries since the witness routes
  went away; `crates/mabel-node/tests/witness.rs` pins it.

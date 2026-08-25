# 006: stale append

- Status: implemented
- Surfaces: wallet UI (alice), CLI, wallet HTTP API, witness HTTP API
- Test: `tests/e2e/specs/006-stale-append.spec.ts`

Two machines may sign for one shared ledger. One of them signs on a head the
other has already moved, and the wallet refuses with exit code 50 instead of
producing a second branch. The repair is automatic and the retry is the same
action, run again.

## Actors

- alice: wallet node, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`. The machine that loses the race.
- alice's second machine: container `mabel-alice-two`, a byte-for-byte copy of
  alice's home with a fresh node key. The machine that wins it.
- bob: wallet node, compose service `bob`, a controller of the shared ledger
  who does nothing here. His key is what makes the ledger shared: no machine
  holds every controller key, so every append must ask the witness first.
- witness: compose service `witness`, API and UI on `http://127.0.0.1:9080`.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root. The race is one wallet's key on two machines, because that is
what runs today: bob is a controller on the chain but cannot append from his
own home until ticket 031 lands. This story tests one controller key on two
machines; two admitted controllers acting from their own homes is ticket 031.

## Story

1. Run story 002 steps 1 to 8. The shared ledger `org_id` is at seq 2 with two
   controllers, alice and bob, and alice's home holds only alice's key.
   `witness_identity` and `witness_id` are the two ids story 001 step 1 read.
2. Name the witness identity on the shared ledger and push it, so the ledger
   has somewhere to be asked about. Bob controls this ledger too, so the append
   asks the witness where it ends before it signs, and a CLI process needs the
   ticket to reach one: `node.json` records which machines answer for a witness,
   never how to route to one (proposal 006 section 5.4).
   ```sh
   dc exec -T alice sh -c 'mabel witness add --identity mabel-demo-co \
     --witness '"$witness_identity"' --peer "$(cat /shared/witness.ticket)"'
   dc exec -T alice sh -c 'mabel sync push --identity mabel-demo-co \
     --peer "$(cat /shared/witness.ticket)"'
   ```
   The shared ledger is now at seq 3 in alice's home and on the witness.
3. Make the second machine, after the push so both copies start level:
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
   `docker run -d` returns before the entrypoint has written `node.json`, so
   the wait is not optional: an `exec` that lands first fails on a home that is
   not a home yet.
4. In alice's UI at `http://127.0.0.1:9081/wallet`, open
   `identity-card-link-<org_id>`, click `action-trust-summary` to open the
   action, which starts closed, paste `bob_id` into `trust-add-subject` and
   click `trust-add-submit`. It succeeds: the witness is at seq 3, alice is at
   seq 3, nobody has moved. `identity-detail-event-count` reads `5`, and `GET
   /api/identities/<org_id>` answers `head_seq: 4`. Alice does not push.
5. The second machine attests someone else on the same ledger and pushes:
   ```sh
   docker exec mabel-alice-two sh -c 'mabel trust add --issuer mabel-demo-co \
     --subject '"$alice_id"' --peer "$(cat /shared/witness.ticket)"'
   docker exec mabel-alice-two sh -c 'mabel sync push --identity mabel-demo-co \
     --peer "$(cat /shared/witness.ticket)"'
   ```
   Its attestation is at seq 4 as well, and the witness now serves that one.
6. In alice's UI, with `action-trust` open, paste `bob_id` into
   `trust-add-subject` again and click `trust-add-submit`. This is the losing
   append: before anything is signed, the wallet asks the ledger's witnesses
   where it ends and finds an event it does not hold at seq 4.
7. Read what alice's home now holds. Round 5 of proposal 005 removed the since
   box, the limit box and the Load button: the page size is fixed at eight and
   nobody tunes it from the screen. A refused append does not refresh the page
   either, so open `/identities/<org_id>` in a second tab, which leaves the trust
   form on the first one holding what step 6 typed. Five rows appear,
   `ledger-event-0` to `ledger-event-4`. Alice's seq 4 is the second machine's
   event, not the one she signed in step 4.
8. Click `trust-add-submit` once more, with `bob_id` still in
   `trust-add-subject`. Losing a race is a retry: the same intent, re-signed on
   the new head.
9. Click `action-push-summary`, then `sync-push-submit`.
10. The second machine reads the settled chain back from the witness:
    ```sh
    docker exec mabel-alice-two sh -c 'mabel verify trust --issuer mabel-demo-co \
      --subject '"$bob_id"' --from '"$witness_id"' \
      --peer "$(cat /shared/witness.ticket)"'
    ```
    The `--peer` is explicit here: this container was started without
    `MABEL_WAIT_FOR_TICKET`, so nothing seeded the witness address for it.
11. Tear the extra container down, after the assertions above are made:
    ```sh
    docker rm -f mabel-alice-two
    docker volume rm mabel-alice-second
    ```

## Verified outcomes

- Step 6 shows the error envelope in the trust panel and appends nothing:
  - `trust-error` is present, `error-code` reads `code 50`, `error-status`
    reads `status 409`, `error-reason` reads `stale_head`.
  - `error-message` reads exactly `State error: witness <witness_id> reports
    head seq 4, this node holds seq 4`.
  - `error-code-meaning` reads `Something changed this record first. Reload the
    page and try again.`
  - `error-detail-ledger_id` carries `org_id`, `error-detail-local_head_seq`
    reads `4`, `error-detail-observed_head_seq` reads `4`, and
    `error-detail-source` carries `witness_id`.
- The same failure on the CLI is the same document:
  `dc exec -T alice mabel trust add --issuer mabel-demo-co --subject "$bob_id"
  --peer "$(cat /shared/witness.ticket)" --json` exits 50 with `ok: false`,
  `code: 50` and `details.reason == "stale_head"`.
- Step 7: alice's event from step 4 is gone from her home. `dc exec -T alice
  mabel trust list --issuer mabel-demo-co --json` answers `head_seq: 4` and a
  one-element `trust` array: `trust[0].subject == alice_id` and
  `trust[0].attestation_seq == 4`, the second machine's event, fetched during
  the failed attempt. The event id alice signed in step 4 appears nowhere in
  the ledger.
- Step 7's Ledger card agrees. The five closed lines read, in order, `created
  this identity`, `invited someone to help control this identity`, `confirmed
  someone as a controller`, `chose who keeps a copy` and `said it trusts
  someone`, which is the whole of a closed line beside its position. Opening
  each one shows the same five as raw kinds, `inception`,
  `membership_invitation`, `membership_acceptance`, `witness_set` and
  `trust_attestation`. The kind is `witness_set`, tag 19, because a witness set
  names identities; tag 11 `witness_config` is readable forever and never
  written again (proposal 006 section 1). In the open entry at position 4 the identifier inside
  `event-id-4` carries the second machine's event id and `event-payload-4` reads
  `{"subject":"<alice_id>"}`.
- No fork was created: `GET http://127.0.0.1:9080/api/forks?ledger_id=<org_id>`
  answers `entries: []`. The losing event was discarded before it was ever
  pushed, which is the difference between this story and story 004.
- Step 7's ledger has no footer at all: five entries at eight a page is one
  page, and round 5 of proposal 005 draws the pagination bar only over more than
  one. `ledger-event-count` reads `5`, and `ledger-footer`, `ledger-page-1`,
  `ledger-previous`, `ledger-next` and `ledger-range` are all absent.
- Step 8 succeeds: `trust-appended-event` shows a new event id,
  `identity-detail-event-count` reads `6`, `GET /api/identities/<org_id>` answers
  `head_seq: 5`, and `identity-card-<bob_id>` appears in
  `trust-list`. The same route answers that bob's entry is
  unrevoked and that its `attestation_event` is the id the form reported: the
  list is keyed by the identity trusted, so the entry is read on the record.
- Step 9's push report reads `push-status-<witness_id>` `accepted` and
  `push-stored-<witness_id>` `1`.
- Step 10 exits 0 and prints `trusted: true`, then `valid as of seq 5 of
  <org_id>, fetched from <witness_id> at <RFC 3339 UTC>; no revocation up to
  seq 5`, then `signed by principal <alice_id> (<alice active key>)`. The
  second machine reads the witness's seq-5 copy: `--from` pins the source, so
  the report is about the witness's chain, not this container's own.
- The witness agrees: `GET http://127.0.0.1:9080/api/identities/<org_id>`
  answers `identity.head_seq: 5` and `identity.event_count: 6`. A witness
  serves the identity routes every node serves; `/api/ledgers` is gone.

## Deviations

Where `tests/e2e/specs/006-stale-append.spec.ts` departs from or exceeds the
story text above.

- The CLI form of the exit-50 failure is not run at step 6. A losing append
  repairs the chain it lost on before returning 50, so one race produces one
  `stale_head`. The spec sets the race up a second time after step 10, with
  two subjects nothing has attested yet, and runs the CLI form there.
- Step 7's second tab is a spec device, not something a person would reach for.
  The panel refetches when the page it sits on refetches, and a refused append
  refetches nothing, so a reader would reload. A reload would clear
  `trust-add-subject`, and step 8 is exactly the claim that the box still holds
  what step 6 typed, so the spec reads the repaired chain on a second page in the
  same browser context and closes it again.
- Every position this story reads is read on `GET /api/identities/<org_id>`
  through the shared `expectHeadSeq` helper: round 5 of proposal 005 removed
  `identity-detail-head-seq`, and `identity-detail-event-count` is what the
  screen says instead.
- No story builds a record longer than eight entries, so this is the only
  ledger footer any of them reads and it reads the absent one. The bar itself,
  paging forward and back over two pages, is pinned by
  `ui/src/test/ledger-and-push.test.tsx`.
- Step 7 counts the five rows as `li[data-testid^="ledger-event-"]` under
  `ledger-events`. Proposal 005 draws the ledger as compact rows rather than a
  table, so a line is a list item.
- Step 7 opens all five lines and closes the first four again. The final round
  of proposal 005 moved the raw kind string in beside the entry id and the
  payload, so reading five kinds means five clicks, and the entry at position 4
  is left open because that is the one this story reads by id.
- "The event id alice signed in step 4 appears nowhere in the ledger" is
  checked against `GET /api/identities/<org_id>/ledger?since=0&limit=16` in
  alice's home: the five events it returns do not include that id.
- Step 11 tears the second machine down through the same helper the other
  stories use, which also removes `mabel-witness-two`. Nothing started it
  here, so that removal is a no-op.
- Every screen this story drives survived proposal 004: the Ledger card, the
  trust form and the error envelope all sit on `/identities/<org_id>`, which is
  now the only place an identity is shown. Only the way step 4 opens that page
  changed, from a row link to the card link.

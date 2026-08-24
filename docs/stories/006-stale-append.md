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
2. Name the witness on the shared ledger and push it, so the ledger has
   somewhere to be asked about:
   ```sh
   dc exec -T alice mabel witness add --identity mabel-demo-co --endpoint "$witness_id"
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
     --env MABEL_ROLE=wallet --env MABEL_RELAY=disabled \
     --env MABEL_HTTP_BIND=0.0.0.0:9084 --env MABEL_IROH_PORT=9074 \
     --publish 9084:9084 \
     mabel:dev wallet serve --http 0.0.0.0:9084 --iroh-port 9074
   until curl -fsS http://127.0.0.1:9084/api/node >/dev/null; do sleep 1; done
   ```
   `docker run -d` returns before the entrypoint has written `node.json`, so
   the wait is not optional: an `exec` that lands first fails on a home that is
   not a home yet.
4. In alice's UI at `http://127.0.0.1:9081/wallet`, open
   `identity-card-link-<org_id>`, click `action-trust-summary` to open the
   action, which starts closed, paste `bob_id` into `trust-add-subject` and
   click `trust-add-submit`. It succeeds: the witness is at seq 3, alice is at
   seq 3, nobody has moved. `identity-detail-head-seq` reads `4`. Alice does
   not push.
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
7. Read what alice's home now holds: in the Ledger card set `ledger-since` to
   `0`, `ledger-limit` to `8`, and click `ledger-load`. Five rows appear,
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
- Step 7's Ledger card agrees: `event-payload-kind-4` reads
  `trust_attestation`, the identifier inside `event-id-4` carries the second
  machine's event id, and `event-payload-4` reads `{"subject":"<alice_id>"}`.
  The five rows read `inception`, `membership_invitation`,
  `membership_acceptance`, `witness_config`, `trust_attestation` in order.
- No fork was created: `GET http://127.0.0.1:9080/api/forks` answers `entries:
  []` and `GET http://127.0.0.1:9080/api/ledgers/<org_id>` answers
  `entry.fork_count: 0`. The losing event was discarded before it was ever
  pushed, which is the difference between this story and story 004.
- Step 8 succeeds: `trust-appended-event` shows a new event id,
  `identity-detail-head-seq` reads `5`, and a row for the new attestation reads
  `trusted`.
- Step 9's push report reads `push-status-<witness_id>` `accepted` and
  `push-stored-<witness_id>` `1`.
- Step 10 exits 0 and prints `trusted: true`, then `valid as of seq 5 of
  <org_id>, fetched from <witness_id> at <RFC 3339 UTC>; no revocation up to
  seq 5`, then `signed by principal <alice_id> (<alice active key>)`. The
  second machine reads the witness's seq-5 copy: `--from` pins the source, so
  the report is about the witness's chain, not this container's own.
- The witness agrees: `GET http://127.0.0.1:9080/api/ledgers/<org_id>` answers
  `entry.head_seq: 5` and `entry.event_count: 6`.

## Deviations

Where `tests/e2e/specs/006-stale-append.spec.ts` departs from or exceeds the
story text above.

- The CLI form of the exit-50 failure is not run at step 6. A losing append
  repairs the chain it lost on before returning 50, so one race produces one
  `stale_head`. The spec sets the race up a second time after step 10, with
  two subjects nothing has attested yet, and runs the CLI form there.
- Step 7's "set `ledger-since` to 0, `ledger-limit` to 8, Load" is a no-op on a
  panel that opens at those values and refetches only when one of them
  changes. The spec moves the limit to 16, loads, moves it back to 8 and loads
  again, which is the same read.
- Step 7 counts the five rows as `li[data-testid^="ledger-event-"]` under
  `ledger-events`. Proposal 005 draws the ledger as compact rows rather than a
  table, so a line is a list item.
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

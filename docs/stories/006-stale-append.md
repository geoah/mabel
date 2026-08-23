# 006: stale append

- Status: draft
- Surfaces: wallet UI (alice), CLI
- Test: `tests/e2e/006-stale-append.spec.ts` (not written yet)

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
repository root.

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
   The shared ledger is now at seq 3 on both machines' witness.
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
     mabel:dev wallet serve --http 0.0.0.0:9084 --iroh-port 9074
   ```
4. In alice's UI at `http://127.0.0.1:9081/wallet`, open
   `identity-link-<org_id>`, paste `bob_id` into `trust-add-subject` and click
   `trust-add-submit`. It succeeds: the witness is at seq 3, alice is at seq 3,
   nobody has moved. `identity-detail-head-seq` reads `4`. Alice does not push.
5. The second machine attests someone else on the same ledger and pushes:
   ```sh
   docker exec mabel-alice-two sh -c 'mabel trust add --issuer mabel-demo-co \
     --subject '"$alice_id"' --peer "$(cat /shared/witness.ticket)"'
   docker exec mabel-alice-two sh -c 'mabel sync push --identity mabel-demo-co \
     --peer "$(cat /shared/witness.ticket)"'
   ```
   Its attestation is at seq 4 as well, and the witness now serves that one.
6. In alice's UI, paste `bob_id` into `trust-add-subject` again and click
   `trust-add-submit`. This is the losing append: before anything is signed,
   the wallet asks the ledger's witnesses where it ends and finds an event it
   does not hold at seq 4.
7. Read what the panel now shows in the ledger card: click `ledger-load` with
   `ledger-since` at `0`. Alice's seq 4 is the second machine's event, not the
   one she signed in step 4.
8. Click `trust-add-submit` once more, with `bob_id` still in
   `trust-add-subject`. Losing a race is a retry: the same intent, re-signed on
   the new head.
9. Click `sync-push-submit`, then tear the extra container down:
   `docker rm -f mabel-alice-two && docker volume rm mabel-alice-second`.

## Verified outcomes

- Step 6 shows the error envelope in the trust panel and appends nothing:
  - `trust-error` is present, `error-code` reads `code 50`, `error-status`
    reads `status 409`, `error-reason` reads `stale_head`.
  - `error-message` reads exactly `State error: witness <witness_id> reports
    head seq 4, this node holds seq 4`.
  - `error-code-meaning` reads `stale state, a conflicting event or a replay`.
  - `error-detail-ledger_id` carries `org_id`, `error-detail-local_head_seq`
    reads `4`, `error-detail-observed_head_seq` reads `4`, and
    `error-detail-source` carries `witness_id`.
- The same failure on the CLI is the same document:
  `dc exec -T alice mabel trust add --issuer mabel-demo-co --subject "$bob_id"
  --peer "$(cat /shared/witness.ticket)" --json` exits 50 with `ok: false`,
  `code: 50` and `details.reason == "stale_head"`.
- Step 7: alice's event from step 4 is gone from her home. `dc exec -T alice
  mabel trust list --issuer mabel-demo-co --json` answers `head_seq: 4` and one
  entry, whose `subject == alice_id` and whose `attestation_seq` is 4: the
  second machine's event, fetched during the failed attempt. The event id alice
  signed in step 4 appears nowhere in the ledger.
- No fork was created: `GET http://127.0.0.1:9080/api/forks` answers `entries:
  []` and `GET http://127.0.0.1:9080/api/ledgers/<org_id>` answers
  `fork_count: 0`. The losing event was discarded before it was ever pushed,
  which is the difference between this story and story 004.
- Step 8 succeeds: `trust-appended-event` shows a new event id,
  `identity-detail-head-seq` reads `5`, and a row for the new attestation reads
  `unrevoked`.
- Step 9's push report reads `push-status-<witness_id>` `accepted` and
  `push-stored-<witness_id>` `1`.
- After step 9 both machines agree:
  `docker exec mabel-alice-two mabel verify trust --issuer mabel-demo-co
  --subject "$bob_id" --from "$witness_id"` exits 0 with `trusted: true` and
  the statement `valid as of seq 5 of <org_id>, fetched from <witness_id> at
  <RFC 3339 UTC>; no revocation up to seq 5`.

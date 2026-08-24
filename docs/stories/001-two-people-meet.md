# 001: two people meet

- Status: implemented
- Surfaces: wallet UI (alice and bob), CLI, wallet HTTP API, witness HTTP API
- Test: `tests/e2e/specs/001-two-people-meet.spec.ts`

Two strangers create identities in two wallet UIs, exchange descriptors out of
band, name the same witness, push, and each attests trust in the other. A third
party with an empty home reads the result from the witness alone.

## Actors

- alice: wallet node, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`.
- bob: wallet node, compose service `bob`, API and UI on
  `http://127.0.0.1:9082`.
- witness: witness node, compose service `witness`, API and UI on
  `http://127.0.0.1:9080`.
- a stranger: one throwaway container with an empty home, holding no identity
  and no key but the node key it makes on the spot.

Host and port must be `127.0.0.1` (or `localhost`) on the port the node bound,
or the API answers 403 `host_not_loopback` (proposal 001 section 10). `dc`
below stands for `docker compose -f docker/compose.yaml`, run from the
repository root.

## Story

1. Bring the topology up from nothing: `dc down -v && dc up -d --wait`. All
   three services report healthy. Read the witness endpoint id,
   `witness_id="$(dc exec -T witness cat /shared/witness.id)"`, a 52-character
   lowercase base32 string.
2. Open `http://127.0.0.1:9081/wallet`. The nav holds two entries and no third,
   `nav-wallet` and `nav-witnesses`, the search box `wallet-search` is there,
   and `identity-list-empty` reads `You have no identities yet. Create one
   below.` The role itself is a fact of `GET /api/node`, which answers `role:
   "wallet"`.
3. Click `identity-create-summary` to unfold the create form, which the wallet
   home keeps closed. Type `alice` into `identity-create-alias`, leave
   `identity-create-declared-kind` at `person`, leave `identity-create-founder`
   empty, click `identity-create-submit`. `identity-create-result-identity-id`
   appears; record its identifier `data-value` as `alice_id`.
4. Repeat step 3 at `http://127.0.0.1:9082/wallet` with alias `bob`; record
   `bob_id`.
5. Exchange descriptors out of band. The descriptor carries the inception byte
   for byte, which is what binds an id to a key; nothing in the protocol proves
   it, which is what the flag-L sentence in every report says.
   ```sh
   dc exec -T bob mabel identity export bob --out /tmp/bob.descriptor
   docker cp mabel-bob:/tmp/bob.descriptor /tmp/bob.descriptor
   docker cp /tmp/bob.descriptor mabel-alice:/tmp/bob.descriptor
   dc exec -T alice mabel identity export alice --out /tmp/alice.descriptor
   docker cp mabel-alice:/tmp/alice.descriptor /tmp/alice.descriptor
   docker cp /tmp/alice.descriptor mabel-bob:/tmp/alice.descriptor
   ```
   Each export prints `exported <id> to <path> (N bytes)` and a second line
   `declared kind person, raw root, 0 witnesses`.
6. In alice's UI click `identity-card-link-<alice_id>`: the whole card is one
   link to `/identities/<alice_id>`. On the identity page click
   `action-witnesses-summary` to open the action, which starts closed, put
   `$witness_id` into `witness-add-endpoint` and click `witness-add-submit`.
   `witness-add-head-seq` reads `Saved at position 1.` and
   `witness-row-<witness_id>` appears. Do the same in bob's UI.
7. In each UI click `action-push-summary`, leave `sync-push-to` empty and click
   `sync-push-submit`. `sync-push-report` appears with
   `push-status-<witness_id>` reading `accepted`,
   `push-stored-<witness_id>` reading `2` and `sync-push-head-seq` reading `1`.
8. In alice's UI click `action-trust-summary`, paste `bob_id` into
   `trust-add-subject` and click `trust-add-submit`. `trust-appended-event`
   shows the new event id, a row `trust-row-<attestation>` appears,
   `trust-state-<attestation>` reads `trusted since position 2` and
   `identity-detail-head-seq` reads `2`. Record the event id as
   `alice_attestation`.
9. In bob's UI do the same with `alice_id` as the subject. Trust is one-way
   (decision 003), so this is a second event in a second ledger, not a
   handshake.
10. Click `sync-push-submit` in both UIs again. Each report reads
    `push-status-<witness_id>` `accepted` and `push-stored-<witness_id>` `1`.
11. A stranger verifies from an empty home, reading the witness's copy and
    nothing else:
    ```sh
    docker run --rm --network mabel_mabel \
      --volume mabel_witness-ticket:/shared:ro \
      --env MABEL_WAIT_FOR_TICKET=/shared/witness \
      mabel:dev verify trust --issuer "$alice_id" --subject "$bob_id" \
      --from "$witness_id"
    ```
    The compose project is named `mabel`, so the bridge is `mabel_mabel` and
    the ticket volume is `mabel_witness-ticket`; `docker network ls` and
    `docker volume ls` confirm both.
12. Run step 11 again with `--json` for the document assertions below.
13. The subject nobody can read. Alice creates a third identity that never
    reaches the witness, and attests it:
    ```sh
    dc exec -T alice mabel identity create --alias carol --kind person
    ```
    Record `carol_id`. In alice's UI open `action-trust`, paste `carol_id` into
    `trust-add-subject`, click `trust-add-submit` (`identity-detail-head-seq`
    reads `3`), then open `action-push` and click `sync-push-submit`.
14. Verify that attestation from an empty home. Carol's ledger is in nobody's
    reach: alice pushed her own ledger, not carol's.
    ```sh
    docker run --rm --network mabel_mabel \
      --volume mabel_witness-ticket:/shared:ro \
      --env MABEL_WAIT_FOR_TICKET=/shared/witness \
      mabel:dev verify trust --issuer "$alice_id" --subject "$carol_id" \
      --from "$witness_id" --json
    ```
    The subject's participation is deliberately not required (decision 003), so
    this is an answer, not a failure.
15. Alice saves her keys. Open `identity-card-link-<alice_id>` and click
    `action-keys-summary`. `identity-keys-active` and `identity-keys-reserve`
    each hold a 52-character lowercase base32 secret key, and
    `identity-keys-warning` says what holding them and losing them means. The
    same two values are what `GET
    http://127.0.0.1:9081/api/identities/<alice_id>/keys` answers (decision
    017).

## Verified outcomes

- Step 3: `identity-create-result-identity-id` and
  `identity-create-result-inception-event` carry the same `data-value`: an
  identity is the digest of its own inception event.
- Step 6: `GET http://127.0.0.1:9081/api/identities/<alice_id>` answers
  `identity.witnesses == ["<witness_id>"]`, `identity.head_seq: 1`,
  `identity.event_count: 2`.
- Step 8: the trust panel row for `alice_attestation` reads `trusted since
  position 2`, and the same document carries `identity.trust[0].subject ==
  bob_id` and `identity.trust[0].revoked == false`.
- Step 11 exits 0. Its stdout is five lines in this order:
  - `trusted: true`
  - `valid as of seq 2 of <alice_id>, fetched from <witness_id> at <RFC 3339
    UTC>; no revocation up to seq 2`
  - `signed by principal <alice_id> (<alice active key>)`
  - `subject control was not proven to this verifier; the issuer is
    responsible for out-of-band confirmation`
  - `Verified means this identity signed this statement at this position in
    its chain. It is not proof that the statement is true, not proof of legal
    identity, and not proof of unique humanity.`
- Step 12's document has `ok: true`, `kind: "trust"`, `trusted: true`,
  `subject_resolution: "resolved"`, `subject_note: null`,
  `attestation_event == alice_attestation`, `attestation_seq: 2`,
  `signing_principal.identity == alice_id`, `revoked_count: 0`,
  `source == witness_id`, `sources_queried == [witness_id]`, `head_seq: 2`.
- The mirrored verification, `--issuer "$bob_id" --subject "$alice_id"`, also
  exits 0 with `trusted: true`: two ledgers, two events.
- After step 10, `GET http://127.0.0.1:9080/api/ledgers` lists exactly two
  entries, `alice_id` and `bob_id`, each with `declared_kind: "person"`,
  `head_seq: 2`, `event_count: 3`, `fork_count: 0`.
- Step 14 exits 0, and its document reads `trusted: true` with
  `subject_resolution: "unresolved"` and `subject_note` exactly `subject:
  unresolved (not held by any queried source)`. The text form prints that
  sentence as its own line, after `signed by principal ...` and before the two
  standing sentences.
- Step 14's `head_seq` is 3 and its statement reads `valid as of seq 3 of
  <alice_id>, fetched from <witness_id> at <RFC 3339 UTC>; no revocation up to
  seq 3`. An unresolved subject changes what is reported, never the exit code:
  only chain, signature and equivocation failures exit 20.
- `GET http://127.0.0.1:9080/api/ledgers/<carol_id>` answers 404 with
  `details.reason == "ledger_not_held"`: the witness holds no copy of the
  subject, which is exactly what step 14 reported.
- After step 13 alice's wallet home draws one card per identity, in the
  ascending identity id order `GET /api/identities` answers in:
  `identity-cards` holds `identity-card-<alice_id>` and
  `identity-card-<carol_id>`. Alice's card reads
  `identity-card-name-<alice_id>-name` `alice`,
  `identity-card-declared-kind-<alice_id>` `person` and
  `identity-card-head-seq-<alice_id>` `at position 3`; carol's reads `at
  position 0`, because she was created and never appended to.
  `identity-card-link-<alice_id>` points at `/identities/<alice_id>`: the card
  is the page, and there is no selection state anywhere.
- Step 15: `GET /api/identities/<alice_id>/keys` answers 200 with `identity_id
  == alice_id`, an `active_secret_key` and a `reserve_secret_key` matching what
  the two boxes hold, and an `active_key` equal to `identity.active_key` of the
  identity document.

## Deviations

Where `tests/e2e/specs/001-two-people-meet.spec.ts` departs from or exceeds the
story text above.

- The spec asserts two testids the story never names. `identity-detail` is how
  the shared `openIdentity` helper knows an identity page opened, and
  `identity-detail-identity-id` is read in a last test that checks the whole
  52-character value is what `data-value` holds.
- Step 13 creates carol with `--json` added, so the spec can read `carol_id`
  from the document instead of parsing the text form.
- Step 2's role assertion goes through `GET /api/node`. Proposal 004 removed
  the node card from the wallet home, so no testid carries the role; the two
  nav entries and the search box are what the spec reads on the screen.
- The shared `createIdentity` helper clicks `identity-create-summary` only when
  the form is not already on the screen: a summary click toggles, so a second
  one would close the form the previous step opened. The shared `openAction`
  helper does the same for each closed action of steps 6, 7, 8 and 15.
- The spec runs step 15 straight after step 7, where it needs nothing that
  steps 8 to 14 add, so the whole keys assertion sits in one test.

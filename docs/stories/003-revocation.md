# 003: revocation

- Status: draft
- Surfaces: wallet UI (alice), CLI
- Test: `tests/e2e/003-revocation.spec.ts` (not written yet)

Alice revokes an attestation in her wallet UI. A verifier that has never seen
the earlier answer reads the revocation from the witness and says how far it
read. Alice then attests again, and the same verifier says trusted.

## Actors

- alice: wallet node, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`. The issuer.
- bob: wallet node, compose service `bob`, API and UI on
  `http://127.0.0.1:9082`. The subject, who signs nothing here.
- witness: witness node, compose service `witness`, API and UI on
  `http://127.0.0.1:9080`.
- a fresh verifier: one throwaway container per verification, with an empty
  home.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root.

## Story

1. Run story 001 in full. Alice's ledger is at seq 2 with one unrevoked
   attestation naming bob, pushed to the witness, and a fresh home already
   answered `trusted: true`. Keep `alice_id`, `bob_id`, `witness_id` and
   `alice_attestation`.
2. In alice's UI open `identity-link-<alice_id>`. The trust table shows
   `trust-row-<alice_attestation>` with `trust-state-<alice_attestation>`
   reading `unrevoked`.
3. Click `trust-revoke-<alice_attestation>`. `trust-appended-event` shows the
   revocation event id, `trust-state-<alice_attestation>` now reads `revoked at
   seq 3`, the `trust-revoke-<alice_attestation>` button is disabled and
   `identity-detail-head-seq` reads `3`. The attestation stays in the table:
   the chain is the full history (decision 003).
4. Click `sync-push-submit`. `push-status-<witness_id>` reads `accepted` and
   `push-stored-<witness_id>` reads `1`.
5. A fresh verifier reads the witness's copy:
   ```sh
   docker run --rm --network mabel_mabel \
     --volume mabel_witness-ticket:/shared:ro \
     --env MABEL_WAIT_FOR_TICKET=/shared/witness \
     mabel:dev verify trust --issuer "$alice_id" --subject "$bob_id" \
     --from "$witness_id"
   ```
   Run it again with `--json` for the document assertions.
6. Alice attests bob again: paste `bob_id` into `trust-add-subject`, click
   `trust-add-submit`. A second row appears, `trust-state-<second attestation>`
   reads `unrevoked`, `identity-detail-head-seq` reads `4`. Record the event id
   as `second_attestation`. Nothing forbids this: the policy refuses only a
   second *unrevoked* attestation for one subject.
7. Click `sync-push-submit` again, then repeat step 5 in a new container.

## Verified outcomes

- Step 3: `GET http://127.0.0.1:9081/api/identities/<alice_id>` answers
  `identity.trust[0].revoked == true`, `identity.trust[0].revocation_seq == 3`
  and `identity.trust[0].attestation_event == alice_attestation`.
- Step 5 exits 0 (a revoked attestation is a successful verification, not a
  failure) and prints, in order:
  - `trusted: false`
  - `valid as of seq 3 of <alice_id>, fetched from <witness_id> at <RFC 3339
    UTC>; attestation <alice_attestation> revoked at seq 3`
  - `subject control was not proven to this verifier; the issuer is
    responsible for out-of-band confirmation`
  - `Verified means this identity signed this statement at this position in
    its chain. It is not proof that the statement is true, not proof of legal
    identity, and not proof of unique humanity.`
- No `signed by principal` line appears in step 5: `signing_principal` is null
  when `trusted` is false.
- Step 5's document has `ok: true`, `trusted: false`, `attestation_event:
  null`, `attestation_seq: null`, `revoked_count: 1`,
  `revoked_attestations[0].attestation_event == alice_attestation`,
  `revoked_attestations[0].attestation_seq == 2`,
  `revoked_attestations[0].revocation_seq == 3`, `head_seq: 3`.
- The statement never says "unrevoked" and never claims global completeness: it
  names the source, the head it read to and the fetch time (flag R, proposal
  001 section 6).
- Step 7's verification exits 0 with `trusted: true`, `attestation_event ==
  second_attestation`, `attestation_seq: 4`, `revoked_count: 1`, and the
  statement `valid as of seq 4 of <alice_id>, fetched from <witness_id> at
  <RFC 3339 UTC>; no revocation up to seq 4`. The revoked attestation stays in
  `revoked_attestations`.
- Alice's UI verify page agrees: with `alice_id`, `bob_id` and `witness_id` in
  the three inputs, `verify-report-trusted-badge` reads `false` after step 4
  and `true` after step 7, and
  `verify-report-revoked-<alice_attestation>` is present in the revoked table
  in both.

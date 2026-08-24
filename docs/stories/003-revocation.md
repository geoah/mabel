# 003: revocation

- Status: implemented
- Surfaces: wallet UI (alice), CLI, wallet HTTP API
- Test: `tests/e2e/specs/003-revocation.spec.ts`

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

1. Run story 001 steps 1 to 12. Alice's ledger is at seq 2 with one unrevoked
   attestation naming bob, pushed to the witness, and a fresh home already
   answered `trusted: true`. Keep `alice_id`, `bob_id`, `witness_id` and
   `alice_attestation`.
2. In alice's UI open `identity-card-link-<alice_id>`. The trust list shows
   `trust-row-<alice_attestation>` with `trust-state-<alice_attestation>`
   reading `trusted`.
3. Attest bob a second time, before revoking anything. One unrevoked
   attestation per subject is the rule, so this is refused:
   ```sh
   dc exec -T alice mabel trust add --issuer alice --subject "$bob_id" --json
   ```
   Click `action-trust-summary` to open the action, which starts closed, paste
   `bob_id` into `trust-add-subject` and click `trust-add-submit` for the same
   refusal in the UI.
4. Click `trust-revoke-<alice_attestation>`. `trust-appended-event` shows the
   revocation event id, `trust-state-<alice_attestation>` now reads `taken
   back`, the `trust-revoke-<alice_attestation>` button is disabled and
   `identity-detail-head-seq` reads `3`. The row leaves the standing list for
   the folded `trust-revoked` list, whose `trust-revoked-summary` reads `1 taken
   back, still on the record`, and `trust-list-empty` reads `This identity has
   not said it trusts anyone yet.` The attestation stays on the screen: the
   chain is the full history (decision 003).
5. Click `action-push-summary`, then `sync-push-submit`.
   `push-status-<witness_id>` reads `accepted` and `push-stored-<witness_id>`
   reads `1`.
6. A fresh verifier reads the witness's copy:
   ```sh
   docker run --rm --network mabel_mabel \
     --volume mabel_witness-ticket:/shared:ro \
     --env MABEL_WAIT_FOR_TICKET=/shared/witness \
     mabel:dev verify trust --issuer "$alice_id" --subject "$bob_id" \
     --from "$witness_id"
   ```
   Run it again with `--json` for the document assertions.
7. Read the same answer from alice's own home instead of an empty one.
   Verification is a CLI concern (proposal 004): the wallet UI has no verify
   screen, so the same command runs in her container, which needs `--peer`
   because a CLI process holds no seeded witness address:
   ```sh
   dc exec -T alice sh -c 'mabel verify trust --issuer '"$alice_id"' \
     --subject '"$bob_id"' --from '"$witness_id"' \
     --peer "$(cat /shared/witness.ticket)"'
   ```
8. Alice attests bob again: click `nav-wallet`, open
   `identity-card-link-<alice_id>`, click `action-trust-summary`, paste `bob_id`
   into `trust-add-subject`, click `trust-add-submit`. A second row appears, and
   `trust-state-<second attestation>` reads `trusted` with
   `identity-detail-head-seq` reading `4`. Record the event id as
   `second_attestation`. This is the same command step 3 refused: the policy
   refuses only a second *unrevoked* attestation for one subject.
9. Open `action-push` and click `sync-push-submit` again, then repeat step 6 in
   a new container.

## Verified outcomes

- Step 3's CLI attempt exits 20 and appends nothing. Its document has `ok:
  false`, `code: 20`, `details.reason == "duplicate_unrevoked_attestation"`,
  `details.subject == bob_id`, `details.attestation_event ==
  alice_attestation`, `details.at_seq == 2`, and `message` exactly `Policy
  error: an unrevoked attestation for <bob_id> already exists at seq 2`, the
  `policy` case of `contracts/cli/errors.json`.
- Step 3's UI attempt is refused too, in the same words. `trust-error` is
  present with `error-code` reading `code 20`, `error-status` reading `status
  409`, `error-code-meaning` reading `A signature, the record itself or a rule
  refused this.`, `error-reason` reading `duplicate_unrevoked_attestation` and
  `error-message` reading `Policy error: an unrevoked attestation for <bob_id>
  already exists at seq 2`. `error-detail-at_seq` reads `2`, the position of the attestation
  still standing. `identity-detail-head-seq` still reads `2` on both paths.
- Step 4: `GET http://127.0.0.1:9081/api/identities/<alice_id>` answers
  `identity.trust[0].revoked == true`, `identity.trust[0].revocation_seq == 3`
  and `identity.trust[0].attestation_event == alice_attestation`.
- Step 6 exits 0 (a revoked attestation is a successful verification, not a
  failure) and prints, in order:
  - `trusted: false`
  - `valid as of seq 3 of <alice_id>, fetched from <witness_id> at <RFC 3339
    UTC>; attestation <alice_attestation> revoked at seq 3`
  - `subject control was not proven to this verifier; the issuer is
    responsible for out-of-band confirmation`
  - `Verified means this identity signed this statement at this position in
    its chain. It is not proof that the statement is true, not proof of legal
    identity, and not proof of unique humanity.`
- No `signed by principal` line appears in step 6: `signing_principal` is null
  when `trusted` is false.
- Step 6's document has `ok: true`, `trusted: false`, `attestation_event:
  null`, `attestation_seq: null`, `revoked_count: 1`,
  `revoked_attestations[0].attestation_event == alice_attestation`,
  `revoked_attestations[0].attestation_seq == 2`,
  `revoked_attestations[0].revocation_seq == 3`, `head_seq: 3`.
- The statement never says "unrevoked" and never claims global completeness: it
  names the source, the head it read to and the fetch time (flag R, proposal
  001 section 6).
- Step 9's verification exits 0 with `trusted: true`, `attestation_event ==
  second_attestation`, `attestation_seq: 4`, `revoked_count: 1`, and the
  statement `valid as of seq 4 of <alice_id>, fetched from <witness_id> at
  <RFC 3339 UTC>; no revocation up to seq 4`. The revoked attestation stays in
  `revoked_attestations`.
- Step 7 prints the same four lines step 6 printed, from a home that holds
  alice's ledger. Where the report is read from does not change it, because
  `--from` pins the source; only the fetch time inside the statement moves.
- Step 6's document also reads `signing_principal: null`, with
  `subject_control` and `verified_means` carrying the two standing sentences
  the text form printed as its last two lines.
- Run again after step 9, the document reads `trusted: true`,
  `attestation_seq: 4`, `signing_principal.identity == alice_id`, and a
  one-element `revoked_attestations` still naming `alice_attestation` with
  `revocation_seq: 3`: revocation is history, not deletion.

## Deviations

Where `tests/e2e/specs/003-revocation.spec.ts` departs from or exceeds the
story text above.

- The spec asserts one container testid the story never names, because the
  shared UI helpers wait on it: `identity-detail`. Those helpers open the closed
  action each form lives in, and step 3's refusal opens `action-trust` itself
  because it fills the form without the helper.
- Step 9 repeats step 6 in its `--json` form only. Step 6 already pins the
  text form line by line, and the document is what step 9 adds.
- Step 4 reads `trust-state-<alice_attestation>` after the row has moved into
  the closed `trust-revoked` list. The element stays in the DOM, so its text
  and the disabled revoke button are readable without opening the list.
- Steps 7 and 9 lost their UI half with the verify tab (proposal 004). What the
  report screen asserted, the verdict, the statement, the revoked list and the
  null signing principal, is asserted on the `--json` document instead, and
  step 7 became the same command run from alice's own home.
- Step 7 compares its four lines against step 6's with the RFC 3339 time
  masked: two reads of one witness are two fetch times.

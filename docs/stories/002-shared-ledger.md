# 002: the shared ledger

- Status: draft
- Surfaces: wallet UI (alice), wallet HTTP API (bob), CLI
- Test: `tests/e2e/002-shared-ledger.spec.ts` (not written yet)

Alice founds a ledger with an identity root, invites bob as a controller
through the three file artifacts, and the shared ledger attests trust in bob.
The ledger holds no key of its own, so a verifier is told which principal
signed.

## Actors

- alice: wallet node, compose service `alice`, API and UI on
  `http://127.0.0.1:9081`. Founder and controller of the shared ledger.
- bob: wallet node, compose service `bob`, API and UI on
  `http://127.0.0.1:9082`. The invitee, who signs his own acceptance.
- witness: witness node, compose service `witness`, API and UI on
  `http://127.0.0.1:9080`.

`dc` stands for `docker compose -f docker/compose.yaml`, run from the
repository root. Bob's wallet UI has no membership screen yet (ticket 019 is
superseded by ticket 028), so step 6 drives the shipped
`/memberships/acceptances` route directly and step 10 drives the CLI
equivalent; the accept surface they assert is the document that screen will
render.

## Story

1. Run story 001 steps 1 to 7. Alice and bob each hold one person identity,
   both name the witness and both are pushed. `alice_id`, `bob_id`,
   `witness_id` and `/tmp/bob.descriptor` inside alice's container are the
   values that story left behind.
2. In alice's UI at `http://127.0.0.1:9081/wallet`, type `mabel-demo-co` into
   `identity-create-alias`, select `organization` in
   `identity-create-declared-kind`, paste `alice_id` into
   `identity-create-founder`, and click `identity-create-submit`. Record
   `identity-create-result-identity-id` as `org_id`.
3. Open `identity-link-<org_id>`. `identity-detail-declared-kind` reads
   `organization`, `identity-detail-active-key` and
   `identity-detail-reserve-commit` both read `null`: an identity-rooted ledger
   holds no key of its own and its controllers sign for it (decision 002 as
   amended).
4. Alice invites bob as a controller:
   ```sh
   dc exec -T alice mabel membership invite --ledger mabel-demo-co --by alice \
     --invitee /tmp/bob.descriptor --role controller --out /tmp/invitation.bundle
   ```
   It prints `invited <bob_id> as controller at seq 1 of <org_id>` and
   `wrote /tmp/invitation.bundle (2 events, N bytes)`.
5. Carry the bundle to bob's machine, which shares no disk with alice's:
   ```sh
   docker cp mabel-alice:/tmp/invitation.bundle /tmp/invitation.bundle
   bundle="$(base64 -w0 /tmp/invitation.bundle)"
   ```
6. Bob's wallet folds the bundle and answers the accept surface, then signs:
   ```sh
   surface="$(curl -fsS -X POST \
     -H 'Origin: http://127.0.0.1:9082' -H 'Content-Type: application/json' \
     --data "{\"invitation_bundle_base64\":\"$bundle\"}" \
     "http://127.0.0.1:9082/api/identities/$bob_id/memberships/acceptances")"
   acceptance_base64="$(printf '%s' "$surface" | jq -r .acceptance_base64)"
   ```
   The surface is the fold of the bundle, not a claim the file makes.
7. Carry the acceptance back as the file a controller admits. The base64 in the
   response is the same bytes `mabel membership accept --out` writes:
   ```sh
   printf '%s' "$acceptance_base64" | base64 -d > /tmp/acceptance.file
   docker cp /tmp/acceptance.file mabel-alice:/tmp/acceptance.file
   dc exec -T alice mabel membership admit --ledger mabel-demo-co --by alice \
     /tmp/acceptance.file
   ```
   It prints `admitted <bob_id> as controller at seq 2 of <org_id>`.
8. Read the membership state: `dc exec -T alice mabel membership list --ledger
   mabel-demo-co`.
9. Read the same state in alice's UI. Reload `identity-link-<org_id>` and read
   the Principals card, which renders the `principals` array the identity
   document carries.
10. The warning case, on a raw root instead of an identity root, through the
    CLI this time. Alice invites bob as a controller of her own person ledger,
    bob's home answers the surface as a document and signs the acceptance, and
    alice cancels the open invitation:
    ```sh
    dc exec -T alice mabel membership invite --ledger alice --by alice \
      --invitee /tmp/bob.descriptor --role controller --out /tmp/raw.bundle
    docker cp mabel-alice:/tmp/raw.bundle /tmp/raw.bundle
    docker cp /tmp/raw.bundle mabel-bob:/tmp/raw.bundle
    dc exec -T bob mabel membership accept /tmp/raw.bundle --as bob \
      --out /tmp/raw.acceptance --yes --json
    dc exec -T alice mabel membership remove --ledger alice --by alice \
      --member "$bob_id" --json
    ```
    `--yes` is required with `--json`: without it the command exits 2 with
    `confirmation_required`, having signed nothing.
11. Back in alice's UI on the `org_id` page, paste `bob_id` into
    `trust-add-subject` and click `trust-add-submit`. The shared ledger holds no
    key, so alice's key signs for it (decision 004: any single current
    controller). `trust-state-<attestation>` reads `unrevoked` and
    `identity-detail-head-seq` reads `3`. Record the event id as
    `org_attestation`.
12. Push before the ledger names a witness, which the witness refuses:
    ```sh
    dc exec -T alice sh -c 'mabel sync push --identity mabel-demo-co --to '"$witness_id"' \
      --peer "$(cat /shared/witness.ticket)" --json'
    ```
13. Name the witness on the shared ledger's own chain, in alice's UI: paste
    `$witness_id` into `witness-add-endpoint`, click `witness-add-submit`
    (`witness-add-head-seq` reads `head_seq 4`), then click
    `sync-push-submit`.
14. Verify who signed, from alice's home and from her UI:
    ```sh
    dc exec -T alice mabel verify trust --issuer mabel-demo-co \
      --subject "$bob_id" --from "$witness_id"
    ```
    In the UI, open `nav-verify`, put `org_id` in `verify-trust-issuer`,
    `bob_id` in `verify-trust-subject`, `witness_id` in `verify-trust-from`,
    and click `verify-trust-submit`.

## Verified outcomes

- Step 6 answers 200 with `ledger_id == org_id`, `declared_kind:
  "organization"`, `root: "identity"`, one entry in `controllers` whose
  `identity == alice_id` and `is_root == true`, `invitee == bob_id`, `role:
  "controller"`, `controller_on_raw_root: false` and `warning: null`, plus a
  non-empty `acceptance_base64`.
- Step 8 prints `<org_id>: 2 principals, 0 open invitations up to seq 2`, one
  `controller <alice_id> (<key>) root` line and one `controller <bob_id>
  (<key>)` line, and an `invitation ... offers controller to <bob_id>,
  accepted` line. `--json` gives `root: "identity"`, `principals` sorted by
  ascending `identity`, and `invitations[0].status == "accepted"`.
- Step 9's Principals card holds exactly two `principal-row-*` elements, one
  per principal: `principal-role-<alice_id>` and `principal-role-<bob_id>` both
  read `controller`, `principal-root-<alice_id>` is present and
  `principal-root-<bob_id>` is absent, and `principals-open-invitations` reads
  `open_invitation_count 0`.
- Step 10's accept document, the `controller-on-a-raw-root` case of
  `contracts/cli/membership-accept.json`, answers `ledger_id == alice_id`,
  `declared_kind: "person"`, `root: "raw"`, `controller_on_raw_root: true` and
  `warning` exactly: `accepting a controller role on a raw-rooted ledger means
  signing as <alice_id>: every event you append to it is that identity's own
  event`. The text form prints that sentence prefixed `warning: ` before
  anything is signed.
- Step 10's removal document answers `principal_removed: false` (bob signed an
  acceptance but nobody admitted it), `invitation_cancelled == <the step 10
  invitation event>`, `target == bob_id`, `removal_seq: 3` and `head_seq: 3`:
  alice's ledger was at seq 1 after story 001 step 7, the invitation landed at
  seq 2 and the removal at seq 3. The text form prints `removed <bob_id> at seq
  3 of <alice_id>` and `cancelled open invitation <invitation event>`.
- Replaying step 7 exits 50 with `Replay error: this acceptance was already
  admitted at seq 2 of <org_id>` and `details.reason ==
  "acceptance_already_used"`.
- Step 12 exits 30 with `details.reason == "all_witnesses_failed"`,
  `details.results[0].status == "rejected"` and `details.results[0].reject_code
  == "NOT_ADMITTED"`: a witness admits a ledger only when the pushed chain
  names it.
- Step 13's push report reads `push-status-<witness_id>` `accepted` and
  `push-stored-<witness_id>` `5`.
- Step 14 exits 0 and prints `trusted: true`, then `valid as of seq 4 of
  <org_id>, fetched from <witness_id> at <RFC 3339 UTC>; no revocation up to
  seq 4`, then `signed by principal <alice_id> (<alice active key>)`. The
  signing principal is alice, not the shared ledger.
- In the UI report, `verify-report-trusted-badge` reads `true`,
  `verify-report-signing-principal` carries `alice_id` and alice's active key,
  `verify-report-statement` is the sentence above verbatim, and
  `verify-report-subject-control` reads `subject control was not proven to this
  verifier; the issuer is responsible for out-of-band confirmation`.
- A fresh home reaches the same answer:
  `docker run --rm --network mabel_mabel --volume mabel_witness-ticket:/shared:ro
  --env MABEL_WAIT_FOR_TICKET=/shared/witness mabel:dev verify trust --issuer
  "$org_id" --subject "$bob_id" --from "$witness_id"` exits 0 with `trusted:
  true` and the same signing principal.
- Bob acts from his own home (ticket 031): `dc exec -T bob sh -c 'mabel sync
  fetch '"$org_id"' --from '"$witness_id"' --peer "$(cat
  /shared/witness.ticket)"' --json` reports `controlled_by == "<bob_id>"`;
  then `dc exec -T bob sh -c 'mabel trust add --issuer '"$org_id"' --subject
  '"$alice_id"' --peer "$(cat /shared/witness.ticket)"'` exits 0 and `dc exec
  -T bob sh -c 'mabel sync push --identity '"$org_id"' --peer "$(cat
  /shared/witness.ticket)"'` is accepted; a fresh verifier of that
  attestation names bob as the signing principal.

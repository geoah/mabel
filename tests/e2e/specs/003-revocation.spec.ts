import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  BOB_URL,
  dcSh,
  json,
  mabel,
  stdoutLines,
  verifier,
} from "../lib/docker";
import { expectExit, expectHeadSeq, story001Steps1to7 } from "../lib/stories";
import { addTrust, openAction, openIdentity, push, revokeTrust, trustCard } from "../lib/ui";

/** docs/stories/003-revocation.md */
test.describe.configure({ mode: "serial" });

const RFC3339_UTC = "\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z";
const SUBJECT_CONTROL =
  "subject control was not proven to this verifier; the issuer is responsible for out-of-band confirmation";
const VERIFIED_MEANS =
  "Verified means this identity signed this statement at this position in its chain. It is not proof that the statement is true, not proof of legal identity, and not proof of unique humanity.";

/** The RFC 3339 fetch time inside a statement, which every read moves. */
function masked(lines: string[]): string[] {
  return lines.map((line) => line.replace(/\d{4}-\d{2}-\d{2}T[\d:]+Z/, "<time>"));
}

let alicePage: Page;
let bobPage: Page;

let witnessId = "";
let aliceId = "";
let bobId = "";
let aliceAttestation = "";
let secondAttestation = "";

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  alicePage = await context.newPage();
  bobPage = await context.newPage();
});

test("step 1: story 001 steps 1 to 12, alice at seq 2 and verified", async () => {
  const state = await story001Steps1to7(alicePage, bobPage);
  witnessId = state.witnessId;
  aliceId = state.aliceId;
  bobId = state.bobId;

  await test.step("001 steps 8 and 9: one attestation in each ledger", async () => {
    await openIdentity(alicePage, ALICE_URL, aliceId);
    aliceAttestation = await addTrust(alicePage, bobId);
    await expectHeadSeq(ALICE_URL, aliceId, 2);
    await openIdentity(bobPage, BOB_URL, bobId);
    await addTrust(bobPage, aliceId);
  });

  await test.step("001 step 10: both push", async () => {
    await push(alicePage, witnessId, { stored: 1 });
    await push(bobPage, witnessId, { stored: 1 });
  });

  await test.step("001 steps 11 and 12: a fresh home answers trusted", async () => {
    const document = json(
      expectExit(
        verifier([
          "verify",
          "trust",
          "--issuer",
          aliceId,
          "--subject",
          bobId,
          "--from",
          witnessId,
          "--json",
        ]),
        0,
      ),
    );
    expect(document.trusted).toBe(true);
    expect(document.attestation_event).toBe(aliceAttestation);
  });
});

test("step 2: bob's card is in the trust list, and the entry is unrevoked", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  // Round 4 of proposal 005: who this identity trusts is a list of collapsed
  // identity cards keyed by the subject, and it sits above the record. Round 5
  // put the identity's own name in the heading, and the final round shortened
  // the line under it and gave it a testid of its own.
  await expect(alicePage.getByTestId("trust-panel")).toContainText("Who alice trusts");
  await expect(alicePage.getByTestId("trust-panel-description")).toHaveText(
    "People this identity currently trusts.",
  );
  await expect(trustCard(alicePage, bobId)).toBeVisible();

  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.trust[0].subject).toBe(bobId);
  expect(identity.body.identity.trust[0].revoked).toBe(false);
  expect(identity.body.identity.trust[0].attestation_event).toBe(aliceAttestation);
});

test("step 3: a second unrevoked attestation for one subject is refused", async () => {
  const document = json(
    expectExit(mabel("alice", ["trust", "add", "--issuer", "alice", "--subject", bobId, "--json"]), 20),
  );
  expect(document.ok).toBe(false);
  expect(document.code).toBe(20);
  expect(document.details.reason).toBe("duplicate_unrevoked_attestation");
  expect(document.details.subject).toBe(bobId);
  expect(document.details.attestation_event).toBe(aliceAttestation);
  expect(document.details.at_seq).toBe(2);
  expect(document.message).toBe(
    `Policy error: an unrevoked attestation for ${bobId} already exists at seq 2`,
  );

  // Every action starts closed (decision 017), so the form is opened before it
  // is used.
  await openAction(alicePage, "action-trust");
  await alicePage.getByTestId("trust-add-subject").fill(bobId);
  await alicePage.getByTestId("trust-add-submit").click();
  await expect(alicePage.getByTestId("trust-error")).toBeVisible();
  await expect(alicePage.getByTestId("error-code")).toHaveText("code 20");
  await expect(alicePage.getByTestId("error-status")).toHaveText("status 409");
  await expect(alicePage.getByTestId("error-code-meaning")).toHaveText(
    "A signature, the record itself or a rule refused this.",
  );
  await expect(alicePage.getByTestId("error-reason")).toHaveText("duplicate_unrevoked_attestation");
  await expect(alicePage.getByTestId("error-message")).toHaveText(
    `Policy error: an unrevoked attestation for ${bobId} already exists at seq 2`,
  );
  await expect(alicePage.getByTestId("error-detail-at_seq")).toHaveText("2");
  await expectHeadSeq(ALICE_URL, aliceId, 2);
});

test("step 4: taking trust back names the identity, and its card leaves the list", async () => {
  // The form names the identity, not the entry: `trust-revoke-submit` finds the
  // standing entry on the record this page already holds and revokes that one.
  const revocation = await revokeTrust(alicePage, bobId);
  expect(revocation).not.toBe(aliceAttestation);
  // Trust taken back is not drawn at all: it stays on the record forever, and
  // the record is where it is read.
  await expect(trustCard(alicePage, bobId)).toHaveCount(0);
  await expect(alicePage.getByTestId("trust-list-empty")).toHaveText(
    "This identity does not trust anyone yet.",
  );
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("4");
  await expectHeadSeq(ALICE_URL, aliceId, 3);

  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.trust[0].revoked).toBe(true);
  expect(identity.body.identity.trust[0].revocation_seq).toBe(3);
  expect(identity.body.identity.trust[0].attestation_event).toBe(aliceAttestation);
  // The revocation is the entry the form reported, at the head it moved to.
  const ledger = await apiGet(ALICE_URL, `/api/identities/${aliceId}/ledger?since=3&limit=1`);
  expect(ledger.body.events[0].event_id).toBe(revocation);
  expect(ledger.body.events[0].payload_kind).toBe("trust_revocation");

  // Naming an id this identity does not trust right now is refused in the form,
  // before anything is signed.
  await alicePage.getByTestId("trust-revoke-subject").fill(bobId);
  await alicePage.getByTestId("trust-revoke-submit").click();
  await expect(alicePage.getByTestId("trust-revoke-none")).toHaveText(
    "This identity does not trust that id right now, so there is nothing to take back.",
  );
  await expectHeadSeq(ALICE_URL, aliceId, 3);
});

test("steps 5 to 7: a fresh verifier reads the revocation", async () => {
  await push(alicePage, witnessId, { stored: 1 });

  const statement = new RegExp(
    `^valid as of seq 3 of ${aliceId}, fetched from ${witnessId} at ${RFC3339_UTC}; attestation ${aliceAttestation} revoked at seq 3$`,
  );

  const text = expectExit(
    verifier(["verify", "trust", "--issuer", aliceId, "--subject", bobId, "--from", witnessId]),
    0,
  );
  const lines = stdoutLines(text);
  expect(lines).toHaveLength(4);
  expect(lines[0]).toBe("trusted: false");
  expect(lines[1]).toMatch(statement);
  expect(lines[2]).toBe(SUBJECT_CONTROL);
  expect(lines[3]).toBe(VERIFIED_MEANS);
  // signing_principal is null when trusted is false.
  expect(text.stdout).not.toContain("signed by principal");

  const document = json(
    expectExit(
      verifier([
        "verify",
        "trust",
        "--issuer",
        aliceId,
        "--subject",
        bobId,
        "--from",
        witnessId,
        "--json",
      ]),
      0,
    ),
  );
  expect(document.ok).toBe(true);
  expect(document.trusted).toBe(false);
  expect(document.attestation_event).toBeNull();
  expect(document.attestation_seq).toBeNull();
  expect(document.revoked_count).toBe(1);
  expect(document.revoked_attestations[0].attestation_event).toBe(aliceAttestation);
  expect(document.revoked_attestations[0].attestation_seq).toBe(2);
  expect(document.revoked_attestations[0].revocation_seq).toBe(3);
  expect(document.head_seq).toBe(3);
  expect(document.statement).toMatch(statement);
  expect(document.statement).not.toContain("unrevoked");
  // signing_principal is null in the document too, not only in the text form.
  expect(document.signing_principal).toBeNull();
  expect(document.subject_control).toBe(SUBJECT_CONTROL);
  expect(document.verified_means).toBe(VERIFIED_MEANS);

  // Step 7: the same question from alice's own home rather than an empty one.
  // A CLI process holds no seeded witness address, so it needs --peer, and
  // only the fetch time inside the statement moves between two reads.
  const fromHome = expectExit(
    dcSh(
      "alice",
      `mabel verify trust --issuer ${aliceId} --subject ${bobId} --from ${witnessId} --peer "$(cat /shared/witness.ticket)"`,
    ),
    0,
  );
  expect(masked(stdoutLines(fromHome))).toEqual(masked(lines));
});

test("steps 8 and 9: attested again, and revocation stays history", async () => {
  await alicePage.getByTestId("nav-wallet").click();
  await openIdentity(alicePage, ALICE_URL, aliceId);
  secondAttestation = await addTrust(alicePage, bobId);
  // Bob's card is back in the list, and the entry behind it is the new one.
  await expect(trustCard(alicePage, bobId)).toBeVisible();
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("5");
  await expectHeadSeq(ALICE_URL, aliceId, 4);
  expect(secondAttestation).not.toBe(aliceAttestation);

  await push(alicePage, witnessId, { stored: 1 });

  const document = json(
    expectExit(
      verifier([
        "verify",
        "trust",
        "--issuer",
        aliceId,
        "--subject",
        bobId,
        "--from",
        witnessId,
        "--json",
      ]),
      0,
    ),
  );
  expect(document.trusted).toBe(true);
  expect(document.attestation_event).toBe(secondAttestation);
  expect(document.attestation_seq).toBe(4);
  expect(document.revoked_count).toBe(1);
  // Revocation is history, not deletion: the seq-3 revocation is still listed
  // beside the attestation that now stands.
  expect(document.revoked_attestations).toHaveLength(1);
  expect(document.revoked_attestations[0].attestation_event).toBe(aliceAttestation);
  expect(document.revoked_attestations[0].revocation_seq).toBe(3);
  expect(document.signing_principal.identity).toBe(aliceId);
  // A standing attestation keeps the plain clause; the seq-3 revocation
  // stays in revoked_attestations (revoked_count above).
  expect(document.statement).toMatch(
    new RegExp(
      `^valid as of seq 4 of ${aliceId}, fetched from ${witnessId} at ${RFC3339_UTC}; no revocation up to seq 4$`,
    ),
  );
});

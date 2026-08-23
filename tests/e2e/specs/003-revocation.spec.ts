import { expect, test, type Page } from "@playwright/test";

import { ALICE_URL, apiGet, BOB_URL, json, mabel, stdoutLines, verifier } from "../lib/docker";
import { expectExit, story001Steps1to7 } from "../lib/stories";
import { addTrust, identifier, openIdentity, push, verifyTrustInUi } from "../lib/ui";

/** docs/stories/003-revocation.md */
test.describe.configure({ mode: "serial" });

const RFC3339_UTC = "\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z";
const SUBJECT_CONTROL =
  "subject control was not proven to this verifier; the issuer is responsible for out-of-band confirmation";
const VERIFIED_MEANS =
  "Verified means this identity signed this statement at this position in its chain. It is not proof that the statement is true, not proof of legal identity, and not proof of unique humanity.";

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
    await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("2");
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

test("step 2: the attestation reads unrevoked", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  await expect(alicePage.getByTestId(`trust-row-${aliceAttestation}`)).toBeVisible();
  await expect(alicePage.getByTestId(`trust-state-${aliceAttestation}`)).toHaveText("unrevoked");
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

  await alicePage.getByTestId("trust-add-subject").fill(bobId);
  await alicePage.getByTestId("trust-add-submit").click();
  await expect(alicePage.getByTestId("trust-error")).toBeVisible();
  await expect(alicePage.getByTestId("error-code")).toHaveText("code 20");
  await expect(alicePage.getByTestId("error-status")).toHaveText("status 409");
  await expect(alicePage.getByTestId("error-code-meaning")).toHaveText(
    "cryptographic, chain or policy failure",
  );
  await expect(alicePage.getByTestId("error-reason")).toHaveText("duplicate_unrevoked_attestation");
  await expect(alicePage.getByTestId("error-message")).toHaveText(
    `Policy error: an unrevoked attestation for ${bobId} already exists at seq 2`,
  );
  await expect(alicePage.getByTestId("error-detail-at_seq")).toHaveText("2");
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("2");
});

test("step 4: the revocation names the attestation event", async () => {
  await alicePage.getByTestId(`trust-revoke-${aliceAttestation}`).click();
  await expect(alicePage.getByTestId("trust-appended-event")).toBeVisible();
  await expect(alicePage.getByTestId(`trust-state-${aliceAttestation}`)).toHaveText(
    "revoked at seq 3",
  );
  await expect(alicePage.getByTestId(`trust-revoke-${aliceAttestation}`)).toBeDisabled();
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("3");

  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.trust[0].revoked).toBe(true);
  expect(identity.body.identity.trust[0].revocation_seq).toBe(3);
  expect(identity.body.identity.trust[0].attestation_event).toBe(aliceAttestation);
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
  expect(document.statement).not.toContain("unrevoked");

  await verifyTrustInUi(alicePage, ALICE_URL, { issuer: aliceId, subject: bobId, from: witnessId });
  await expect(alicePage.getByTestId("verify-report-trusted-badge")).toHaveText("false");
  await expect(alicePage.getByTestId("verify-report-statement")).toHaveText(statement);
  await expect(alicePage.getByTestId("verify-report-revoked-count")).toHaveText("1");
  await expect(alicePage.getByTestId("verify-report-signing-principal")).toHaveText("null");
  await expect(
    alicePage
      .getByTestId("verify-report-revoked-attestations")
      .getByTestId(`verify-report-revoked-${aliceAttestation}`),
  ).toBeVisible();
});

test("steps 8 and 9: attested again, and revocation stays history", async () => {
  await alicePage.getByTestId("nav-wallet").click();
  await openIdentity(alicePage, ALICE_URL, aliceId);
  secondAttestation = await addTrust(alicePage, bobId);
  await expect(alicePage.getByTestId(`trust-state-${secondAttestation}`)).toHaveText("unrevoked");
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("4");
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
  // A standing attestation keeps the plain clause; the seq-3 revocation
  // stays in revoked_attestations (revoked_count above).
  expect(document.statement).toMatch(
    new RegExp(
      `^valid as of seq 4 of ${aliceId}, fetched from ${witnessId} at ${RFC3339_UTC}; no revocation up to seq 4$`,
    ),
  );

  await verifyTrustInUi(alicePage, ALICE_URL, { issuer: aliceId, subject: bobId, from: witnessId });
  await expect(alicePage.getByTestId("verify-report-trusted-badge")).toHaveText("true");
  await expect(alicePage.getByTestId("verify-report-attestation-seq")).toHaveText("4");
  await expect(
    alicePage.getByTestId(`verify-report-revoked-${aliceAttestation}`),
  ).toBeVisible();
  expect(await identifier(alicePage, "verify-report-attestation-event")).toBe(secondAttestation);
});

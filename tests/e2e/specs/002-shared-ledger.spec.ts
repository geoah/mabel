import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  carry,
  dcExec,
  dcSh,
  json,
  mabel,
  stdoutLines,
  verifier,
} from "../lib/docker";
import { expectExit, story002Steps1to8 } from "../lib/stories";
import { addTrust, addWitness, identifier, openIdentity, push, verifyTrustInUi } from "../lib/ui";

/** docs/stories/002-shared-ledger.md */
test.describe.configure({ mode: "serial" });

const RFC3339_UTC = "\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z";
const SUBJECT_CONTROL =
  "subject control was not proven to this verifier; the issuer is responsible for out-of-band confirmation";

let alicePage: Page;
let bobPage: Page;

let witnessId = "";
let aliceId = "";
let bobId = "";
let aliceKey = "";
let orgId = "";
let orgAttestation = "";

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  alicePage = await context.newPage();
  bobPage = await context.newPage();
});

test("steps 1 to 8: the shared ledger, invited, admitted, read back", async () => {
  const state = await story002Steps1to8(alicePage, bobPage);
  witnessId = state.witnessId;
  aliceId = state.aliceId;
  bobId = state.bobId;
  orgId = state.orgId;
  aliceKey = (await apiGet(ALICE_URL, `/api/identities/${aliceId}`)).body.identity.active_key;
});

test("step 9: the Principals card holds one row per principal", async () => {
  await openIdentity(alicePage, ALICE_URL, orgId);
  await expect(alicePage.getByTestId(/^principal-row-/)).toHaveCount(2);
  await expect(alicePage.getByTestId(`principal-role-${aliceId}`)).toHaveText("controller");
  await expect(alicePage.getByTestId(`principal-role-${bobId}`)).toHaveText("controller");
  await expect(alicePage.getByTestId(`principal-root-${aliceId}`)).toBeVisible();
  await expect(alicePage.getByTestId(`principal-root-${bobId}`)).toHaveCount(0);
  await expect(alicePage.getByTestId("principals-open-invitations")).toHaveText(
    "open_invitation_count 0",
  );
});

test("step 10: a controller role on a raw root warns before it signs", async () => {
  expectExit(
    mabel("alice", [
      "membership",
      "invite",
      "--ledger",
      "alice",
      "--by",
      "alice",
      "--invitee",
      "/tmp/bob.descriptor",
      "--role",
      "controller",
      "--out",
      "/tmp/raw.bundle",
    ]),
    0,
  );
  carry("mabel-alice", "/tmp/raw.bundle", "mabel-bob", "/tmp/raw.bundle");

  const warning = `accepting a controller role on a raw-rooted ledger means signing as ${aliceId}: every event you append to it is that identity's own event`;

  // --yes is required with --json: without it nothing is signed.
  const refused = expectExit(
    mabel("bob", [
      "membership",
      "accept",
      "/tmp/raw.bundle",
      "--as",
      "bob",
      "--out",
      "/tmp/raw.acceptance",
      "--json",
    ]),
    2,
  );
  expect(json(refused).details.reason).toBe("confirmation_required");
  // "having signed nothing": the --out file the command would have written
  // does not exist.
  expect(dcExec("bob", ["test", "!", "-f", "/tmp/raw.acceptance"]).status).toBe(0);

  const accepted = json(
    expectExit(
      mabel("bob", [
        "membership",
        "accept",
        "/tmp/raw.bundle",
        "--as",
        "bob",
        "--out",
        "/tmp/raw.acceptance",
        "--yes",
        "--json",
      ]),
      0,
    ),
  );
  expect(accepted.ledger_id).toBe(aliceId);
  expect(accepted.declared_kind).toBe("person");
  expect(accepted.root).toBe("raw");
  expect(accepted.controller_on_raw_root).toBe(true);
  expect(accepted.warning).toBe(warning);

  // The text form prints the warning prefixed `warning: `.
  const text = expectExit(
    mabel("bob", [
      "membership",
      "accept",
      "/tmp/raw.bundle",
      "--as",
      "bob",
      "--out",
      "/tmp/raw.acceptance.text",
      "--yes",
    ]),
    0,
  );
  expect(stdoutLines(text)).toContain(`warning: ${warning}`);

  const invitations = json(
    expectExit(mabel("alice", ["membership", "list", "--ledger", "alice", "--json"]), 0),
  ).invitations;
  const invitationEvent = invitations[invitations.length - 1].invitation_event;

  const removal = json(
    expectExit(
      mabel("alice", [
        "membership",
        "remove",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--member",
        bobId,
        "--json",
      ]),
      0,
    ),
  );
  expect(removal.principal_removed).toBe(false);
  expect(removal.invitation_cancelled).toBe(invitationEvent);
  expect(removal.target).toBe(bobId);
  expect(removal.removal_seq).toBe(3);
  expect(removal.head_seq).toBe(3);
});

test("replaying step 7 exits 50: the acceptance was already admitted", async () => {
  const replay = expectExit(
    mabel("alice", [
      "membership",
      "admit",
      "--ledger",
      "mabel-demo-co",
      "--by",
      "alice",
      "/tmp/acceptance.file",
      "--json",
    ]),
    50,
  );
  const document = json(replay);
  expect(document.message).toBe(
    `Replay error: this acceptance was already admitted at seq 2 of ${orgId}`,
  );
  expect(document.details.reason).toBe("acceptance_already_used");
});

test("step 11: the shared ledger attests bob, signed by alice's key", async () => {
  await openIdentity(alicePage, ALICE_URL, orgId);
  orgAttestation = await addTrust(alicePage, bobId);
  await expect(alicePage.getByTestId(`trust-state-${orgAttestation}`)).toHaveText("unrevoked");
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("3");
});

test("step 12: a witness the chain does not name refuses the push", async () => {
  const pushed = expectExit(
    dcSh(
      "alice",
      `mabel sync push --identity mabel-demo-co --to ${witnessId} --peer "$(cat /shared/witness.ticket)" --json`,
    ),
    30,
  );
  const document = json(pushed);
  expect(document.details.reason).toBe("all_witnesses_failed");
  expect(document.details.results[0].status).toBe("rejected");
  expect(document.details.results[0].reject_code).toBe("NOT_ADMITTED");
});

test("step 13: the shared ledger names the witness and is accepted", async () => {
  await openIdentity(alicePage, ALICE_URL, orgId);
  await addWitness(alicePage, witnessId, 4);
  await push(alicePage, witnessId, { stored: 5 });
});

test("step 14: a verifier is told which principal signed", async () => {
  const statement = new RegExp(
    `^valid as of seq 4 of ${orgId}, fetched from ${witnessId} at ${RFC3339_UTC}; no revocation up to seq 4$`,
  );

  const text = expectExit(
    dcSh(
      "alice",
      `mabel verify trust --issuer mabel-demo-co --subject ${bobId} --from ${witnessId} --peer "$(cat /shared/witness.ticket)"`,
    ),
    0,
  );
  const lines = stdoutLines(text);
  expect(lines[0]).toBe("trusted: true");
  expect(lines[1]).toMatch(statement);
  expect(lines[2]).toBe(`signed by principal ${aliceId} (${aliceKey})`);

  await verifyTrustInUi(alicePage, ALICE_URL, {
    issuer: orgId,
    subject: bobId,
    from: witnessId,
  });
  await expect(alicePage.getByTestId("verify-report-trusted-badge")).toHaveText("true");
  await expect(alicePage.getByTestId("verify-report-statement")).toHaveText(statement);
  expect(await identifier(alicePage, "verify-report-signing-principal")).toBe(aliceId);
  await expect(alicePage.getByTestId("verify-report-signing-principal")).toContainText(aliceKey);
  await expect(alicePage.getByTestId("verify-report-subject-control")).toHaveText(SUBJECT_CONTROL);
});

test("a fresh home reaches the same answer", async () => {
  const text = expectExit(
    verifier(["verify", "trust", "--issuer", orgId, "--subject", bobId, "--from", witnessId]),
    0,
  );
  const lines = stdoutLines(text);
  expect(lines[0]).toBe("trusted: true");
  expect(lines[2]).toBe(`signed by principal ${aliceId} (${aliceKey})`);
});

test("bob acts from his own home (ticket 031)", async () => {
  const fetched = json(
    expectExit(
      dcSh(
        "bob",
        `mabel sync fetch ${orgId} --from ${witnessId} --peer "$(cat /shared/witness.ticket)" --json`,
      ),
      0,
    ),
  );
  expect(fetched.controlled_by).toBe(bobId);

  expectExit(
    dcSh(
      "bob",
      `mabel trust add --issuer ${orgId} --subject ${aliceId} --peer "$(cat /shared/witness.ticket)"`,
    ),
    0,
  );
  const pushed = expectExit(
    dcSh("bob", `mabel sync push --identity ${orgId} --peer "$(cat /shared/witness.ticket)"`),
    0,
  );
  expect(pushed.stdout).toContain("accepted");

  const document = json(
    expectExit(
      verifier([
        "verify",
        "trust",
        "--issuer",
        orgId,
        "--subject",
        aliceId,
        "--from",
        witnessId,
        "--json",
      ]),
      0,
    ),
  );
  expect(document.trusted).toBe(true);
  expect(document.signing_principal.identity).toBe(bobId);
});

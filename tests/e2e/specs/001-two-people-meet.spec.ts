import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  BOB_URL,
  json,
  stdoutLines,
  verifier,
  WITNESS_URL,
} from "../lib/docker";
import {
  BASE32_ID,
  compareIds,
  createIdentityCli,
  expectExit,
  story001Steps1to7,
} from "../lib/stories";
import { addTrust, cardIds, identifier, openAction, openIdentity, push } from "../lib/ui";

/** docs/stories/001-two-people-meet.md */
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
let aliceKey = "";
let aliceAttestation = "";
let carolId = "";

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  alicePage = await context.newPage();
  bobPage = await context.newPage();
});

test("steps 1 to 7: two identities, one witness, both pushed", async () => {
  const state = await story001Steps1to7(alicePage, bobPage);
  witnessId = state.witnessId;
  aliceId = state.aliceId;
  bobId = state.bobId;
  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  aliceKey = identity.body.identity.active_key;
});

test("the keys action offers both secret keys, and the route answers the same", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  await openAction(alicePage, "action-keys");
  const active = alicePage.getByTestId("identity-keys-active");
  const reserve = alicePage.getByTestId("identity-keys-reserve");
  await expect(active).toHaveValue(BASE32_ID);
  await expect(reserve).toHaveValue(BASE32_ID);

  const keys = await apiGet(ALICE_URL, `/api/identities/${aliceId}/keys`);
  expect(keys.status).toBe(200);
  expect(keys.body.identity_id).toBe(aliceId);
  expect(await active.inputValue()).toBe(keys.body.active_secret_key);
  expect(await reserve.inputValue()).toBe(keys.body.reserve_secret_key);
  expect(keys.body.active_key).toBe(aliceKey);
});

test("step 8: alice attests bob", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  aliceAttestation = await addTrust(alicePage, bobId);
  await expect(alicePage.getByTestId(`trust-row-${aliceAttestation}`)).toBeVisible();
  await expect(alicePage.getByTestId(`trust-state-${aliceAttestation}`)).toHaveText(
    "trusted since position 2",
  );
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("2");

  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.trust[0].subject).toBe(bobId);
  expect(identity.body.identity.trust[0].revoked).toBe(false);
  expect(identity.body.identity.trust[0].attestation_event).toBe(aliceAttestation);
});

test("step 9: bob attests alice, in a second ledger", async () => {
  await openIdentity(bobPage, BOB_URL, bobId);
  const bobAttestation = await addTrust(bobPage, aliceId);
  await expect(bobPage.getByTestId(`trust-state-${bobAttestation}`)).toHaveText(
    "trusted since position 2",
  );
  await expect(bobPage.getByTestId("identity-detail-head-seq")).toHaveText("2");
  expect(bobAttestation).not.toBe(aliceAttestation);
});

test("step 10: both push again and the witness holds two ledgers", async () => {
  await push(alicePage, witnessId, { stored: 1 });
  await push(bobPage, witnessId, { stored: 1 });

  const ledgers = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
  expect(ledgers.body.entries).toHaveLength(2);
  expect(ledgers.body.entries.map((entry: any) => entry.ledger_id).sort()).toEqual(
    [aliceId, bobId].sort(),
  );
  for (const entry of ledgers.body.entries) {
    expect(entry.declared_kind).toBe("person");
    expect(entry.head_seq).toBe(2);
    expect(entry.event_count).toBe(3);
    expect(entry.fork_count).toBe(0);
  }
});

test("steps 11 and 12: a stranger verifies from an empty home", async () => {
  const text = expectExit(
    verifier(["verify", "trust", "--issuer", aliceId, "--subject", bobId, "--from", witnessId]),
    0,
  );
  const lines = stdoutLines(text);
  expect(lines).toHaveLength(5);
  expect(lines[0]).toBe("trusted: true");
  expect(lines[1]).toMatch(
    new RegExp(
      `^valid as of seq 2 of ${aliceId}, fetched from ${witnessId} at ${RFC3339_UTC}; no revocation up to seq 2$`,
    ),
  );
  expect(lines[2]).toBe(`signed by principal ${aliceId} (${aliceKey})`);
  expect(lines[3]).toBe(SUBJECT_CONTROL);
  expect(lines[4]).toBe(VERIFIED_MEANS);

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
  expect(document.kind).toBe("trust");
  expect(document.trusted).toBe(true);
  expect(document.subject_resolution).toBe("resolved");
  expect(document.subject_note).toBeNull();
  expect(document.attestation_event).toBe(aliceAttestation);
  expect(document.attestation_seq).toBe(2);
  expect(document.signing_principal.identity).toBe(aliceId);
  expect(document.revoked_count).toBe(0);
  expect(document.source).toBe(witnessId);
  expect(document.sources_queried).toEqual([witnessId]);
  expect(document.head_seq).toBe(2);

  // Two ledgers, two events: the mirrored verification answers the same way.
  const mirrored = json(
    expectExit(
      verifier([
        "verify",
        "trust",
        "--issuer",
        bobId,
        "--subject",
        aliceId,
        "--from",
        witnessId,
        "--json",
      ]),
      0,
    ),
  );
  expect(mirrored.trusted).toBe(true);
});

test("steps 13 and 14: the subject nobody can read", async () => {
  carolId = createIdentityCli("alice", "carol");

  await openIdentity(alicePage, ALICE_URL, aliceId);
  await addTrust(alicePage, carolId);
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("3");
  await push(alicePage, witnessId, { stored: 1 });

  const document = json(
    expectExit(
      verifier([
        "verify",
        "trust",
        "--issuer",
        aliceId,
        "--subject",
        carolId,
        "--from",
        witnessId,
        "--json",
      ]),
      0,
    ),
  );
  expect(document.trusted).toBe(true);
  expect(document.subject_resolution).toBe("unresolved");
  expect(document.subject_note).toBe("subject: unresolved (not held by any queried source)");
  expect(document.head_seq).toBe(3);
  expect(document.statement).toMatch(
    new RegExp(
      `^valid as of seq 3 of ${aliceId}, fetched from ${witnessId} at ${RFC3339_UTC}; no revocation up to seq 3$`,
    ),
  );

  // The text form prints the note as its own line, after the signing
  // principal and before the two standing sentences.
  const text = expectExit(
    verifier(["verify", "trust", "--issuer", aliceId, "--subject", carolId, "--from", witnessId]),
    0,
  );
  const lines = stdoutLines(text);
  expect(lines).toHaveLength(6);
  expect(lines[0]).toBe("trusted: true");
  expect(lines[2]).toMatch(new RegExp(`^signed by principal ${aliceId} \\([a-z2-7]{52}\\)$`));
  expect(lines[3]).toBe("subject: unresolved (not held by any queried source)");
  expect(lines[4]).toBe(SUBJECT_CONTROL);
  expect(lines[5]).toBe(VERIFIED_MEANS);

  // The witness holds no copy of the subject, which is what step 14 reported.
  const carolLedger = await apiGet(WITNESS_URL, `/api/ledgers/${carolId}`);
  expect(carolLedger.status).toBe(404);
  expect(carolLedger.body.details.reason).toBe("ledger_not_held");
});

test("the wallet home draws one card per identity, and the card is the page", async () => {
  await alicePage.goto(`${ALICE_URL}/wallet`);
  // GET /api/identities answers in ascending identity id order, and the card
  // list renders what it answered.
  expect(await cardIds(alicePage)).toEqual([aliceId, carolId].sort(compareIds));

  await expect(alicePage.getByTestId(`identity-card-name-${aliceId}-name`)).toHaveText("alice");
  await expect(alicePage.getByTestId(`identity-card-declared-kind-${aliceId}`)).toHaveText(
    "person",
  );
  await expect(alicePage.getByTestId(`identity-card-head-seq-${aliceId}`)).toHaveText(
    "at position 3",
  );
  // Carol was created and never appended to, so her card reads position 0.
  await expect(alicePage.getByTestId(`identity-card-head-seq-${carolId}`)).toHaveText(
    "at position 0",
  );
  await expect(alicePage.getByTestId(`identity-card-link-${aliceId}`)).toHaveAttribute(
    "href",
    `/identities/${aliceId}`,
  );
});

test("the identifier a spec reads is the whole value", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  expect(await identifier(alicePage, "identity-detail-identity-id")).toBe(aliceId);
});

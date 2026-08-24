import { expect, test, type BrowserContext, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  containerRunning,
  dcSh,
  docker,
  json,
  mabel,
  mustRun,
  removeExtras,
  stdoutLines,
  WITNESS_URL,
} from "../lib/docker";
import { expectExit, expectHeadSeq, startAliceTwo, story002Steps1to8 } from "../lib/stories";
import { addTrust, identifier, openAction, openIdentity, push, trustCard } from "../lib/ui";

/** docs/stories/006-stale-append.md */
test.describe.configure({ mode: "serial" });

const RFC3339_UTC = "\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z";

let context: BrowserContext;
let alicePage: Page;
let bobPage: Page;

let witnessId = "";
let aliceId = "";
let bobId = "";
let aliceKey = "";
let orgId = "";
let secondMachineEvent = "";
let losingEvent = "";

test.beforeAll(async ({ browser }) => {
  context = await browser.newContext();
  alicePage = await context.newPage();
  bobPage = await context.newPage();
});

test("step 1: story 002 steps 1 to 8, the shared ledger at seq 2", async () => {
  const state = await story002Steps1to8(alicePage, bobPage);
  witnessId = state.witnessId;
  aliceId = state.aliceId;
  bobId = state.bobId;
  orgId = state.orgId;
  aliceKey = (await apiGet(ALICE_URL, `/api/identities/${aliceId}`)).body.identity.active_key;
});

test("steps 2 and 3: the ledger names the witness, and a second machine", async () => {
  expectExit(
    mabel("alice", ["witness", "add", "--identity", "mabel-demo-co", "--endpoint", witnessId]),
    0,
  );
  expectExit(
    dcSh("alice", 'mabel sync push --identity mabel-demo-co --peer "$(cat /shared/witness.ticket)"'),
    0,
  );
  const ledger = await apiGet(WITNESS_URL, `/api/ledgers/${orgId}`);
  expect(ledger.body.entry.head_seq).toBe(3);

  await startAliceTwo();
});

test("steps 4 and 5: alice appends, the second machine wins the race", async () => {
  await openIdentity(alicePage, ALICE_URL, orgId);
  // The event alice signs here is the one the race discards, so step 7 can
  // check it is gone rather than take the head's word for it.
  losingEvent = await addTrust(alicePage, bobId);
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("5");
  await expectHeadSeq(ALICE_URL, orgId, 4);

  expectExit(
    docker([
      "exec",
      "mabel-alice-two",
      "sh",
      "-c",
      `mabel trust add --issuer mabel-demo-co --subject ${aliceId} --peer "$(cat /shared/witness.ticket)"`,
    ]),
    0,
  );
  expectExit(
    docker([
      "exec",
      "mabel-alice-two",
      "sh",
      "-c",
      'mabel sync push --identity mabel-demo-co --peer "$(cat /shared/witness.ticket)"',
    ]),
    0,
  );

  const events = await apiGet(WITNESS_URL, `/api/ledgers/${orgId}/events?since=4&limit=1`);
  secondMachineEvent = events.body.events[0].event_id;
});

test("step 6: the losing append is refused with exit code 50", async () => {
  // Every action starts closed (decision 017), so the form is opened before it
  // is used.
  await openAction(alicePage, "action-trust");
  await alicePage.getByTestId("trust-add-subject").fill(bobId);
  await alicePage.getByTestId("trust-add-submit").click();
  await expect(alicePage.getByTestId("trust-error")).toBeVisible();
  await expect(alicePage.getByTestId("error-code")).toHaveText("code 50");
  await expect(alicePage.getByTestId("error-status")).toHaveText("status 409");
  await expect(alicePage.getByTestId("error-reason")).toHaveText("stale_head");
  await expect(alicePage.getByTestId("error-message")).toHaveText(
    `State error: witness ${witnessId} reports head seq 4, this node holds seq 4`,
  );
  await expect(alicePage.getByTestId("error-code-meaning")).toHaveText(
    "Something changed this record first. Reload the page and try again.",
  );
  await expect(alicePage.getByTestId("error-detail-ledger_id")).toHaveText(orgId);
  await expect(alicePage.getByTestId("error-detail-local_head_seq")).toHaveText("4");
  await expect(alicePage.getByTestId("error-detail-observed_head_seq")).toHaveText("4");
  await expect(alicePage.getByTestId("error-detail-source")).toHaveText(witnessId);
});

test("step 7: alice's home holds the second machine's event at seq 4", async () => {
  // Round 5 of proposal 005 took the since box, the limit box and the Load
  // button off the panel: the page size is fixed at eight and nobody tunes it
  // from the screen. A refused append does not refresh the page either, so the
  // repaired chain is read on a second page opened on the same identity, which
  // leaves the form on alice's page holding what step 6 typed.
  const reader = await context.newPage();
  await reader.goto(`${ALICE_URL}/identities/${orgId}`);
  await expect(reader.getByTestId("ledger-panel")).toBeVisible();
  await expect(reader.getByTestId("ledger-event-4")).toBeVisible();
  // Proposal 005 draws the ledger as compact rows rather than a table, so a
  // line is an `li` under `ledger-events`.
  await expect(
    reader.getByTestId("ledger-events").locator('li[data-testid^="ledger-event-"]'),
  ).toHaveCount(5);
  // A closed line carries its position and the plain gloss; the raw kind, the
  // entry id and the payload are one click into the line (ticket 028, and the
  // final round of proposal 005 which moved the kind in with them).
  for (const [seq, gloss, kind] of [
    [0, "created this identity", "inception"],
    [1, "invited someone to help control this identity", "membership_invitation"],
    [2, "confirmed someone as a controller", "membership_acceptance"],
    [3, "chose who keeps a copy", "witness_config"],
    [4, "said it trusts someone", "trust_attestation"],
  ] as const) {
    await expect(reader.getByTestId(`event-gloss-${seq}`)).toHaveText(gloss);
    await expect(reader.getByTestId(`event-payload-kind-${seq}`)).toHaveCount(0);
    await reader.getByTestId(`event-expand-${seq}`).click();
    await expect(reader.getByTestId(`event-payload-kind-${seq}`)).toHaveText(kind);
    if (seq !== 4) {
      await reader.getByTestId(`event-expand-${seq}`).click();
      await expect(reader.getByTestId(`event-detail-${seq}`)).toHaveCount(0);
    }
  }
  await expect(reader.getByTestId("event-detail-4")).toBeVisible();
  expect(await identifier(reader, "event-id-4")).toBe(secondMachineEvent);
  await expect(reader.getByTestId("event-payload-4")).toHaveText(`{"subject":"${aliceId}"}`);
  // The whole record is held and it is one page of eight, so the panel counts
  // the entries and draws no pagination at all: a bar with one page on it is
  // not a choice (round 5).
  await expect(reader.getByTestId("ledger-event-count")).toHaveText("5");
  await expect(reader.getByTestId("ledger-footer")).toHaveCount(0);
  await expect(reader.getByTestId("ledger-page-1")).toHaveCount(0);
  await expect(reader.getByTestId("ledger-previous")).toHaveCount(0);
  await expect(reader.getByTestId("ledger-next")).toHaveCount(0);
  await expect(reader.getByTestId("ledger-range")).toHaveCount(0);
  await reader.close();

  // The event alice signed in step 4 appears nowhere in the ledger her home
  // now holds: the losing branch was truncated, not kept beside the winner.
  const held = await apiGet(ALICE_URL, `/api/identities/${orgId}/ledger?since=0&limit=16`);
  expect(held.body.events.map((event: any) => event.event_id)).not.toContain(losingEvent);
  expect(held.body.events).toHaveLength(5);

  const trust = json(
    expectExit(mabel("alice", ["trust", "list", "--issuer", "mabel-demo-co", "--json"]), 0),
  );
  expect(trust.head_seq).toBe(4);
  expect(trust.trust).toHaveLength(1);
  expect(trust.trust[0].subject).toBe(aliceId);
  expect(trust.trust[0].attestation_seq).toBe(4);

  // The losing event was discarded before it was ever pushed, so no fork.
  const forks = await apiGet(WITNESS_URL, "/api/forks");
  expect(forks.body.entries).toEqual([]);
  const ledger = await apiGet(WITNESS_URL, `/api/ledgers/${orgId}`);
  expect(ledger.body.entry.fork_count).toBe(0);
});

test("steps 8 and 9: the retry is the same action, run again", async () => {
  await expect(alicePage.getByTestId("trust-add-subject")).toHaveValue(bobId);
  await alicePage.getByTestId("trust-add-submit").click();
  await expect(alicePage.getByTestId("trust-appended-event")).toBeVisible();
  const attestation = await identifier(alicePage, "trust-appended-event");
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("6");
  await expectHeadSeq(ALICE_URL, orgId, 5);
  // Round 4 of proposal 005 keys the trust list by the subject, so the entry
  // this retry wrote is pinned on the identity document instead.
  await expect(trustCard(alicePage, bobId)).toBeVisible();
  const identity = await apiGet(ALICE_URL, `/api/identities/${orgId}`);
  const standing = identity.body.identity.trust.find(
    (record: any) => record.subject === bobId,
  );
  expect(standing.revoked).toBe(false);
  expect(standing.attestation_event).toBe(attestation);

  await push(alicePage, witnessId, { stored: 1 });
});

test("step 10: the second machine reads the settled chain back", async () => {
  const text = expectExit(
    docker([
      "exec",
      "mabel-alice-two",
      "sh",
      "-c",
      `mabel verify trust --issuer mabel-demo-co --subject ${bobId} --from ${witnessId} --peer "$(cat /shared/witness.ticket)"`,
    ]),
    0,
  );
  const lines = stdoutLines(text);
  expect(lines[0]).toBe("trusted: true");
  expect(lines[1]).toMatch(
    new RegExp(
      `^valid as of seq 5 of ${orgId}, fetched from ${witnessId} at ${RFC3339_UTC}; no revocation up to seq 5$`,
    ),
  );
  expect(lines[2]).toBe(`signed by principal ${aliceId} (${aliceKey})`);

  const ledger = await apiGet(WITNESS_URL, `/api/ledgers/${orgId}`);
  expect(ledger.body.entry.head_seq).toBe(5);
  expect(ledger.body.entry.event_count).toBe(6);
});

test("the same failure on the CLI is the same document", async () => {
  // A losing append repairs the chain it lost on (it truncates the local
  // branch and stores the witness's copy before returning 50), so one race
  // produces one stale_head. The race is set up again here for the CLI,
  // with subjects nothing has attested yet.
  const aliceNode = mustRun("docker", [
    "compose",
    "-f",
    "docker/compose.yaml",
    "exec",
    "-T",
    "alice",
    "mabel",
    "node",
    "id",
  ]).stdout.trim();
  const secondNode = mustRun("docker", ["exec", "mabel-alice-two", "mabel", "node", "id"]).stdout.trim();

  expectExit(
    mabel("alice", [
      "trust",
      "add",
      "--issuer",
      "mabel-demo-co",
      "--subject",
      witnessId,
      "--no-sync",
    ]),
    0,
  );
  expectExit(
    docker([
      "exec",
      "mabel-alice-two",
      "sh",
      "-c",
      `mabel trust add --issuer mabel-demo-co --subject ${secondNode} --peer "$(cat /shared/witness.ticket)" && mabel sync push --identity mabel-demo-co --peer "$(cat /shared/witness.ticket)"`,
    ]),
    0,
  );

  const document = json(
    expectExit(
      dcSh(
        "alice",
        `mabel trust add --issuer mabel-demo-co --subject ${aliceNode} --peer "$(cat /shared/witness.ticket)" --json`,
      ),
      50,
    ),
  );
  expect(document.ok).toBe(false);
  expect(document.code).toBe(50);
  expect(document.details.reason).toBe("stale_head");
});

test("step 11: the second machine comes down", async () => {
  removeExtras();
  expect(containerRunning("mabel-alice-two")).toBe(false);
});

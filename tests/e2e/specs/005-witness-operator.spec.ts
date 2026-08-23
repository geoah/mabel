import { expect, test, type Page } from "@playwright/test";

import {
  apiGet,
  composeDown,
  containerRunning,
  dcSh,
  docker,
  json,
  mabel,
  removeExtras,
  run,
  witnessId as readWitnessId,
  WITNESS_URL,
} from "../lib/docker";
import { compareIds, createIdentityCli, expectExit, story004Steps1to7 } from "../lib/stories";
import { identifier } from "../lib/ui";

/** docs/stories/005-witness-operator.md */
test.describe.configure({ mode: "serial" });

const HOLDINGS_NOTE =
  "this is what this one witness holds, a diagnostic and not an index: a ledger missing here may still exist on another witness";
const DECLARED_KIND_NOTE =
  "declared kind is advisory: it gates no authorization, no payload validity and no verification outcome";

let page: Page;

let witnessId = "";
let aliceId = "";
let orgId = "";
let firstPageIds: string[] = [];
let secondPageIds: string[] = [];

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  page = await context.newPage();
});

test("step 1: story 004 steps 1 to 7, one ledger and one fork record", async () => {
  const holdsTheFork = async () => {
    if (!containerRunning("mabel-alice-two") || !containerRunning("mabel-witness-two")) {
      return false;
    }
    try {
      const ledgers = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
      return ledgers.body.entries?.some(
        (entry: any) => entry.head_seq === 3 && entry.fork_count === 1,
      );
    } catch {
      return false;
    }
  };

  // Story 004 leaves exactly this state, and this suite runs it first. Run it
  // here when it is missing, so this story is also runnable on its own.
  if (!(await holdsTheFork())) {
    await story004Steps1to7();
  }

  witnessId = readWitnessId();
  const identities = json(expectExit(mabel("alice", ["identity", "list", "--json"]), 0)).identities;
  aliceId = identities.find((identity: any) => identity.alias === "alice").identity_id;

  const ledgers = await apiGet(WITNESS_URL, `/api/ledgers/${aliceId}`);
  expect(ledgers.body.entry.head_seq).toBe(3);
  expect(ledgers.body.entry.fork_count).toBe(1);
});

test("step 2: five ledgers on the witness, four person and one organization", async () => {
  createIdentityCli("alice", "erin");
  orgId = createIdentityCli("alice", "mabel-demo-co", [
    "--kind",
    "organization",
    "--founder",
    "alice",
  ]);
  for (const name of ["carol", "dave", "erin", "mabel-demo-co"]) {
    expectExit(mabel("alice", ["witness", "add", "--identity", name, "--endpoint", witnessId]), 0);
    expectExit(
      dcSh("alice", `mabel sync push --identity ${name} --peer "$(cat /shared/witness.ticket)"`),
      0,
    );
  }
  const ledgers = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
  expect(ledgers.body.entries).toHaveLength(5);
});

test("step 3: the Node card", async () => {
  await page.goto(`${WITNESS_URL}/witness`);
  await expect(page.getByTestId("witness-read-only-note")).toHaveText(
    "every request this route issues is a read",
  );
  await expect(page.getByTestId("witness-node-role")).toHaveText("witness");
  await expect(page.getByTestId("witness-node-relay")).toHaveText("disabled");
  expect(await identifier(page, "witness-node-endpoint-id")).toBe(witnessId);
  await expect(page.getByTestId("witness-node-ledger-count")).toHaveText("5");
  await expect(page.getByTestId("witness-node-fork-count")).toHaveText("1");
  await expect(page.getByTestId("witness-node-storage-capacity")).toHaveText("2147483648");
});

test("steps 4 to 7: one page of four, then the fifth", async () => {
  await expect(page.getByTestId("witness-ledger-offset")).toHaveText("offset 0");
  await expect(page.getByTestId("witness-ledger-limit")).toHaveText("limit 4");
  await expect(page.getByTestId("witness-ledger-more")).toHaveText("more true");
  await expect(page.getByTestId("witness-ledger-previous")).toBeDisabled();
  await expect(page.getByTestId("witness-holdings-note")).toHaveText(HOLDINGS_NOTE);
  await expect(page.getByTestId("witness-ledger-declared-kind-note")).toHaveText(
    DECLARED_KIND_NOTE,
  );

  firstPageIds = await rowIds();
  expect(firstPageIds).toHaveLength(4);
  expect(firstPageIds).toEqual([...firstPageIds].sort(compareIds));
  await assertRows(firstPageIds);

  await page.getByTestId("witness-ledger-next").click();
  await expect(page.getByTestId("witness-ledger-offset")).toHaveText("offset 4");
  await expect(page.getByTestId("witness-ledger-more")).toHaveText("more false");
  await expect(page.getByTestId("witness-ledger-next")).toBeDisabled();
  await expect(page.getByTestId("witness-ledger-previous")).toBeEnabled();
  secondPageIds = await rowIds();
  expect(secondPageIds).toHaveLength(1);
  await assertRows(secondPageIds);

  // Every ledger the witness holds was seen exactly once, in the one order
  // that makes paging stable.
  const all = [...firstPageIds, ...secondPageIds];
  const ledgers = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
  expect(all).toEqual(ledgers.body.entries.map((entry: any) => entry.ledger_id));
  expect(all).toContain(aliceId);
  expect(all).toContain(orgId);

  await page.getByTestId("witness-ledger-previous").click();
  await expect(page.getByTestId("witness-ledger-offset")).toHaveText("offset 0");
  expect(await rowIds()).toEqual(firstPageIds);
});

/** The event rows of the events table, which the cells share a prefix with. */
function eventRows() {
  return page.getByTestId("witness-events-table").locator('tr[data-testid^="witness-event-"]');
}

/** The ledger ids of the rows on the page now shown, in the order shown. */
async function rowIds(): Promise<string[]> {
  await expect(page.getByTestId("witness-ledger-table")).toBeVisible();
  const rows = page.locator('[data-testid^="witness-ledger-row-"]');
  const ids: string[] = [];
  for (const testId of await rows.evaluateAll((elements) =>
    elements.map((element) => element.getAttribute("data-testid") ?? ""),
  )) {
    ids.push(testId.replace("witness-ledger-row-", ""));
  }
  return ids;
}

/** Steps 6 and 7, asserted per visible row: the kind and the fork count. */
async function assertRows(ids: string[]): Promise<void> {
  for (const id of ids) {
    await expect(page.getByTestId(`witness-ledger-declared-kind-${id}`)).toHaveText(
      id === orgId ? "organization" : "person",
    );
    await expect(page.getByTestId(`witness-ledger-fork-count-${id}`)).toHaveText(
      id === aliceId ? "1" : "0",
    );
    await expect(page.getByTestId(`witness-ledger-forks-truncated-${id}`)).toHaveText(
      "forks_truncated false",
    );
  }
}

test("step 8: one ledger's summary and one page of its events", async () => {
  if (!firstPageIds.includes(aliceId)) {
    await page.getByTestId("witness-ledger-next").click();
    await expect(page.getByTestId("witness-ledger-offset")).toHaveText("offset 4");
  }
  await page.getByTestId(`witness-ledger-link-${aliceId}`).click();
  await expect(page.getByTestId("witness-ledger-detail")).toBeVisible();

  await expect(page.getByTestId("witness-detail-head-seq")).toHaveText("3");
  await expect(page.getByTestId("witness-detail-event-count")).toHaveText("4");
  await expect(page.getByTestId("witness-detail-fork-count")).toHaveText("1");
  await expect(page.getByTestId("witness-detail-forks-truncated")).toHaveText("false");
  await expect(
    page.getByTestId("witness-detail-witnesses").locator("[data-value]"),
  ).toHaveCount(2);
  expect(await identifier(page, "witness-detail-source-endpoint")).toMatch(/^[a-z2-7]{52}$/);

  await page.getByTestId("witness-events-since").fill("2");
  await page.getByTestId("witness-events-limit").fill("1");
  await page.getByTestId("witness-events-load").click();
  await expect(page.getByTestId("witness-events-page-since")).toHaveText("2");
  await expect(page.getByTestId("witness-events-page-limit")).toHaveText("1");
  await expect(page.getByTestId("witness-events-more")).toHaveText("true");
  await expect(page.getByTestId("witness-event-2")).toBeVisible();
  await expect(eventRows()).toHaveCount(1);

  await page.getByTestId("witness-events-since").fill("0");
  await page.getByTestId("witness-events-limit").fill("8");
  await page.getByTestId("witness-events-load").click();
  await expect(eventRows()).toHaveCount(4);
  for (const [seq, kind] of [
    [0, "inception"],
    [1, "witness_config"],
    [2, "witness_config"],
    [3, "trust_attestation"],
  ] as const) {
    await expect(page.getByTestId(`witness-event-payload-kind-${seq}`)).toHaveText(kind);
  }
});

test("step 9: the Forks card is filtered to this ledger", async () => {
  await expect(page.getByTestId("witness-forks-filter")).toBeVisible();
  expect(await identifier(page, "witness-forks-filter")).toBe(aliceId);
  await expect(page.getByTestId(`fork-record-${aliceId}-3`)).toBeVisible();
  await expect(page.locator('[data-testid^="fork-record-"]')).toHaveCount(1);
  await expect(page.getByTestId("witness-forks-more")).toHaveText("more false");

  await page.goto(`${WITNESS_URL}/witness/ledgers/${orgId}`);
  await expect(page.getByTestId("witness-forks-empty")).toHaveText(
    "this witness recorded no fork for this ledger",
  );
});

test("step 10: every write is refused and nothing changed", async () => {
  const before = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
  const forksBefore = await apiGet(WITNESS_URL, "/api/forks");

  const post = curl([
    "-i",
    "-X",
    "POST",
    "-H",
    "Origin: http://127.0.0.1:9080",
    "-H",
    "Content-Type: application/json",
    "--data",
    "{}",
    `${WITNESS_URL}/api/ledgers`,
  ]);
  expect(post.status).toBe(405);
  expect(post.body.code).toBe(2);
  expect(post.body.message).toBe("POST is not allowed on /api/ledgers");
  expect(post.body.details.reason).toBe("method_not_allowed");

  const trust = curl([
    "-i",
    "-X",
    "POST",
    "-H",
    "Origin: http://127.0.0.1:9080",
    "-H",
    "Content-Type: application/json",
    "--data",
    "{}",
    `${WITNESS_URL}/api/trust`,
  ]);
  expect(trust.status).toBe(404);
  expect(trust.body.message).toBe("no route for POST /api/trust");
  expect(trust.body.details.reason).toBe("unknown_route");

  const evil = curl(["-i", "-H", "Host: evil.example", `${WITNESS_URL}/api/node`]);
  expect(evil.status).toBe(403);
  expect(evil.body.code).toBe(2);
  expect(evil.body.details.reason).toBe("host_not_loopback");
  expect(evil.body.message).toBe(
    "request rejected: Host header must be 127.0.0.1:9080 or localhost:9080",
  );

  const after = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
  const summary = (body: any) =>
    body.entries.map((entry: any) => ({
      ledger_id: entry.ledger_id,
      head_seq: entry.head_seq,
      head_event: entry.head_event,
      event_count: entry.event_count,
      fork_count: entry.fork_count,
    }));
  expect(summary(after.body)).toEqual(summary(before.body));
  expect(summary(after.body)).toHaveLength(5);

  const forksAfter = await apiGet(WITNESS_URL, "/api/forks");
  expect(forksAfter.body.entries).toHaveLength(1);
  expect(forksAfter.body.entries[0].kept.event_id).toBe(forksBefore.body.entries[0].kept.event_id);
  expect(forksAfter.body.entries[0].conflicting.event_id).toBe(
    forksBefore.body.entries[0].conflicting.event_id,
  );
});

/** One curl request, split into its status code and its JSON body. */
function curl(args: string[]): { status: number; body: any } {
  const result = run("curl", ["-sS", ...args]);
  expect(result.status, result.stderr).toBe(0);
  const [head, body] = splitResponse(result.stdout);
  const match = /^HTTP\/[\d.]+ (\d{3})/.exec(head);
  if (!match) {
    throw new Error(`no status line in\n${result.stdout}`);
  }
  return { status: Number(match[1]), body: JSON.parse(body) };
}

function splitResponse(response: string): [string, string] {
  const separator = response.indexOf("\r\n\r\n");
  if (separator === -1) {
    throw new Error(`no header separator in\n${response}`);
  }
  return [response.slice(0, separator), response.slice(separator + 4)];
}

test("step 11: the hand-started containers and the topology come down", async () => {
  removeExtras();
  expect(containerRunning("mabel-alice-two")).toBe(false);
  expect(containerRunning("mabel-witness-two")).toBe(false);
  expect(docker(["volume", "inspect", "mabel-alice-second"]).status).not.toBe(0);
  composeDown();
});

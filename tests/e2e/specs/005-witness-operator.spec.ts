import { expect, test, type Page } from "@playwright/test";

import {
  apiGet,
  composeDown,
  containerRunning,
  dcExec,
  dcSh,
  docker,
  json,
  mabel,
  removeExtras,
  run,
  witnessId as readWitnessId,
  WITNESS_URL,
} from "../lib/docker";
import {
  compareIds,
  createIdentityCli,
  expectExit,
  readNodePage,
  story004Steps1to7,
} from "../lib/stories";
import { cardIds, identifier } from "../lib/ui";

/** docs/stories/005-witness-operator.md */
test.describe.configure({ mode: "serial" });

const HOLDINGS_NOTE =
  "This is what this one witness holds. A record missing here may still be on another witness.";
const READ_ONLY_NOTE = "This page only reads. Nothing here changes anything.";

let page: Page;

let witnessId = "";
let witnessTwoId = "";
let aliceNodeId = "";
let aliceId = "";
let orgId = "";

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
  // Both witnesses publish to the shared volume, so the second one's id is
  // readable whether this suite ran story 004 just now or inherited its state.
  witnessTwoId = expectExit(dcExec("alice", ["cat", "/shared/witness-two.id"]), 0).stdout.trim();
  // The endpoint that pushed alice's ledger to this witness: the second
  // machine's push of the conflicting branch was rejected, so it is alice's.
  aliceNodeId = expectExit(mabel("alice", ["node", "id"]), 0).stdout.trim();
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

test("step 3: the node facts, on the route and on the node page", async () => {
  const node = await apiGet(WITNESS_URL, "/api/node");
  expect(node.body.role).toBe("witness");
  expect(node.body.relay).toBe("disabled");
  expect(node.body.endpoint_id).toBe(witnessId);
  expect(node.body.ledger_count).toBe(5);
  expect(node.body.fork_count).toBe(1);
  expect(node.body.storage_capacity).toBe(2147483648);

  // Round 4 of proposal 005 gave the facts a page again. A witness serves no
  // wallet, so its nav names the records it keeps and the program keeping them.
  await page.goto(`${WITNESS_URL}/witness`);
  await expect(page.getByTestId("nav-witness")).toBeVisible();
  await expect(page.getByTestId("nav-node")).toBeVisible();
  await expect(page.locator('header [data-testid^="nav-"]')).toHaveCount(2);
  await readNodePage(page, WITNESS_URL, { role: "witness", endpointId: witnessId });
});

test("step 4: five ledgers as five cards, on one page and in id order", async () => {
  await page.goto(`${WITNESS_URL}/witness`);
  await expect(page.getByTestId("witness-read-only-note")).toHaveText(READ_ONLY_NOTE);
  await expect(page.getByTestId("witness-holdings-note")).toHaveText(HOLDINGS_NOTE);
  // The three holdings filters belong to the wallet's drill-in, where "yours"
  // and "trusted" mean something: a witness serves no wallet, so this route
  // draws one flat list and no filter at all. Story 007 step 13 pins them.
  for (const filter of ["all", "ours", "trusted"] as const) {
    await expect(page.getByTestId(`witness-holdings-${filter}`)).toHaveCount(0);
  }

  // Every ledger the witness holds is drawn, in ascending ledger id order,
  // with no paging control anywhere: the route asks for all of them at once.
  const ids = await cardIds(page);
  expect(ids).toHaveLength(5);
  expect(ids).toEqual([...ids].sort(compareIds));
  expect(ids).toContain(aliceId);
  expect(ids).toContain(orgId);

  const ledgers = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=256");
  expect(ids).toEqual(ledgers.body.entries.map((entry: any) => entry.ledger_id));
});

test("steps 5 to 7: the declared kind of every card, and the one fork count", async () => {
  for (const id of await cardIds(page)) {
    await expect(page.getByTestId(`identity-card-declared-kind-${id}`)).toHaveText(
      id === orgId ? "organization" : "person",
    );
  }
  // A fork count is drawn on a card only when the witness recorded one, so
  // alice's card carries it and no other card has the element at all.
  await expect(page.getByTestId(`identity-card-fork-count-${aliceId}`)).toHaveText("1 conflict");
  await expect(page.locator('[data-testid^="identity-card-fork-count-"]')).toHaveCount(1);

  // Paging left the UI with the operator table, so the route is where offset,
  // limit and more are pinned. Two requests cover every ledger exactly once,
  // in the one order that makes paging stable.
  const first = await apiGet(WITNESS_URL, "/api/ledgers?offset=0&limit=4");
  expect(first.body.offset).toBe(0);
  expect(first.body.limit).toBe(4);
  expect(first.body.more).toBe(true);
  expect(first.body.entries).toHaveLength(4);
  const second = await apiGet(WITNESS_URL, "/api/ledgers?offset=4&limit=4");
  expect(second.body.offset).toBe(4);
  expect(second.body.more).toBe(false);
  expect(second.body.entries).toHaveLength(1);

  const paged = [...first.body.entries, ...second.body.entries].map(
    (entry: any) => entry.ledger_id,
  );
  expect(paged).toEqual([...paged].sort(compareIds));
  expect(paged).toEqual(await cardIds(page));
});

test("step 8: one ledger's summary and its chain, on the identity page", async () => {
  await page.getByTestId(`identity-card-link-${aliceId}`).click();
  await expect(page.getByTestId("witness-ledger-detail")).toBeVisible();
  // The page names itself, and the nav is the one way back: the final round of
  // proposal 005 removed the page's own back link.
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("This record");
  await expect(page.getByTestId("witness-ledger-back")).toHaveCount(0);

  expect(await identifier(page, "witness-detail-ledger-id")).toBe(aliceId);
  await expect(page.getByTestId("witness-detail-declared-kind")).toHaveText("person");
  await expect(page.getByTestId("witness-detail-head-seq")).toHaveText("3");
  await expect(page.getByTestId("witness-detail-event-count")).toHaveText("4");
  await expect(page.getByTestId("witness-detail-fork-count")).toHaveText("1");
  // Proposal 005 removed the declared-kind advisory sentence outright. The row
  // above still says which kind was declared, and nothing repeats a disclaimer
  // beside it.
  await expect(page.getByTestId("witness-detail-declared-kind-note")).toHaveCount(0);
  await expect(page.getByTestId("witness-detail-holdings-note")).toHaveText(HOLDINGS_NOTE);
  // The two endpoints alice's chain names, by value: witness one and witness
  // two, in whichever order the rendered list holds them.
  const witnesses = await page
    .getByTestId("witness-detail-witnesses")
    .locator("[data-value]")
    .evaluateAll((elements) => elements.map((element) => element.getAttribute("data-value") ?? ""));
  expect([...witnesses].sort(compareIds)).toEqual([witnessId, witnessTwoId].sort(compareIds));
  expect(await identifier(page, "witness-detail-source-endpoint")).toBe(aliceNodeId);

  // The chain renders through the wallet's own ledger component: one line per
  // event, each opening into the event it records.
  await expect(page.getByTestId("ledger-event-count")).toHaveText("4");
  await expect(page.getByTestId("ledger-head-seq")).toHaveText("3");
  // Proposal 005 draws the ledger as compact rows rather than a table, so a
  // line is an `li` under `ledger-events`.
  await expect(
    page.getByTestId("ledger-events").locator('li[data-testid^="ledger-event-"]'),
  ).toHaveCount(4);
  // The final round of proposal 005 left the closed line the position and the
  // plain gloss only: the raw kind string moved inside the opened entry, so a
  // spec that wants it opens the line first.
  for (const [seq, gloss, kind] of [
    [0, "created this identity", "inception"],
    [1, "chose who keeps a copy", "witness_config"],
    [2, "chose who keeps a copy", "witness_config"],
    [3, "said it trusts someone", "trust_attestation"],
  ] as const) {
    await expect(page.getByTestId(`event-seq-${seq}`)).toHaveText(String(seq));
    await expect(page.getByTestId(`event-gloss-${seq}`)).toHaveText(gloss);
    await expect(page.getByTestId(`event-payload-kind-${seq}`)).toHaveCount(0);
    await page.getByTestId(`event-expand-${seq}`).click();
    await expect(page.getByTestId(`event-payload-kind-${seq}`)).toHaveText(kind);
    // Every line but the last closes again, so the one open entry step 8 reads
    // its id from is the one it opened for that.
    if (seq !== 3) {
      await page.getByTestId(`event-expand-${seq}`).click();
      await expect(page.getByTestId(`event-detail-${seq}`)).toHaveCount(0);
    }
  }
  await expect(page.getByTestId("event-detail-3")).toBeVisible();
  expect(await identifier(page, "event-id-3")).toBe(
    (await apiGet(WITNESS_URL, `/api/ledgers/${aliceId}`)).body.entry.head_event,
  );

  // The event form left the UI with the paging controls, so `since` and
  // `limit` are pinned on the route the panel reads: since is inclusive.
  const events = await apiGet(WITNESS_URL, `/api/ledgers/${aliceId}/events?since=2&limit=1`);
  expect(events.body.since).toBe(2);
  expect(events.body.limit).toBe(1);
  expect(events.body.more).toBe(true);
  expect(events.body.events).toHaveLength(1);
  expect(events.body.events[0].seq).toBe(2);
});

test("step 9: the Forks card holds this ledger's record and no other", async () => {
  await expect(page.getByTestId("witness-forks")).toBeVisible();
  await expect(page.getByTestId(`fork-record-${aliceId}-3`)).toBeVisible();
  await expect(page.locator('[data-testid^="fork-record-"]')).toHaveCount(1);

  // A ledger with no fork record draws no Forks card at all.
  await page.goto(`${WITNESS_URL}/witness/ledgers/${orgId}`);
  await expect(page.getByTestId("witness-ledger-detail")).toBeVisible();
  await expect(page.getByTestId("witness-forks")).toHaveCount(0);
  const forks = await apiGet(WITNESS_URL, `/api/forks?ledger_id=${orgId}`);
  expect(forks.body.entries).toEqual([]);
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

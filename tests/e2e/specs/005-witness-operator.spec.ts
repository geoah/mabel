import { expect, test, type Page } from "@playwright/test";

import {
  apiGet,
  composeDown,
  containerRunning,
  dcSh,
  docker,
  json,
  mabel,
  mustRun,
  removeExtras,
  run,
  witnessOf,
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

/** The standing note the known list carries, which came off the witness route. */
const HOLDINGS_NOTE =
  "This is what this home holds. A record missing here may still be on another witness.";
/** A 64-character hex endpoint id, which is the shape a pre-proposal-006 node.json holds. */
const LEGACY_ENDPOINT = "9b7ba6acb9804b63ae69a67ed1ff1bc9ec7336f282757bfe1230cb5f8180eb0a";
/** The volume the migration check builds and throws away. */
const LEGACY_VOLUME = "mabel-legacy-home";

let page: Page;

let witnessId = "";
let witnessIdentity = "";
let witnessTwoIdentity = "";
let aliceNodeId = "";
let aliceId = "";
let orgId = "";

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  page = await context.newPage();
});

test("step 1: story 004 steps 1 to 7, one record and one conflict", async () => {
  const holdsTheFork = async () => {
    if (!containerRunning("mabel-alice-two")) {
      return false;
    }
    try {
      const forks = await apiGet(WITNESS_URL, "/api/forks");
      return forks.body.entries?.length === 1;
    } catch {
      return false;
    }
  };

  // Story 004 leaves exactly this state, and this suite runs it first. Run it
  // here when it is missing, so this story is also runnable on its own.
  if (!(await holdsTheFork())) {
    await story004Steps1to7();
  }

  const witness = witnessOf("witness");
  witnessId = witness.endpointId;
  witnessIdentity = witness.identity;
  witnessTwoIdentity = witnessOf("witness-two").identity;
  // The machine that pushed alice's record to this witness: the second
  // machine's push of the conflicting branch was rejected, so it is alice's.
  aliceNodeId = expectExit(mabel("alice", ["node", "id"]), 0).stdout.trim();
  const identities = json(expectExit(mabel("alice", ["identity", "list", "--json"]), 0)).identities;
  aliceId = identities.find((identity: any) => identity.alias === "alice").identity_id;

  const held = await apiGet(WITNESS_URL, `/api/identities/${aliceId}`);
  expect(held.body.identity.head_seq).toBe(3);
  const forks = await apiGet(WITNESS_URL, `/api/forks?ledger_id=${aliceId}`);
  expect(forks.body.entries).toHaveLength(1);
});

test("step 2: five records on the witness, four person and one organization", async () => {
  createIdentityCli("alice", "erin");
  orgId = createIdentityCli("alice", "mabel-demo-co", [
    "--kind",
    "organization",
    "--founder",
    "alice",
  ]);
  for (const name of ["carol", "dave", "erin", "mabel-demo-co"]) {
    // The set names the witness identity; the push dials the endpoint.
    expectExit(
      mabel("alice", ["witness", "add", "--identity", name, "--witness", witnessIdentity]),
      0,
    );
    expectExit(
      dcSh("alice", `mabel sync push --identity ${name} --peer "$(cat /shared/witness.ticket)"`),
      0,
    );
  }
  // A witness's holdings are the records it stores and does not sign for, so
  // its own witness identity is not one of them.
  const known = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=256");
  expect(known.body.identities).toHaveLength(5);
  expect(known.body.identities.map((row: any) => row.identity_id)).not.toContain(witnessIdentity);
});

test("step 3: the node facts, on the route and on the node page", async () => {
  const node = await apiGet(WITNESS_URL, "/api/node");
  expect(node.body).not.toHaveProperty("role");
  expect(node.body.relay).toBe("disabled");
  expect(node.body.endpoint_id).toBe(witnessId);
  // Six records: the five it keeps for other people and its own.
  expect(node.body.ledger_count).toBe(6);
  expect(node.body.identity_count).toBe(1);
  expect(node.body.fork_count).toBe(1);
  expect(node.body.storage_capacity).toBe(2147483648);

  // Every node serves the same three nav entries and the same home. A node
  // that keeps other people's records is not a different program.
  await page.goto(`${WITNESS_URL}/wallet`);
  await expect(page.getByTestId("nav-wallet")).toBeVisible();
  await expect(page.getByTestId("nav-witnesses")).toBeVisible();
  await expect(page.getByTestId("nav-node")).toBeVisible();
  await expect(page.getByTestId("nav-witness")).toHaveCount(0);
  await expect(page.locator('header [data-testid^="nav-"]')).toHaveCount(3);
  await readNodePage(page, WITNESS_URL, {
    endpointId: witnessId,
    witnessFor: [witnessIdentity],
  });
});

test("step 4: five records as five cards on the witness's own home", async () => {
  await page.goto(`${WITNESS_URL}/wallet`);
  // The one identity this home signs for is the witness identity it minted,
  // and it keeps its own record like any other.
  expect(await cardIds(page)).toEqual([witnessIdentity]);

  // Everything else it holds is a known identity: a record it has and does not
  // control. The note that used to sit on the witness route sits here.
  await expect(page.getByTestId("known-identities-note")).toHaveText(HOLDINGS_NOTE);
  const ids = await cardIds(page, "known-identity-cards");
  expect(ids).toHaveLength(5);
  // `known` sorts by the rendered id, which orders the digits before the
  // letters, and the list renders what the route answered.
  expect(ids).toEqual([...ids].sort());
  expect(ids).toContain(aliceId);
  expect(ids).toContain(orgId);

  const known = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=256");
  expect(ids).toEqual(known.body.identities.map((row: any) => row.identity_id));
});

test("steps 5 to 7: the declared kind of every record, and the paged route", async () => {
  const known = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=256");
  for (const row of known.body.identities) {
    expect(row.declared_kind).toBe(row.identity_id === orgId ? "organization" : "person");
    expect(row.stored).toBe(true);
  }

  // Paging is a fact of the route, and two requests cover every record exactly
  // once, in the one order that makes paging stable.
  const first = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=4");
  expect(first.body.offset).toBe(0);
  expect(first.body.limit).toBe(4);
  expect(first.body.more).toBe(true);
  expect(first.body.identities).toHaveLength(4);
  const second = await apiGet(WITNESS_URL, "/api/identities/known?offset=4&limit=4");
  expect(second.body.offset).toBe(4);
  expect(second.body.more).toBe(false);
  expect(second.body.identities).toHaveLength(1);

  const paged = [...first.body.identities, ...second.body.identities].map(
    (row: any) => row.identity_id,
  );
  expect(paged).toEqual([...paged].sort());
  expect(paged).toEqual(known.body.identities.map((row: any) => row.identity_id));
});

test("step 8: one record's summary and its chain, on the identity page", async () => {
  // A record is a record: the witness draws the same identity page a wallet
  // draws, opened from the same card (proposal 006 section 8).
  await page.goto(`${WITNESS_URL}/wallet`);
  await page.getByTestId(`identity-card-link-${aliceId}`).click();
  await expect(page).toHaveURL(`${WITNESS_URL}/identities/${aliceId}`);
  await expect(page.getByTestId("identity-detail")).toBeVisible();

  expect(await identifier(page, "identity-detail-resolved")).toBe(aliceId);
  await expect(page.getByTestId("identity-detail-declared-kind")).toHaveText("person");
  await expect(page.getByTestId("identity-detail-event-count")).toHaveText("4");
  // This home holds no key for alice, so the page carries no actions at all.
  await expect(page.getByTestId("identity-actions")).toHaveCount(0);

  const identity = await apiGet(WITNESS_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.head_seq).toBe(3);
  expect(identity.body.identity.event_count).toBe(4);
  expect([...identity.body.identity.witnesses].sort(compareIds)).toEqual(
    [witnessIdentity, witnessTwoIdentity].sort(compareIds),
  );

  // The chain renders through the same ledger component: one line per event,
  // each opening into the event it records.
  await expect(page.getByTestId("ledger-event-count")).toHaveText("4");
  await expect(
    page.getByTestId("ledger-events").locator('li[data-testid^="ledger-event-"]'),
  ).toHaveCount(4);
  // A closed line carries its position and the plain gloss only: the raw kind
  // string is one click into the line.
  for (const [seq, gloss, kind] of [
    [0, "created this identity", "inception"],
    [1, "chose who keeps a copy", "witness_set"],
    [2, "chose who keeps a copy", "witness_set"],
    [3, "said it trusts someone", "trust_attestation"],
  ] as const) {
    await expect(page.getByTestId(`event-seq-${seq}`)).toHaveText(String(seq));
    await expect(page.getByTestId(`event-gloss-${seq}`)).toHaveText(gloss);
    await expect(page.getByTestId(`event-payload-kind-${seq}`)).toHaveCount(0);
    await page.getByTestId(`event-expand-${seq}`).click();
    await expect(page.getByTestId(`event-payload-kind-${seq}`)).toHaveText(kind);
    if (seq !== 3) {
      await page.getByTestId(`event-expand-${seq}`).click();
      await expect(page.getByTestId(`event-detail-${seq}`)).toHaveCount(0);
    }
  }
  await expect(page.getByTestId("event-detail-3")).toBeVisible();
  expect(await identifier(page, "event-id-3")).toBe(identity.body.identity.head_event);

  // The event form left the UI with the paging controls, so `since` and
  // `limit` are pinned on the route the panel reads: since is inclusive.
  const events = await apiGet(WITNESS_URL, `/api/identities/${aliceId}/ledger?since=2&limit=1`);
  expect(events.body.since).toBe(2);
  expect(events.body.limit).toBe(1);
  expect(events.body.more).toBe(true);
  expect(events.body.events).toHaveLength(1);
  expect(events.body.events[0].seq).toBe(2);
});

test("step 9: the conflict this witness recorded, and the one that has none", async () => {
  // The witness detail screen and its Forks card went with the witness routes,
  // so a conflict is read where it is recorded (proposal 006 section 8).
  const forks = await apiGet(WITNESS_URL, `/api/forks?ledger_id=${aliceId}`);
  expect(forks.body.entries).toHaveLength(1);
  expect(forks.body.entries[0].seq).toBe(3);
  expect(forks.body.entries[0].source_endpoint).not.toBe(aliceNodeId);

  const none = await apiGet(WITNESS_URL, `/api/forks?ledger_id=${orgId}`);
  expect(none.body.entries).toEqual([]);

  const all = await apiGet(WITNESS_URL, "/api/forks?offset=0&limit=64");
  expect(all.body.entries).toHaveLength(1);
  expect(all.body.more).toBe(false);
});

test("step 10: the witness routes are gone and a stranger is still refused", async () => {
  const before = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=256");
  const forksBefore = await apiGet(WITNESS_URL, "/api/forks");

  // `/api/ledgers` was the witness's own read-only route. One node serves one
  // API now, so the path is not a route at all, for any method.
  for (const method of ["GET", "POST"] as const) {
    const answer = curl([
      "-i",
      "-X",
      method,
      "-H",
      "Origin: http://127.0.0.1:9080",
      "-H",
      "Content-Type: application/json",
      ...(method === "POST" ? ["--data", "{}"] : []),
      `${WITNESS_URL}/api/ledgers`,
    ]);
    expect(answer.status).toBe(404);
    expect(answer.body.code).toBe(2);
    expect(answer.body.message).toBe(`no route for ${method} /api/ledgers`);
    expect(answer.body.details.reason).toBe("unknown_route");
  }

  const evil = curl(["-i", "-H", "Host: evil.example", `${WITNESS_URL}/api/node`]);
  expect(evil.status).toBe(403);
  expect(evil.body.code).toBe(2);
  expect(evil.body.details.reason).toBe("host_not_loopback");
  expect(evil.body.message).toBe(
    "request rejected: Host header must be 127.0.0.1:9080 or localhost:9080",
  );

  const after = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=256");
  const summary = (body: any) =>
    body.identities.map((row: any) => ({
      identity_id: row.identity_id,
      head_seq: row.head_seq,
      declared_kind: row.declared_kind,
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

test("step 11: an old home is refused, and the entrypoint's rewrite starts clean", async () => {
  // A `node.json` written before witnesses were identities holds 64-character
  // hex endpoint ids under `witnesses`. A base32 identity id is 52, so the
  // loader tells the two apart and refuses rather than misreading one
  // (proposal 006 section 5.4).
  docker(["volume", "rm", "-f", LEGACY_VOLUME]);
  mustRun("docker", ["volume", "create", LEGACY_VOLUME]);
  const legacy = JSON.stringify({
    role: "witness",
    http_bind: "0.0.0.0:9080",
    witnesses: [LEGACY_ENDPOINT],
    storage_capacity: 2147483648,
    relay: "disabled",
  });
  mustRun("docker", [
    "run",
    "--rm",
    "--volume",
    `${LEGACY_VOLUME}:/data`,
    "--entrypoint",
    "sh",
    "mabel:dev",
    "-c",
    `mabel node id >/dev/null && printf '%s\\n' '${legacy}' > /data/node.json`,
  ]);

  // Started past the entrypoint, so nothing rewrites the file first.
  const refused = docker([
    "run",
    "--rm",
    "--volume",
    `${LEGACY_VOLUME}:/data`,
    "--entrypoint",
    "mabel",
    "mabel:dev",
    "serve",
    "--http",
    "127.0.0.1:9099",
    "--iroh-port",
    "9098",
  ]);
  expect(refused.status).toBe(10);
  const said = `${refused.stdout}${refused.stderr}`;
  expect(said).toContain(
    `node.json names the endpoint id ${LEGACY_ENDPOINT} under witnesses, which proposal 006 replaced with {"identity", "endpoints"} objects`,
  );
  // The message names the command that fixes it.
  expect(said).toContain(
    "run mabel witness set-default --witness <mabel-id> --endpoints <endpoint,...>",
  );

  // The compose entrypoint writes node.json on every start, before anything
  // loads it, so the same volume starts clean through the image's entrypoint.
  const started = docker(["run", "--rm", "--volume", `${LEGACY_VOLUME}:/data`, "mabel:dev", "node", "id"]);
  expect(started.status, `${started.stdout}${started.stderr}`).toBe(0);
  const rewritten = json(
    docker(["run", "--rm", "--volume", `${LEGACY_VOLUME}:/data`, "--entrypoint", "cat", "mabel:dev", "/data/node.json"]),
  );
  expect(rewritten).not.toHaveProperty("role");
  expect(rewritten).not.toHaveProperty("accept_legacy_witness_config");
  expect(rewritten.witnesses).toEqual([]);
  expect(rewritten.witness_for).toEqual([]);
  docker(["volume", "rm", "-f", LEGACY_VOLUME]);
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

test("step 12: the hand-started container and the topology come down", async () => {
  removeExtras();
  expect(containerRunning("mabel-alice-two")).toBe(false);
  expect(docker(["volume", "inspect", "mabel-alice-second"]).status).not.toBe(0);
  composeDown();
  // The second witness is a compose service now, so `down -v` removed it too.
  expect(containerRunning("mabel-witness-two")).toBe(false);
});

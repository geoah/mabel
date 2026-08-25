import { expect, test, type Page } from "@playwright/test";

import {
  apiGet,
  BOB_URL,
  composeDown,
  containerRunning,
  dc,
  docker,
  json,
  mabel,
  mustRun,
  removeExtras,
  resetTopology,
  WITNESS_URL,
} from "../lib/docker";
import { BASE32_ID, expectExit } from "../lib/stories";
import { createIdentity, identifier, openAction, searchIdentity } from "../lib/ui";

/** docs/stories/008-link-with-no-witness.md */
test.describe.configure({ mode: "serial" });

/** The home this story starts by hand: a wallet with no witness at all. */
const DANA = "mabel-dana";
const DANA_URL = "http://127.0.0.1:9085";

/** What publishing a machine puts in front of a person, once per home. */
const CONSENT = [
  "The machine's id stays readable forever by anyone who can name this identity.",
  "Anyone who reads it can dial that machine directly, which shows the machine's address to them and to the relay that connects them.",
  "Once this home answers at a published address, anyone who dials it can list the identities it signs for and, if it keeps records for other people, the records it keeps.",
];

/** What handing the link over gives away, said on the panel that makes one. */
const DISCLOSURE = [
  "The link carries this identity's Mabel ID, which anyone holding it can read.",
  "It carries the machines that answer for this identity, so whoever has it can dial them directly.",
  "Whoever uses it asks those machines for this record, which tells them this home's network address.",
];

let bobPage: Page;
let danaPage: Page;

let bobId = "";
let bobNodeId = "";
let bobTicket = "";
let link = "";

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  bobPage = await context.newPage();
  danaPage = await context.newPage();
});

test("step 1: the topology from nothing, and an identity in bob's wallet", async () => {
  resetTopology();
  bobNodeId = expectExit(mabel("bob", ["node", "id"]), 0).stdout.trim();
  expect(bobNodeId).toMatch(BASE32_ID);

  await bobPage.goto(`${BOB_URL}/wallet`);
  const bob = await createIdentity(bobPage, { alias: "bob", kind: "person" });
  bobId = bob.identityId;
  // Bob names no witness and pushes nothing: this story is about an identity
  // no witness keeps a copy of (proposal 006 section 2).
  const identity = await apiGet(BOB_URL, `/api/identities/${bobId}`);
  expect(identity.body.identity.witnesses).toEqual([]);
  expect(identity.body.identity.endpoints).toEqual([]);
});

test("step 2: bob publishes the machine that answers for him", async () => {
  await bobPage.getByTestId(`identity-card-link-${bobId}`).click();
  await expect(bobPage).toHaveURL(`${BOB_URL}/identities/${bobId}`);
  // The action sits under the group about being reached, beside the one that
  // hands the link over (proposal 006 section 8).
  await expect(bobPage.getByTestId("action-group-reach")).toContainText("Reaching this identity");
  await openAction(bobPage, "action-endpoints");
  await expect(bobPage.getByTestId("endpoints-empty")).toHaveText(
    "This identity's record names no machine yet.",
  );

  // "Use this node" fills the box with the Iroh ID of the machine serving this
  // page, which is the machine that answers for bob.
  await bobPage.getByTestId("endpoints-use-this-node").click();
  await expect(bobPage.getByTestId("endpoints-input")).toHaveValue(bobNodeId);
  await bobPage.getByTestId("endpoints-submit").click();

  // Nothing is signed before the three facts are on the screen.
  const consent = bobPage.getByTestId("endpoints-consent");
  for (const sentence of CONSENT) {
    await expect(consent).toContainText(sentence);
  }
  await expect(bobPage.getByTestId("endpoints-consent-confirm")).toHaveText("Publish the machine");
  await bobPage.getByTestId("endpoints-consent-confirm").click();
  await expect(bobPage.getByTestId("endpoints-head-seq")).toHaveText("Saved at position 1.");
  await expect(bobPage.getByTestId("endpoints-list")).toContainText(bobNodeId);

  // The advertisement is an entry on bob's own record, signed by bob.
  const identity = await apiGet(BOB_URL, `/api/identities/${bobId}`);
  expect(identity.body.identity.endpoints).toEqual([bobNodeId]);
  expect(identity.body.identity.head_seq).toBe(1);
  const ledger = await apiGet(BOB_URL, `/api/identities/${bobId}/ledger?since=1&limit=1`);
  expect(ledger.body.events[0].payload_kind).toBe("endpoint_advertisement");
  expect(ledger.body.events[0].payload).toEqual({ endpoints: [bobNodeId] });
  // The record says what it did, in the words a reader gets.
  await expect(bobPage.getByTestId("event-gloss-1")).toHaveText(
    "published the machines that answer for it",
  );
});

test("step 3: the link bob hands over names him and that machine", async () => {
  await openAction(bobPage, "action-share");
  await expect(bobPage.getByTestId("share-panel")).toBeVisible();
  link = await identifier(bobPage, "share-panel");
  expect(link).toBe(`mabel://${bobId}?endpoints=${bobNodeId}`);
  await expect(bobPage.getByTestId("share-machine-count")).toHaveText("The link names 1 machine.");
  // The same string as a square to scan, and as a file to hand over.
  await expect(bobPage.getByTestId("share-qr")).toBeVisible();
  await expect(bobPage.getByTestId("share-download")).toHaveAttribute(
    "download",
    `${bobId.slice(0, 8)}.mabel`,
  );
  for (const sentence of DISCLOSURE) {
    await expect(bobPage.getByTestId("share-disclosure")).toContainText(sentence);
  }

  // The CLI builds the same link from the same record, which is what a person
  // without a browser hands over.
  const shared = json(expectExit(mabel("bob", ["identity", "share", "bob", "--json"]), 0));
  expect(shared.link).toBe(link);
  expect(shared.endpoints).toEqual([bobNodeId]);
  expect(shared.endpoints_from).toBe("advertised");

  // The address to route to that machine is not on any ledger and never will
  // be, so it travels the way the first one always does: as a ticket, out of
  // band (proposal 006 section 5.4).
  bobTicket = expectExit(mabel("bob", ["node", "ticket", "--port", "9072"]), 0).stdout.trim();
  expect(bobTicket.length).toBeGreaterThan(0);
});

test("step 4: every witness stops, and a home with none of them starts", async () => {
  expectExit(dc(["stop", "witness"]), 0);
  expect(containerRunning("mabel-witness")).toBe(false);
  // Nothing in this topology witnesses for anybody now.
  const answered = await fetch(`${WITNESS_URL}/api/node`).then(
    () => true,
    () => false,
  );
  expect(answered).toBe(false);

  mustRun("docker", ["volume", "create", "mabel-dana-home"]);
  mustRun("docker", [
    "run",
    "-d",
    "--name",
    DANA,
    "--network",
    "mabel_mabel",
    "--volume",
    "mabel-dana-home:/data",
    "--env",
    "MABEL_RELAY=disabled",
    "--env",
    "MABEL_HTTP_BIND=0.0.0.0:9085",
    "--env",
    "MABEL_IROH_PORT=9075",
    "--publish",
    "9085:9085",
    "mabel:dev",
    "serve",
    "--http",
    "0.0.0.0:9085",
    "--iroh-port",
    "9075",
    // The one thing this home is told: how to route to bob's machine.
    "--peer",
    bobTicket,
  ]);
  await expect
    .poll(async () => (await fetch(`${DANA_URL}/api/node`).then((r) => r.ok, () => false)), {
      timeout: 60_000,
    })
    .toBe(true);

  // A home with no keys, no records and no witness: the node page says so in
  // one sentence rather than pretending to be a different program.
  const node = await apiGet(DANA_URL, "/api/node");
  expect(node.body.identity_count).toBe(0);
  expect(node.body.ledger_count).toBe(0);
  expect(node.body.witness_for).toEqual([]);
  expect(node.body.witnesses).toEqual([]);
  const witnesses = await apiGet(DANA_URL, "/api/witnesses");
  expect(witnesses.body.witnesses).toEqual([]);

  await danaPage.goto(`${DANA_URL}/node`);
  await expect(danaPage.getByTestId("node-no-keys")).toHaveText(
    "This home holds no keys, so it signs for nothing and adds nothing to any record. It keeps 0 records.",
  );
  await expect(danaPage.getByTestId("node-witness-for")).toHaveText("none");
  await expect(danaPage.getByTestId("node-witnesses-empty")).toHaveText("none");
});

test("step 5: the link opens bob's page in a wallet that knows nobody", async () => {
  // The wallet parses no link: it hands the string to the node, which owns the
  // grammar and answers with the identity and the machines the link named.
  const resolved = await apiGet(DANA_URL, `/api/resolve?input=${encodeURIComponent(link)}`);
  expect(resolved.body.input_kind).toBe("link");
  expect(resolved.body.identity_id).toBe(bobId);
  expect(resolved.body.endpoints).toEqual([bobNodeId]);

  await searchIdentity(danaPage, DANA_URL, link, bobId, [bobNodeId]);
  // This home holds no copy, so the page offers to fetch it and says first
  // what asking those machines does.
  await expect(danaPage.getByTestId("identity-fetch")).toBeVisible();
  await expect(danaPage.getByTestId("identity-fetch-link-note")).toHaveText(
    "This link names the machines to ask for this record. Asking them tells those machines this home's network address and which identity it is looking for.",
  );
  await expect(danaPage.getByTestId("identity-fetch")).toContainText(
    "Asks the machines the link named, in order, and keeps what they send.",
  );
});

test("step 6: the fetch lands with no witness in the topology", async () => {
  await danaPage.getByTestId("identity-fetch-button").click();
  await expect(danaPage.getByTestId("ledger-panel")).toBeVisible();
  await expect(danaPage.getByTestId("identity-fetch")).toHaveCount(0);
  await expect(danaPage.getByTestId("identity-detail-event-count")).toHaveText("2");

  // The record came from bob's own machine, verified from nothing the way any
  // other source is (proposal 001 section 3.7).
  const stored = await apiGet(DANA_URL, `/api/identities/${bobId}`);
  expect(stored.body.identity.head_seq).toBe(1);
  expect(stored.body.identity.event_count).toBe(2);
  expect(stored.body.identity.endpoints).toEqual([bobNodeId]);
  expect(stored.body.identity.witnesses).toEqual([]);

  // Still no witness anywhere: the witness container is stopped and this home
  // never had one configured.
  expect(containerRunning("mabel-witness")).toBe(false);
  const node = await apiGet(DANA_URL, "/api/node");
  expect(node.body.ledger_count).toBe(1);
  expect(node.body.identity_count).toBe(0);
  expect((await apiGet(DANA_URL, "/api/witnesses")).body.witnesses).toEqual([]);

  // Storing a record is not controlling it, so the page carries no actions.
  await expect(danaPage.getByTestId("identity-actions")).toHaveCount(0);
});

test("step 7: the borrowed home and the topology come down", async () => {
  docker(["rm", "-f", DANA]);
  docker(["volume", "rm", "-f", "mabel-dana-home"]);
  expect(containerRunning(DANA)).toBe(false);
  removeExtras();
  composeDown();
});

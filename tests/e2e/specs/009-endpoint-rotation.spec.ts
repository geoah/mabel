import { expect, test, type Page } from "@playwright/test";

import {
  apiGet,
  apiPost,
  composeDown,
  containerRunning,
  dc,
  dcSh,
  docker,
  json,
  mabel,
  mustRun,
  removeExtras,
  resetTopology,
  witnessOf,
  WITNESS_URL,
} from "../lib/docker";
import { BASE32_ID, createIdentityCli, expectExit } from "../lib/stories";
import { identifier } from "../lib/ui";

/** docs/stories/009-endpoint-rotation.md */
test.describe.configure({ mode: "serial" });

/** The container holding the second endpoint the witness identity moves to. */
const NEW_MACHINE = "mabel-witness-new";
/** The client that holds the advertisement from before the rotation. */
const NEW_MACHINE_URL = "http://127.0.0.1:9086";
const CARLA = "mabel-carla";
const CARLA_URL = "http://127.0.0.1:9087";

/** The two sentences an endpoint row carries, in the words a reader gets. */
const ON_OWN_RECORD = "This endpoint is listed on this identity's own record.";
const NOT_CONFIRMED = "No record we have confirms that this endpoint answers for it.";

let carlaPage: Page;

let witnessIdentity = "";
let oldEndpoint = "";
let newEndpoint = "";
let newTicket = "";
let aliceId = "";

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  carlaPage = await context.newPage();
});

/** `docker exec <container> sh -c <script>`, with the exit code asserted. */
function inContainer(container: string, script: string, status = 0) {
  return expectExit(docker(["exec", container, "sh", "-c", script]), status);
}

test("step 1: one witness on one endpoint, and a record it keeps", async () => {
  resetTopology();
  const witness = witnessOf();
  witnessIdentity = witness.identity;
  oldEndpoint = witness.endpointId;
  expect(witnessIdentity).toMatch(BASE32_ID);

  // The witness identity's own record names the endpoint that answers for it,
  // published by the container on its first start (proposal 006 section 2).
  // The container publishes a display name before that advertisement, so this
  // chain is inception, name, endpoints and its head sits at seq 2.
  const advertised = await apiGet(WITNESS_URL, `/api/identities/${witnessIdentity}`);
  expect(advertised.body.identity.endpoints).toEqual([oldEndpoint]);
  expect(advertised.body.identity.head_seq).toBe(2);
  const node = await apiGet(WITNESS_URL, "/api/node");
  expect(node.body.witness_for).toEqual([
    { identity: witnessIdentity, advertised: true, reason: null },
  ]);

  aliceId = createIdentityCli("alice", "alice");
  expectExit(
    mabel("alice", ["witness", "add", "--identity", "alice", "--witness", witnessIdentity]),
    0,
  );
  expectExit(
    dcSh("alice", 'mabel sync push --identity alice --peer "$(cat /shared/witness.ticket)"'),
    0,
  );
});

test("step 2: a client that holds only this advertisement", async () => {
  mustRun("docker", ["volume", "create", "mabel-carla-home"]);
  mustRun("docker", [
    "run",
    "-d",
    "--name",
    CARLA,
    "--network",
    "mabel_mabel",
    "--volume",
    "mabel-carla-home:/data",
    "--volume",
    "mabel_witness-ticket:/shared:ro",
    "--env",
    "MABEL_RELAY=disabled",
    "--env",
    "MABEL_HTTP_BIND=0.0.0.0:9087",
    "--env",
    "MABEL_IROH_PORT=9077",
    // The one bootstrap record this home ever gets: the ticket the witness
    // published, which is also what configures the witness in node.json.
    "--env",
    "MABEL_WAIT_FOR_TICKET=/shared/witness",
    "--publish",
    "9087:9087",
    "mabel:dev",
    "serve",
    "--http",
    "0.0.0.0:9087",
    "--iroh-port",
    "9077",
  ]);
  await expect
    .poll(async () => await fetch(`${CARLA_URL}/api/node`).then((r) => r.ok, () => false), {
      timeout: 60_000,
    })
    .toBe(true);

  const configured = await apiGet(CARLA_URL, "/api/witnesses");
  expect(configured.body.witnesses).toHaveLength(1);
  expect(configured.body.witnesses[0].identity_id).toBe(witnessIdentity);
  expect(configured.body.witnesses[0].endpoints.map((entry: any) => entry.endpoint_id)).toEqual([
    oldEndpoint,
  ]);

  // It takes its own copy of the witness's record, so the advertisement it
  // holds is the one from before the rotation.
  inContainer(
    CARLA,
    `mabel sync fetch ${witnessIdentity} --from ${oldEndpoint} --peer "$(cat /shared/witness.ticket)"`,
  );
  const held = await apiGet(CARLA_URL, `/api/identities/${witnessIdentity}`);
  expect(held.body.identity.endpoints).toEqual([oldEndpoint]);
  expect(held.body.identity.head_seq).toBe(2);

  // With the old endpoint up it reaches alice's record through the witness, by
  // naming the witness identity and letting resolution find an endpoint.
  const fetched = json(
    inContainer(
      CARLA,
      `mabel sync fetch ${aliceId} --from-witness ${witnessIdentity} --peer "$(cat /shared/witness.ticket)" --json`,
    ),
  );
  expect(fetched.ledger_id).toBe(aliceId);
  expect(fetched.source).toBe(oldEndpoint);
});

test("step 3 (5.5 step 1): the new endpoint comes up and joins the fleet", async () => {
  mustRun("docker", ["volume", "create", "mabel-witness-new-home"]);
  mustRun("docker", [
    "run",
    "-d",
    "--name",
    NEW_MACHINE,
    "--network",
    "mabel_mabel",
    "--volume",
    "mabel-witness-new-home:/data",
    "--volume",
    "mabel_witness-ticket:/shared",
    "--env",
    "MABEL_RELAY=disabled",
    "--env",
    "MABEL_HTTP_BIND=0.0.0.0:9086",
    "--env",
    "MABEL_IROH_PORT=9076",
    // An endpoint joining the fleet is told where the one already in it is, and
    // it publishes its own ticket, which is the out-of-band record step 3 of
    // section 5.5 has to hand over.
    "--env",
    "MABEL_WAIT_FOR_TICKET=/shared/witness",
    "--env",
    "MABEL_PUBLISH_TICKET=/shared/witness-new",
    "--publish",
    "9086:9086",
    "mabel:dev",
    "serve",
    "--http",
    "0.0.0.0:9086",
    "--iroh-port",
    "9076",
  ]);
  await expect
    .poll(
      async () => await fetch(`${NEW_MACHINE_URL}/api/node`).then((r) => r.ok, () => false),
      { timeout: 60_000 },
    )
    .toBe(true);
  newEndpoint = expectExit(docker(["exec", NEW_MACHINE, "mabel", "node", "id"]), 0).stdout.trim();
  expect(newEndpoint).toMatch(BASE32_ID);
  expect(newEndpoint).not.toBe(oldEndpoint);
  newTicket = inContainer(NEW_MACHINE, "cat /shared/witness-new.ticket").stdout.trim();

  // The new endpoint takes copies of what the fleet serves, so a reader that
  // dials it gets the same answers the old one gave. Its own node does the
  // fetching, because a node serves what its own process wrote.
  for (const ledger of [witnessIdentity, aliceId]) {
    const fetched = await apiPost(NEW_MACHINE_URL, `/api/identities/${ledger}/fetch`, {
      from: oldEndpoint,
    });
    expect(fetched.status, JSON.stringify(fetched.body)).toBe(200);
  }
  const copied = await apiGet(NEW_MACHINE_URL, `/api/identities/${aliceId}`);
  expect(copied.body.identity.head_seq).toBe(1);
});

test("step 4 (5.5 step 2): one advertisement naming both endpoints", async () => {
  // Whole replacement: the old endpoint has to be repeated here or it is
  // dropped in this step, and every reader holding this event would stop
  // dialling it. The controller of the witness identity is the node serving
  // this route, so the append runs there and that node serves it at once.
  const replaced = await apiPost(WITNESS_URL, `/api/identities/${witnessIdentity}/endpoints`, {
    endpoints: [oldEndpoint, newEndpoint],
  });
  expect(replaced.status, JSON.stringify(replaced.body)).toBe(200);
  expect(replaced.body.head_seq).toBe(3);
  expect(replaced.body.event.payload_kind).toBe("endpoint_advertisement");
  expect(replaced.body.event.payload).toEqual({ endpoints: [oldEndpoint, newEndpoint] });

  const advertised = await apiGet(WITNESS_URL, `/api/identities/${witnessIdentity}`);
  expect(advertised.body.identity.endpoints).toEqual([oldEndpoint, newEndpoint]);
});

test("step 5 (5.5 step 3): the bootstrap records that were never updated", async () => {
  // This step is out of band by construction: the records are not on any
  // ledger. The new endpoint's ticket exists, and this client was not handed
  // it, so its only bootstrap record still names the old endpoint.
  expect(newTicket.length).toBeGreaterThan(0);
  const configured = await apiGet(CARLA_URL, "/api/witnesses");
  expect(configured.body.witnesses[0].endpoints.map((entry: any) => entry.endpoint_id)).toEqual([
    oldEndpoint,
  ]);
  const held = await apiGet(CARLA_URL, `/api/identities/${witnessIdentity}`);
  expect(held.body.identity.head_seq).toBe(2);
  expect(held.body.identity.endpoints).toEqual([oldEndpoint]);
});

test("step 6 (5.5 step 4): the new endpoint alone, and the old one stops", async () => {
  const replaced = await apiPost(WITNESS_URL, `/api/identities/${witnessIdentity}/endpoints`, {
    endpoints: [newEndpoint],
  });
  expect(replaced.status, JSON.stringify(replaced.body)).toBe(200);
  expect(replaced.body.head_seq).toBe(4);
  expect(replaced.body.event.payload).toEqual({ endpoints: [newEndpoint] });

  // The old endpoint is not on the record any more: a reader that fetches the
  // witness from here is told to dial the new one.
  const advertised = await apiGet(WITNESS_URL, `/api/identities/${witnessIdentity}`);
  expect(advertised.body.identity.endpoints).toEqual([newEndpoint]);
  expect(advertised.body.identity.head_seq).toBe(4);

  // It no longer answers for the identity it witnesses for either, so it stops
  // taking records it does not already store (proposal 006 section 4.1).
  const node = await apiGet(WITNESS_URL, "/api/node");
  expect(node.body.witness_for[0].identity).toBe(witnessIdentity);
  expect(node.body.witness_for[0].advertised).toBe(false);
  expect(node.body.witness_for[0].reason).toBe(
    "that identity's ledger advertises other endpoints and not this one",
  );

  // The fleet keeps its copies current while the old endpoint is still up, so
  // the new one serves the advertisement that names it.
  const refreshed = await apiPost(
    NEW_MACHINE_URL,
    `/api/identities/${witnessIdentity}/fetch`,
    { from: oldEndpoint },
  );
  expect(refreshed.status, JSON.stringify(refreshed.body)).toBe(200);
  const onTheNewOne = await apiGet(NEW_MACHINE_URL, `/api/identities/${witnessIdentity}`);
  expect(onTheNewOne.body.identity.endpoints).toEqual([newEndpoint]);

  expectExit(dc(["stop", "witness"]), 0);
  expect(containerRunning("mabel-witness")).toBe(false);
});

test("step 7: the stale client reaches nothing", async () => {
  // Its copy of the witness names the old endpoint, its node.json names the old
  // endpoint, and the old endpoint is gone. The new advertisement sits on an
  // endpoint it cannot dial, so it cannot learn the new one from inside mabel.
  const refused = json(
    inContainer(
      CARLA,
      `mabel sync fetch ${witnessIdentity} --from-witness ${witnessIdentity} --json`,
      30,
    ),
  );
  expect(refused.ok).toBe(false);
  expect(refused.code).toBe(30);
  expect(JSON.stringify(refused.details)).toContain(oldEndpoint);
  expect(JSON.stringify(refused.details)).not.toContain(newEndpoint);

  const alsoRefused = json(
    inContainer(
      CARLA,
      `mabel sync fetch ${aliceId} --from-witness ${witnessIdentity} --json`,
      30,
    ),
  );
  expect(alsoRefused.code).toBe(30);

  // Nothing changed on disk: the copy it holds is still the old one.
  const held = await apiGet(CARLA_URL, `/api/identities/${witnessIdentity}`);
  expect(held.body.identity.head_seq).toBe(2);
  expect(held.body.identity.endpoints).toEqual([oldEndpoint]);
});

test("step 8: a fresh record hands the new endpoint over, and the client lands", async () => {
  // Recovery is a new ticket, an updated DNS record or a fresh link, handed
  // over the way the first one was (proposal 006 section 5.5).
  const fetched = json(
    inContainer(
      CARLA,
      `mabel sync fetch ${witnessIdentity} --from ${newEndpoint} --peer "${newTicket}" --json`,
    ),
  );
  expect(fetched.ledger_id).toBe(witnessIdentity);
  expect(fetched.source).toBe(newEndpoint);
  expect(fetched.head_seq).toBe(4);

  const held = await apiGet(CARLA_URL, `/api/identities/${witnessIdentity}`);
  expect(held.body.identity.endpoints).toEqual([newEndpoint]);

  // With the new advertisement stored, naming the witness identity resolves to
  // the new endpoint, and the ticket is what routes to it.
  const through = json(
    inContainer(
      CARLA,
      `mabel sync fetch ${aliceId} --from-witness ${witnessIdentity} --peer "${newTicket}" --json`,
    ),
  );
  expect(through.source).toBe(newEndpoint);
});

test("step 9: the client's screens name the new endpoint and doubt the old one", async () => {
  // Both endpoints are hinted, and for the same reason: the only chain this
  // home ever read for the witness came from the endpoint that advertisement
  // names, and an endpoint that served its own evidence proves nothing
  // (proposal 006 section 4.2). Somebody other than the new endpoint would have
  // to serve the same chain for it to count as verified.
  const listed = await apiGet(CARLA_URL, "/api/witnesses");
  const bindings = Object.fromEntries(
    listed.body.witnesses[0].endpoints.map((entry: any) => [entry.endpoint_id, entry.binding]),
  );
  expect(bindings[newEndpoint]).toBe("hinted");
  expect(bindings[oldEndpoint]).toBe("hinted");

  await carlaPage.goto(`${CARLA_URL}/identities/${witnessIdentity}`);
  await expect(carlaPage.getByTestId("identity-detail")).toBeVisible();
  const onRecord = `identity-detail-machine-${newEndpoint}`;
  expect(await identifier(carlaPage, onRecord)).toBe(newEndpoint);
  await expect(carlaPage.getByTestId(`${onRecord}-note`)).toHaveText(ON_OWN_RECORD);
  await expect(carlaPage.getByTestId(`${onRecord}-row`).locator("dt")).toHaveText("endpoint");

  const stale = `identity-detail-machine-${oldEndpoint}`;
  expect(await identifier(carlaPage, stale)).toBe(oldEndpoint);
  await expect(carlaPage.getByTestId(`${stale}-note`)).toHaveText(NOT_CONFIRMED);
});

test("step 10: the containers and the topology come down", async () => {
  removeExtras();
  expect(containerRunning(CARLA)).toBe(false);
  expect(containerRunning(NEW_MACHINE)).toBe(false);
  composeDown();
});

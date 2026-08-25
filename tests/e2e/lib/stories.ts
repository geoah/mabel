import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  apiPost,
  BOB_URL,
  carry,
  COMPOSE_FILE,
  dcSh,
  docker,
  json,
  mabel,
  mustRun,
  readFileBase64,
  resetTopology,
  resetTopologyWithTwoWitnesses,
  stdoutLines,
  waitForNode,
  witnessOf,
  WITNESS_TWO_URL,
  WITNESS_URL,
  writeFileBase64,
  type RunResult,
  type Witness,
} from "./docker";
import {
  addWitness,
  createIdentity,
  identifier,
  idSpan,
  openIdentity,
  push,
  searchIdentity,
} from "./ui";

export const BASE32_ID = /^[a-z2-7]{52}$/;

/** The RFC 4648 base32 alphabet, lowercased: how an id's characters order. */
const BASE32_ALPHABET = "abcdefghijklmnopqrstuvwxyz234567";

/**
 * Orders two ids the way the node orders them, by the bytes they encode.
 * `2` sorts after `z` in base32 and before `a` in ASCII, so a plain string
 * sort disagrees with "ascending identity" on roughly a fifth of pairs.
 */
export function compareIds(left: string, right: string): number {
  for (let index = 0; index < Math.min(left.length, right.length); index += 1) {
    const difference =
      BASE32_ALPHABET.indexOf(left[index]) - BASE32_ALPHABET.indexOf(right[index]);
    if (difference !== 0) {
      return difference;
    }
  }
  return left.length - right.length;
}

/** Asserts an exit code and returns the result, with the output in the message. */
export function expectExit(result: RunResult, status: number): RunResult {
  expect(
    result.status,
    `${result.command}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  ).toBe(status);
  return result;
}

export interface MeetState {
  /** The witness identity a ledger names, and the machine that answers for it. */
  witness: Witness;
  /** The machine that answers for the witness, which is what a push dials. */
  witnessId: string;
  /** The witness's Mabel id, which is what a `WitnessSet` records. */
  witnessIdentity: string;
  aliceId: string;
  bobId: string;
}

/** The default topology of story 001 step 1: `dc down -v && dc up -d --wait`. */
function resetAndReadWitness(): Witness {
  resetTopology();
  return witnessOf();
}

/**
 * Story 001 steps 1 to 7 with the outcomes they verify. Stories 002, 003 and
 * 006 all open with "run story 001 steps 1 to N", so this is the one
 * implementation of them.
 */
export async function story001Steps1to7(
  alicePage: Page,
  bobPage: Page,
  /**
   * Brings the topology up from nothing and answers with the witness's two ids.
   * Story 007 passes its own, because it needs the test resolver overlay and
   * the node-wide witness the wallets start with.
   */
  reset: () => Witness = resetAndReadWitness,
): Promise<MeetState> {
  const state: MeetState = {
    witness: { identity: "", endpointId: "" },
    witnessId: "",
    witnessIdentity: "",
    aliceId: "",
    bobId: "",
  };

  await test.step("001 step 1: the topology from nothing", async () => {
    state.witness = reset();
    state.witnessId = state.witness.endpointId;
    state.witnessIdentity = state.witness.identity;
    expect(state.witnessId).toMatch(BASE32_ID);
    // A witness is an identity, minted by the container on its first start and
    // published beside the ticket (proposal 006 section 1).
    expect(state.witnessIdentity).toMatch(BASE32_ID);
    expect(state.witnessIdentity).not.toBe(state.witnessId);

    // The witness home witnesses for that identity and advertises this machine
    // on its record, which is what admits a push naming it (section 4).
    const witness = await apiGet(WITNESS_URL, "/api/node");
    expect(witness.body.witness_for).toEqual([
      { identity: state.witnessIdentity, advertised: true, reason: null },
    ]);
    expect(witness.body.endpoint_id).toBe(state.witnessId);
  });

  await test.step("001 steps 2 to 4: an identity in each wallet UI", async () => {
    for (const [page, url] of [
      [alicePage, ALICE_URL],
      [bobPage, BOB_URL],
    ] as const) {
      await page.goto(`${url}/wallet`);
      // Three entries on every node and no fourth: one home, whatever else the
      // node does (proposal 006 section 8). There is no witness tab.
      await expect(page.getByTestId("nav-wallet")).toBeVisible();
      await expect(page.getByTestId("nav-witnesses")).toBeVisible();
      await expect(page.getByTestId("nav-node")).toBeVisible();
      await expect(page.getByTestId("nav-witness")).toHaveCount(0);
      await expect(page.locator('header [data-testid^="nav-"]')).toHaveCount(3);
      // Round 6 of proposal 005 made this page three flat sections under three
      // headings: the box that opens an identity, the identities this wallet
      // signs for, and the ones it knows of and does not control. The box takes
      // a handle and a link as well as a Mabel ID, and its label says so.
      await expect(page.getByTestId("wallet-search")).toBeVisible();
      await expect(page.getByTestId("wallet-search")).toContainText("Mabel ID, handle or link");
      await expect(page.getByTestId("wallet-search-input")).toHaveAttribute(
        "placeholder",
        "alice.example, or paste a Mabel ID or a link",
      );
      await expect(page.getByTestId("identity-list-empty")).toHaveText(
        "You have no identities yet. Create one below.",
      );
      // A wallet that has never fetched, crawled or noted anybody knows of no
      // other identity, and the trusted-only switch starts off.
      await expect(page.getByTestId("known-identities")).toBeVisible();
      await expect(page.getByTestId("known-trusted-only")).toHaveAttribute("aria-checked", "false");
      await expect(page.getByTestId("known-identities-empty")).toHaveText(
        "Your wallet knows of no other identity yet.",
      );
      // No document names a role: what a node can do is read from what it
      // holds, which here is no key and nobody's records.
      const node = await apiGet(url, "/api/node");
      expect(node.body).not.toHaveProperty("role");
      expect(node.body.identity_count).toBe(0);
      expect(node.body.witness_for).toEqual([]);
    }

    const alice = await createIdentity(alicePage, { alias: "alice", kind: "person" });
    // An identity is the digest of its own inception event.
    expect(alice.identityId).toBe(alice.inceptionEvent);
    expect(alice.identityId).toMatch(BASE32_ID);
    state.aliceId = alice.identityId;

    const bob = await createIdentity(bobPage, { alias: "bob", kind: "person" });
    expect(bob.identityId).toBe(bob.inceptionEvent);
    state.bobId = bob.identityId;
  });

  await test.step("001 step 5: links exchanged out of band", async () => {
    // `identity share` builds the link this identity hands over: its Mabel ID
    // and the machines that answer for it. Neither has advertised one yet, so
    // `auto` names the machine this home runs on (proposal 006 section 7).
    const aliceNode = expectExit(mabel("alice", ["node", "id"]), 0).stdout.trim();
    const bobNode = expectExit(mabel("bob", ["node", "id"]), 0).stdout.trim();
    const aliceLink = shareLink("alice", "alice", state.aliceId, aliceNode);
    const bobLink = shareLink("bob", "bob", state.bobId, bobNode);

    // The wallet parses no link of its own: the box hands it to the node, which
    // owns the grammar, and navigates to the identity the node named.
    await searchIdentity(alicePage, ALICE_URL, bobLink, state.bobId, [bobNode]);
    // Alice holds no copy of bob's record, so the page offers to fetch it and
    // says first what using the link does.
    await expect(alicePage.getByTestId("identity-fetch")).toBeVisible();
    await expect(alicePage.getByTestId("identity-fetch-link-note")).toHaveText(
      "This link names the machines to ask for this record. Asking them tells those machines this home's network address and which identity it is looking for.",
    );
    await searchIdentity(bobPage, BOB_URL, aliceLink, state.aliceId, [aliceNode]);
    await expect(bobPage.getByTestId("identity-fetch")).toBeVisible();
  });

  await test.step("001 step 6: both name the witness", async () => {
    await openIdentity(alicePage, ALICE_URL, state.aliceId);
    await addWitness(alicePage, state.witnessIdentity, 1);
    await openIdentity(bobPage, BOB_URL, state.bobId);
    await addWitness(bobPage, state.witnessIdentity, 1);

    // The set on the chain names the witness identity, not the machine: a
    // witness that moves machines keeps this event standing (section 1).
    const identity = await apiGet(ALICE_URL, `/api/identities/${state.aliceId}`);
    expect(identity.body.identity.witnesses).toEqual([state.witnessIdentity]);
    expect(identity.body.identity.head_seq).toBe(1);
    expect(identity.body.identity.event_count).toBe(2);
  });

  await test.step("001 step 7: both push", async () => {
    await push(alicePage, state.witnessId, { stored: 2, headSeq: 1 });
    await push(bobPage, state.witnessId, { stored: 2, headSeq: 1 });
  });

  return state;
}

/**
 * `mabel identity share`, with the two facts story 001 step 5 reads: the link
 * names this identity and the machine this home runs on.
 */
function shareLink(
  service: string,
  alias: string,
  identityId: string,
  endpointId: string,
): string {
  const document = json(
    expectExit(mabel(service, ["identity", "share", alias, "--json"]), 0),
  );
  expect(document.identity_id).toBe(identityId);
  expect(document.endpoints).toEqual([endpointId]);
  expect(document.endpoints_from).toBe("node");
  expect(document.link).toBe(`mabel://${identityId}?endpoints=${endpointId}`);
  return document.link;
}

/**
 * The head position of one ledger, on the route that reports it.
 *
 * Round 5 of proposal 005 took the position off the identity page and off the
 * cards: a reader is told how many entries a record holds, and which position
 * the newest one sits at is a fact of `GET /api/identities/<id>`. Every story
 * that used to read `identity-detail-head-seq` reads this instead.
 */
export async function expectHeadSeq(
  base: string,
  identityId: string,
  headSeq: number,
): Promise<void> {
  const identity = await apiGet(base, `/api/identities/${identityId}`);
  expect(identity.body.identity.head_seq, `head of ${identityId}`).toBe(headSeq);
}

/**
 * The `/node` page: what this node is dialled by, how it is reachable, what it
 * signs for, whose records it keeps, what it holds and which build is running.
 * Every row is `GET /api/node`, so the document is what each is read against.
 *
 * There is no role row and no role field. What a node can do is read from what
 * it holds (proposal 006 section 8), so a caller says what it expects the node
 * to hold and the page is checked against that.
 */
export async function readNodePage(
  page: Page,
  base: string,
  expected: {
    endpointId: string;
    /** The witness identities `node.json.witness_for` names, in order. */
    witnessFor?: string[];
  },
): Promise<void> {
  const witnessFor = expected.witnessFor ?? [];
  await page.getByTestId("nav-node").click();
  await expect(page).toHaveURL(`${base}/node`);
  await expect(page.getByTestId("node-page")).toBeVisible();

  const node = await apiGet(base, "/api/node");
  expect(node.body).not.toHaveProperty("role");
  expect(node.body.endpoint_id).toBe(expected.endpointId);
  expect(node.body.relay).toBe("disabled");
  expect(node.body.witness_for.map((entry: any) => entry.identity)).toEqual(witnessFor);

  // No screen names a role, so nothing on this page says one.
  await expect(page.getByTestId("node-role")).toHaveCount(0);
  // Where the API listens is not a fact about the node's place in the network,
  // so round 5 dropped the row; the document still carries it.
  await expect(page.getByTestId("node-http-bind")).toHaveCount(0);
  expect(typeof node.body.http_bind).toBe("string");
  // The endpoint id is written out whole, because it is the only name a node
  // has and it is what another node is given to dial this one. The row calls it
  // the Iroh ID, which is what it is.
  expect(await identifier(page, "node-endpoint-id")).toBe(expected.endpointId);
  await expect(idSpan(page, "node-endpoint-id")).toHaveAttribute("data-truncated", "false");
  await expect(page.getByTestId("node-endpoint-id-row").locator("dt")).toHaveText("Iroh ID");
  await expect(page.getByTestId("node-relay")).toHaveText("direct connections only");
  await expect(page.getByTestId("node-version")).toHaveText(node.body.version);
  // The capacity the topology sets, said in the units a reader reads.
  await expect(page.getByTestId("node-storage")).toHaveText(
    /^[\d.]+ (bytes|kB|MB|GB) of 2\.1 GB$/,
  );

  // Every node draws the same four counts: a count is a bare number, and the
  // row's own label is the noun.
  await expect(page.getByTestId("node-identity-count")).toHaveText(
    String(node.body.identity_count),
  );
  await expect(page.getByTestId("node-identity-count-row").locator("dt")).toHaveText("identities");
  await expect(page.getByTestId("node-ledger-count")).toHaveText(String(node.body.ledger_count));
  await expect(page.getByTestId("node-ledger-count-row").locator("dt")).toHaveText("records");
  await expect(page.getByTestId("node-fork-count")).toHaveText(String(node.body.fork_count));
  await expect(page.getByTestId("node-fork-count-row").locator("dt")).toHaveText("conflicts");

  // Who this node accepts records for, which is what makes it a witness. A
  // node that keeps nobody's records says so in one word.
  await expect(page.getByTestId("node-witness-for-row").locator("dt")).toHaveText(
    "keeps records for",
  );
  if (witnessFor.length === 0) {
    await expect(page.getByTestId("node-witness-for")).toHaveText("none");
  } else {
    for (const identity of witnessFor) {
      await expect(page.getByTestId(`node-witness-for-${identity}`)).toBeVisible();
      await expect(page.getByTestId(`node-witness-for-${identity}-link`)).toHaveAttribute(
        "href",
        `/identities/${identity}`,
      );
    }
  }

  // A home holding no key of its own is not broken and not a different
  // program: it signs for nothing and keeps records for other people.
  if (node.body.identity_count === 0) {
    const records = `${node.body.ledger_count} ${node.body.ledger_count === 1 ? "record" : "records"}`;
    const keeps =
      witnessFor.length === 0
        ? `It keeps ${records}.`
        : `It keeps ${records} and accepts new entries for ${
            witnessFor.length === 1 ? "one identity" : `${witnessFor.length} identities`
          }.`;
    await expect(page.getByTestId("node-no-keys")).toHaveText(
      `This home holds no keys, so it signs for nothing and adds nothing to any record. ${keeps}`,
    );
  } else {
    await expect(page.getByTestId("node-no-keys")).toHaveCount(0);
  }

  // The witnesses this node uses by default are a card list of their own, one
  // identity card each, and a node that uses none says so.
  const defaults: string[] = (await apiGet(base, "/api/witnesses")).body.witnesses
    .filter((witness: any) => witness.is_node_default)
    .map((witness: any) => witness.identity_id);
  await expect(page.getByTestId("node-witnesses")).toBeVisible();
  await expect(page.getByTestId("node-witnesses")).toContainText("Witnesses it uses by default");
  if (defaults.length === 0) {
    await expect(page.getByTestId("node-witnesses-empty")).toHaveText("none");
    await expect(page.getByTestId("node-witness-cards")).toHaveCount(0);
  } else {
    await expect(page.getByTestId("node-witnesses-empty")).toHaveCount(0);
    for (const identity of defaults) {
      await expect(page.getByTestId(`identity-card-link-${identity}`)).toHaveAttribute(
        "href",
        `/identities/${identity}`,
      );
      await expect(
        page.getByTestId(`identity-card-${identity}`).locator("[data-value]").first(),
      ).toHaveAttribute("data-truncated", "false");
    }
  }
}

/**
 * `mabel identity export`, with the two lines story 002 step 1 quotes.
 *
 * A link says where to reach an identity; a descriptor carries its inception
 * byte for byte, which is what an invitation embeds (proposal 002 section 8).
 * Story 002 is the story that needs the second one.
 *
 * The witness count is the raw endpoints the retired tag-11 list holds, and
 * these chains hold none: a descriptor carries endpoints to dial, and a tag-19
 * witness set names identities, which is not the same thing (proposal 006
 * section 1).
 */
function exportDescriptor(
  service: string,
  alias: string,
  identityId: string,
  outPath: string,
): void {
  const result = expectExit(mabel(service, ["identity", "export", alias, "--out", outPath]), 0);
  const lines = stdoutLines(result);
  expect(lines[0]).toMatch(new RegExp(`^exported ${identityId} to ${outPath} \\(\\d+ bytes\\)$`));
  expect(lines[1]).toBe("declared kind person, raw root, 0 witnesses");
}

export interface SharedLedgerState extends MeetState {
  orgId: string;
}

/**
 * Story 002 steps 1 to 8: the shared ledger founded, bob invited, his
 * acceptance admitted, the membership read back. Story 006 opens with it.
 */
export async function story002Steps1to8(
  alicePage: Page,
  bobPage: Page,
): Promise<SharedLedgerState> {
  const meet = await story001Steps1to7(alicePage, bobPage);
  const state: SharedLedgerState = { ...meet, orgId: "" };

  await test.step("002 step 1: the descriptors an invitation embeds", async () => {
    exportDescriptor("bob", "bob", state.bobId, "/tmp/bob.descriptor");
    carry("mabel-bob", "/tmp/bob.descriptor", "mabel-alice", "/tmp/bob.descriptor");
    exportDescriptor("alice", "alice", state.aliceId, "/tmp/alice.descriptor");
    carry("mabel-alice", "/tmp/alice.descriptor", "mabel-bob", "/tmp/alice.descriptor");
  });

  await test.step("002 step 2: alice founds the shared ledger", async () => {
    await alicePage.goto(`${ALICE_URL}/wallet`);
    const org = await createIdentity(alicePage, {
      alias: "mabel-demo-co",
      kind: "organization",
      founder: state.aliceId,
    });
    state.orgId = org.identityId;
  });

  await test.step("002 step 3: an identity root holds no key of its own", async () => {
    await openIdentity(alicePage, ALICE_URL, state.orgId);
    // The kind an identity declares is a badge, in the quiet tone: it labels
    // what the identity says it is, and says nothing about your own trust.
    const kind = alicePage.getByTestId("identity-detail-declared-kind");
    await expect(kind).toHaveText("organization");
    await expect(kind).toHaveAttribute("data-declared-kind", "organization");
    // Proposal 005 moved the one key fact into the card's principals row: the
    // sentence sits beside whoever signs, and the two 52-character values are
    // pinned on the routes that carry them, not on the screen. Round 6 draws
    // that row only when the answer differs from the identity itself, which is
    // what an identity-rooted ledger is, and names each principal.
    await expect(alicePage.getByTestId("identity-detail-principals-row").locator("dt")).toHaveText(
      "who can act for it",
    );
    await expect(
      alicePage.getByTestId(`identity-detail-principal-${state.aliceId}-name`),
    ).toHaveText("alice");
    await expect(alicePage.getByTestId("identity-detail-founded")).toHaveText(
      "Its controllers sign for it.",
    );
    // Both keys are absent from the document, not null, on an identity that
    // holds none (contracts/README.md, "Shared documents").
    const identity = await apiGet(ALICE_URL, `/api/identities/${state.orgId}`);
    expect(identity.body.identity).not.toHaveProperty("active_key");
    expect(identity.body.identity).not.toHaveProperty("reserve_commit");
    const keys = await apiGet(ALICE_URL, `/api/identities/${state.orgId}/keys`);
    expect(keys.status).toBe(409);
    expect(keys.body.details.reason).toBe("no_keys_held");
  });

  await test.step("002 step 4: alice invites bob as a controller", async () => {
    const invite = expectExit(
      mabel("alice", [
        "membership",
        "invite",
        "--ledger",
        "mabel-demo-co",
        "--by",
        "alice",
        "--invitee",
        "/tmp/bob.descriptor",
        "--role",
        "controller",
        "--out",
        "/tmp/invitation.bundle",
      ]),
      0,
    );
    const lines = stdoutLines(invite);
    expect(lines[0]).toBe(`invited ${state.bobId} as controller at seq 1 of ${state.orgId}`);
    expect(lines[1]).toMatch(/^wrote \/tmp\/invitation\.bundle \(2 events, \d+ bytes\)$/);
  });

  let acceptanceBase64 = "";
  await test.step("002 steps 5 and 6: the bundle travels and bob signs", async () => {
    const bundle = readFileBase64("mabel-alice", "/tmp/invitation.bundle");
    expect(bundle.length).toBeGreaterThan(0);

    const surface = await apiPost(
      BOB_URL,
      `/api/identities/${state.bobId}/memberships/acceptances`,
      { invitation_bundle_base64: bundle },
    );
    expect(surface.status).toBe(200);
    expect(surface.body.ledger_id).toBe(state.orgId);
    expect(surface.body.declared_kind).toBe("organization");
    expect(surface.body.root).toBe("identity");
    expect(surface.body.controllers).toHaveLength(1);
    expect(surface.body.controllers[0].identity).toBe(state.aliceId);
    expect(surface.body.controllers[0].is_root).toBe(true);
    expect(surface.body.invitee).toBe(state.bobId);
    expect(surface.body.role).toBe("controller");
    expect(surface.body.controller_on_raw_root).toBe(false);
    expect(surface.body.warning).toBeNull();
    expect(typeof surface.body.acceptance_base64).toBe("string");
    expect(surface.body.acceptance_base64.length).toBeGreaterThan(0);
    acceptanceBase64 = surface.body.acceptance_base64;
  });

  await test.step("002 step 7: a controller admits the acceptance", async () => {
    writeFileBase64("mabel-alice", "/tmp/acceptance.file", acceptanceBase64);
    const admit = expectExit(
      mabel("alice", [
        "membership",
        "admit",
        "--ledger",
        "mabel-demo-co",
        "--by",
        "alice",
        "/tmp/acceptance.file",
      ]),
      0,
    );
    expect(stdoutLines(admit)[0]).toBe(
      `admitted ${state.bobId} as controller at seq 2 of ${state.orgId}`,
    );
  });

  await test.step("002 step 8: the membership state reads back", async () => {
    const list = expectExit(mabel("alice", ["membership", "list", "--ledger", "mabel-demo-co"]), 0);
    const lines = stdoutLines(list).map((line) => line.trim());
    expect(lines[0]).toBe(`${state.orgId}: 2 principals, 0 open invitations up to seq 2`);
    expect(
      lines.some((line) =>
        new RegExp(`^controller ${state.aliceId} \\([a-z2-7]{52}\\) root$`).test(line),
      ),
    ).toBe(true);
    expect(
      lines.some((line) => new RegExp(`^controller ${state.bobId} \\([a-z2-7]{52}\\)$`).test(line)),
    ).toBe(true);
    expect(
      lines.some((line) =>
        new RegExp(`^invitation .* offers controller to ${state.bobId}, accepted$`).test(line),
      ),
    ).toBe(true);

    const document = json(
      expectExit(mabel("alice", ["membership", "list", "--ledger", "mabel-demo-co", "--json"]), 0),
    );
    expect(document.root).toBe("identity");
    const identities = document.principals.map((principal: any) => principal.identity);
    expect(identities).toEqual([...identities].sort(compareIds));
    expect(document.invitations[0].status).toBe("accepted");
  });

  return state;
}

export interface ForkState {
  /** Witness one: the identity alice's chain names, and its machine. */
  witness: Witness;
  witnessId: string;
  witnessIdentity: string;
  witnessTicket: string;
  /** Witness two: a second witness identity, on a second machine. */
  witnessTwo: Witness;
  witnessTwoId: string;
  witnessTwoIdentity: string;
  witnessTwoTicket: string;
  aliceId: string;
  carolId: string;
  daveId: string;
  keptEvent: string;
  conflictingEvent: string;
}

/**
 * Story 004 steps 1 to 7: two witness identities, alice's home on two machines,
 * one branch to each witness and the second branch offered to witness one.
 * Story 005 opens with it and tears down what it leaves running.
 */
export async function story004Steps1to7(): Promise<ForkState> {
  const state: ForkState = {
    witness: { identity: "", endpointId: "" },
    witnessId: "",
    witnessIdentity: "",
    witnessTicket: "",
    witnessTwo: { identity: "", endpointId: "" },
    witnessTwoId: "",
    witnessTwoIdentity: "",
    witnessTwoTicket: "",
    aliceId: "",
    carolId: "",
    daveId: "",
    keptEvent: "",
    conflictingEvent: "",
  };

  await test.step("004 steps 1 and 2: two witnesses from nothing", async () => {
    // The second witness is a compose service, not a hand-wired `docker run`:
    // one overlay starts it, waits for it and wires both wallets to both
    // witnesses (ticket 032).
    resetTopologyWithTwoWitnesses();
    state.witness = witnessOf("witness");
    state.witnessId = state.witness.endpointId;
    state.witnessIdentity = state.witness.identity;
    state.witnessTwo = witnessOf("witness-two");
    state.witnessTwoId = state.witnessTwo.endpointId;
    state.witnessTwoIdentity = state.witnessTwo.identity;
    state.witnessTicket = expectExit(dcSh("alice", "cat /shared/witness.ticket"), 0).stdout.trim();
    state.witnessTwoTicket = expectExit(
      dcSh("alice", "cat /shared/witness-two.ticket"),
      0,
    ).stdout.trim();
    for (const id of [
      state.witnessId,
      state.witnessIdentity,
      state.witnessTwoId,
      state.witnessTwoIdentity,
    ]) {
      expect(id).toMatch(BASE32_ID);
    }
    // Two witnesses are two identities, not two machines answering for one:
    // each home minted its own and witnesses for that one alone.
    expect(state.witnessTwoIdentity).not.toBe(state.witnessIdentity);
    await waitForNode(WITNESS_TWO_URL);
    const second = await apiGet(WITNESS_TWO_URL, "/api/node");
    expect(second.body.witness_for).toEqual([
      { identity: state.witnessTwoIdentity, advertised: true, reason: null },
    ]);
  });

  await test.step("004 step 3: one identity, two subjects, both witnesses", async () => {
    state.aliceId = createIdentityCli("alice", "alice");
    state.carolId = createIdentityCli("alice", "carol");
    state.daveId = createIdentityCli("alice", "dave");
    // The set on the chain names both witness identities. Alice's home reaches
    // each of them through the machine its entrypoint recorded in node.json.
    for (const witness of [state.witnessIdentity, state.witnessTwoIdentity]) {
      expectExit(
        mabel("alice", ["witness", "add", "--identity", "alice", "--witness", witness]),
        0,
      );
    }
    const witnesses = json(
      expectExit(mabel("alice", ["identity", "show", "alice", "--json"]), 0),
    ).witnesses;
    expect([...witnesses].sort(compareIds)).toEqual(
      [state.witnessIdentity, state.witnessTwoIdentity].sort(compareIds),
    );
    expectExit(
      dcSh(
        "alice",
        'mabel sync push --identity alice --peer "$(cat /shared/witness.ticket)" --peer "$(cat /shared/witness-two.ticket)"',
      ),
      0,
    );
  });

  await test.step("004 step 4: alice's home on a second machine", async () => {
    await startAliceTwo();
  });

  await test.step("004 step 5: both machines append at the same sequence", async () => {
    const kept = json(
      expectExit(
        mabel("alice", [
          "trust",
          "add",
          "--issuer",
          "alice",
          "--subject",
          state.carolId,
          "--no-sync",
          "--json",
        ]),
        0,
      ),
    );
    const conflicting = json(
      expectExit(
        docker([
          "exec",
          "mabel-alice-two",
          "mabel",
          "trust",
          "add",
          "--issuer",
          "alice",
          "--subject",
          state.daveId,
          "--no-sync",
          "--json",
        ]),
        0,
      ),
    );
    expect(kept.attestation_seq).toBe(3);
    expect(conflicting.attestation_seq).toBe(3);
    state.keptEvent = kept.attestation_event;
    state.conflictingEvent = conflicting.attestation_event;
    expect(state.keptEvent).not.toBe(state.conflictingEvent);
  });

  await test.step("004 step 6: one branch to each witness", async () => {
    const first = expectExit(
      dcSh(
        "alice",
        `mabel sync push --identity alice --to ${state.witnessId} --peer "$(cat /shared/witness.ticket)"`,
      ),
      0,
    );
    expect(stdoutLines(first)).toContain(`${state.witnessId} accepted, stored 1`);
    const second = expectExit(
      docker([
        "exec",
        "mabel-alice-two",
        "sh",
        "-c",
        `mabel sync push --identity alice --to ${state.witnessTwoId} --peer "$(cat /shared/witness-two.ticket)"`,
      ]),
      0,
    );
    expect(stdoutLines(second)).toContain(`${state.witnessTwoId} accepted, stored 1`);
  });

  await test.step("004 step 7: the second branch reaches witness one", async () => {
    const pushed = expectExit(
      docker([
        "exec",
        "mabel-alice-two",
        "sh",
        "-c",
        `mabel sync push --identity alice --to ${state.witnessId} --peer "$(cat /shared/witness.ticket)" --json`,
      ]),
      30,
    );
    const document = json(pushed);
    expect(document.ok).toBe(false);
    expect(document.code).toBe(30);
    expect(document.message).toMatch(/^Network error: /);
    expect(document.details.reason).toBe("all_witnesses_failed");
    expect(document.details.results[0].status).toBe("rejected");
    expect(document.details.results[0].reject_code).toBe("FORK");
    expect(document.details.results[0].at_seq).toBe(3);
  });

  return state;
}

/** `mabel identity create` in one container, returning the new identity id. */
export function createIdentityCli(
  service: string,
  alias: string,
  extra: string[] = ["--kind", "person"],
): string {
  const result = expectExit(
    mabel(service, ["identity", "create", "--alias", alias, ...extra, "--json"]),
    0,
  );
  return json(result).identity_id;
}

/**
 * Story 004 step 4 and story 006 step 3: alice's home copied to a second
 * machine without node.json and node.key, served on 9084. `docker run -d`
 * returns before the home is prepared, so the API answering is the signal.
 */
export async function startAliceTwo(): Promise<void> {
  mustRun("docker", ["volume", "create", "mabel-alice-second"]);
  mustRun("docker", [
    "run",
    "--rm",
    "--user",
    "0",
    "--volumes-from",
    "mabel-alice",
    "--volume",
    "mabel-alice-second:/copy",
    "--entrypoint",
    "sh",
    "mabel:dev",
    "-c",
    "cp -a /data/. /copy/ && rm -f /copy/node.json /copy/node.key",
  ]);
  mustRun("docker", [
    "run",
    "-d",
    "--name",
    "mabel-alice-two",
    "--network",
    "mabel_mabel",
    "--volume",
    "mabel-alice-second:/data",
    "--volume",
    "mabel_witness-ticket:/shared:ro",
    "--env",
    "MABEL_RELAY=disabled",
    "--env",
    "MABEL_HTTP_BIND=0.0.0.0:9084",
    "--env",
    "MABEL_IROH_PORT=9074",
    "--publish",
    "9084:9084",
    "mabel:dev",
    // One command serves every home (proposal 006 section 8). The two hidden
    // aliases exist so an old command line still runs; nothing here uses one.
    "serve",
    "--http",
    "0.0.0.0:9084",
    "--iroh-port",
    "9074",
  ]);
  await waitForNode("http://127.0.0.1:9084");
}

export { COMPOSE_FILE };

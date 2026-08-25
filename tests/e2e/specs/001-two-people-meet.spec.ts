import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  BOB_URL,
  json,
  mabel,
  stdoutLines,
  verifier,
  WITNESS_URL,
} from "../lib/docker";
import {
  BASE32_ID,
  compareIds,
  createIdentityCli,
  expectExit,
  expectHeadSeq,
  readNodePage,
  story001Steps1to7,
} from "../lib/stories";
import {
  addTrust,
  cardIds,
  createIdentity,
  expandCard,
  identifier,
  idSpan,
  openAction,
  openIdentity,
  push,
  trustCard,
} from "../lib/ui";

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
let witnessIdentity = "";
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
  witnessIdentity = state.witnessIdentity;
  aliceId = state.aliceId;
  bobId = state.bobId;
  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  aliceKey = identity.body.identity.active_key;
});

test("the keys action offers both secret keys, and the route answers the same", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  // The final round of proposal 005 replaced the "What you can do" heading with
  // four group headings, each holding the actions it is about, so the twelve
  // rows are four decisions rather than one list. Every action kept its testid.
  for (const [group, heading] of [
    ["profile", "Profile"],
    ["trust", "Trust"],
    ["witnesses", "Witnesses and sync"],
    ["control", "Control and keys"],
  ] as const) {
    await expect(alicePage.getByTestId(`action-group-${group}`)).toContainText(heading);
  }
  await expect(alicePage.getByTestId("identity-actions")).not.toContainText("What you can do");
  await expect(alicePage.getByTestId("action-group-control")).toContainText("Save your keys");
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
  // Round 4 of proposal 005: who this identity trusts is a list of collapsed
  // identity cards keyed by the subject, and the entry that said it is read on
  // the record rather than drawn as a row of its own.
  await expect(trustCard(alicePage, bobId)).toBeVisible();
  // Three entries on the record now, and which position the newest sits at is
  // read on the route: round 5 took the position off the screen.
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("3");
  await expectHeadSeq(ALICE_URL, aliceId, 2);

  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.trust[0].subject).toBe(bobId);
  expect(identity.body.identity.trust[0].revoked).toBe(false);
  expect(identity.body.identity.trust[0].attestation_event).toBe(aliceAttestation);
});

test("step 9: bob attests alice, in a second ledger", async () => {
  await openIdentity(bobPage, BOB_URL, bobId);
  const bobAttestation = await addTrust(bobPage, aliceId);
  await expect(trustCard(bobPage, aliceId)).toBeVisible();
  await expectHeadSeq(BOB_URL, bobId, 2);
  expect(bobAttestation).not.toBe(aliceAttestation);
});

test("step 10: both push again and the witness holds two ledgers", async () => {
  await push(alicePage, witnessId, { stored: 1 });
  await push(bobPage, witnessId, { stored: 1 });

  // A witness's holdings are the records it stores and cannot sign for, which
  // is the route every node answers: `/api/ledgers` is gone and the witness
  // needs no route of its own (proposal 006 section 8).
  const known = await apiGet(WITNESS_URL, "/api/identities/known?offset=0&limit=256");
  expect(known.body.more).toBe(false);
  // `known` sorts by the rendered id, which orders the digits before the
  // letters, where `GET /api/identities` sorts by the bytes they encode.
  expect(known.body.identities.map((row: any) => row.identity_id)).toEqual(
    [aliceId, bobId].sort(),
  );
  for (const row of known.body.identities) {
    expect(row.declared_kind).toBe("person");
    expect(row.head_seq).toBe(2);
    expect(row.stored).toBe(true);
  }
  // How many entries each record holds is a fact of the record, on the identity
  // route; a conflict is a fact of the store, on /api/forks.
  for (const id of [aliceId, bobId]) {
    const identity = await apiGet(WITNESS_URL, `/api/identities/${id}`);
    expect(identity.body.identity.event_count).toBe(3);
    expect(identity.body.identity.witnesses).toEqual([witnessIdentity]);
  }
  const forks = await apiGet(WITNESS_URL, "/api/forks");
  expect(forks.body.entries).toEqual([]);
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
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("4");
  await expectHeadSeq(ALICE_URL, aliceId, 3);
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
  // One node, one spelling: a record no home holds is `unknown_ledger`.
  const carolLedger = await apiGet(WITNESS_URL, `/api/identities/${carolId}`);
  expect(carolLedger.status).toBe(404);
  expect(carolLedger.body.details.reason).toBe("unknown_ledger");
});

test("the wallet home draws one card per identity, and the card is the page", async () => {
  await alicePage.goto(`${ALICE_URL}/wallet`);
  // GET /api/identities answers in ascending identity id order, and the card
  // list renders what it answered.
  expect(await cardIds(alicePage)).toEqual([aliceId, carolId].sort(compareIds));

  // Alice publishes no name, so the card falls back to the nickname only this
  // device sees, and draws it as the name rather than in parentheses after one.
  await expect(alicePage.getByTestId(`identity-card-name-${aliceId}-name`)).toHaveText("alice");
  await expect(alicePage.getByTestId(`identity-card-name-${aliceId}-nickname`)).toHaveCount(0);
  await expect(alicePage.getByTestId(`identity-card-declared-kind-${aliceId}`)).toHaveText(
    "person",
  );
  // Round 5 took the position off the cards: how far a record has got is not
  // what a reader of an address book came for, and the route still reports it.
  await expect(alicePage.locator('[data-testid^="identity-card-head-seq-"]')).toHaveCount(0);
  await expectHeadSeq(ALICE_URL, aliceId, 3);
  await expectHeadSeq(ALICE_URL, carolId, 0);
  await expect(alicePage.getByTestId(`identity-card-link-${aliceId}`)).toHaveAttribute(
    "href",
    `/identities/${aliceId}`,
  );
  // A card has the width for a whole Mabel ID and a Mabel ID is the only thing
  // that tells two identities apart, so no card truncates one (round 6).
  expect(await identifier(alicePage, `identity-card-name-${aliceId}`)).toBe(aliceId);
  await expect(idSpan(alicePage, `identity-card-name-${aliceId}`)).toHaveAttribute(
    "data-truncated",
    "false",
  );

  // The one expand affordance every card in this app draws. Round 5 made it a
  // small icon button in the corner, so the words are its accessible name, and
  // the chevron inside it turns over rather than sideways.
  const expand = alicePage.getByTestId(`identity-card-expand-${aliceId}`);
  await expect(expand).toHaveAttribute("aria-label", "Show the record");
  await expect(expand.locator('[data-slot="collapsible-chevron"]')).toHaveAttribute(
    "data-state",
    "closed",
  );
  await expandCard(alicePage, aliceId);
  await expect(expand.locator('[data-slot="collapsible-chevron"]')).toHaveAttribute(
    "data-state",
    "open",
  );
  // The opened card is the record: the row labels are lowercase, and the four
  // entries alice's record holds are counted rather than positioned.
  const details = alicePage.getByTestId(`identity-card-details-${aliceId}`);
  await expect(
    details.getByTestId(`identity-card-alias-${aliceId}-row`).locator("dt"),
  ).toContainText("nickname");
  await expect(details.getByTestId(`identity-card-alias-${aliceId}`)).toHaveText("alice");
  await expect(details.getByTestId(`identity-card-event-count-${aliceId}`)).toHaveText("4");
  // Alice holds her own key, so nothing else can act for her: round 6 draws no
  // "who can act for it" row at all when the answer is the identity itself.
  await expect(alicePage.getByTestId(`identity-card-principals-${aliceId}`)).toHaveCount(0);

  // The one identity this wallet knows of and does not control is the witness:
  // naming it on a chain meant resolving it first, and this home kept the copy
  // it read. Alice and carol are identities it signs for, so neither is a row
  // here, and bob is nowhere: opening his link read nothing into this home.
  expect(await cardIds(alicePage, "known-identity-cards")).toEqual([witnessIdentity]);
  await expect(alicePage.getByTestId(`identity-card-unheld-${witnessIdentity}`)).toHaveCount(0);
});

test("the identifier a spec reads is the whole value", async () => {
  await openIdentity(alicePage, ALICE_URL, aliceId);
  // Proposal 005 draws the page's heading through the inline identity
  // component, so the id sits inside `identity-detail-resolved` rather than in
  // a row of its own.
  expect(await identifier(alicePage, "identity-detail-resolved")).toBe(aliceId);
  // The button beside an id names what it copies, because "copy" alone tells a
  // screen reader nothing about which of the ids on a screen it would take. The
  // confirmation it swaps that label for, `Copy Mabel ID: copied`, is pinned by
  // `ui/src/test/identifier.test.tsx`, which can hold the two-second clock still.
  await expect(
    alicePage
      .getByTestId("identity-detail-resolved")
      .getByRole("button", { name: "Copy Mabel ID" }),
  ).toHaveAttribute("data-copied", "false");
});

test("step 16: a new identity that publishes a name and an email from birth", async () => {
  // Proposal 005: the create form takes the private nickname plus the two
  // public facts, and the node appends one ProfileUpdate at seq 1 right after
  // the inception. Dana is created last and never witnessed or pushed, so the
  // sequence arithmetic of steps 6 to 14 is untouched.
  await alicePage.goto(`${ALICE_URL}/wallet`);
  const dana = await createIdentity(alicePage, {
    alias: "dana",
    kind: "person",
    displayName: "Dana Example",
    email: "dana@dana.example",
  });
  await expect(alicePage.getByTestId("identity-create-result-profile")).toBeVisible();
  await expect(alicePage.getByTestId("identity-create-result-display-name")).toHaveText(
    "Dana Example",
  );
  await expect(alicePage.getByTestId("identity-create-result-email")).toHaveText(
    "dana@dana.example",
  );

  // Two entries on a record that was just made: what it is, and what it shows
  // the world.
  const identity = await apiGet(ALICE_URL, `/api/identities/${dana.identityId}`);
  expect(identity.status).toBe(200);
  expect(identity.body.identity.head_seq).toBe(1);
  expect(identity.body.identity.event_count).toBe(2);
  expect(identity.body.identity.profile.display_name).toBe("Dana Example");
  expect(identity.body.identity.profile.email).toBe("dana@dana.example");
  expect(identity.body.identity.profile.hostname).toBeNull();
  expect(identity.body.identity.profile.seq).toBe(1);

  const ledger = await apiGet(ALICE_URL, `/api/identities/${dana.identityId}/ledger?since=0&limit=8`);
  expect(ledger.body.events.map((event: any) => event.payload_kind)).toEqual([
    "inception",
    "profile_update",
  ]);
  expect(ledger.body.events[1].payload).toEqual({
    display_name: "Dana Example",
    hostname: null,
    email: "dana@dana.example",
  });

  // The card list names her by the name she publishes, with the nickname only
  // this device sees in parentheses after it: Dana Example (dana). Round 6 put
  // the public email in the opened card alone, so the card is opened to read it.
  await alicePage.goto(`${ALICE_URL}/wallet`);
  await expect(alicePage.getByTestId(`identity-card-name-${dana.identityId}-name`)).toHaveText(
    "Dana Example",
  );
  await expect(alicePage.getByTestId(`identity-card-name-${dana.identityId}-nickname`)).toHaveText(
    "(dana)",
  );
  await expect(alicePage.getByTestId(`identity-card-email-${dana.identityId}`)).toHaveCount(0);
  await expandCard(alicePage, dana.identityId);
  await expect(alicePage.getByTestId(`identity-card-email-${dana.identityId}`)).toHaveText(
    "dana@dana.example",
  );
  await expect(
    alicePage.getByTestId(`identity-card-email-${dana.identityId}-row`).locator("dt"),
  ).toHaveText("email");
});

test("step 17: the node page names this node and what it holds", async () => {
  // The third nav entry, added by round 4 of proposal 005. The endpoint id it
  // draws is the one `mabel node id` prints, which is what another node dials.
  // Alice's home keeps nobody else's records, so the row says so in one word.
  const endpointId = expectExit(mabel("alice", ["node", "id"]), 0).stdout.trim();
  expect(endpointId).toMatch(BASE32_ID);
  await alicePage.goto(`${ALICE_URL}/wallet`);
  await readNodePage(alicePage, ALICE_URL, { endpointId });
});

test("the witnesses screen draws the witness as the identity it is", async () => {
  // A witness is an identity, so its card is the identity card every other
  // screen draws and its page is the identity page (proposal 006 section 8).
  await alicePage.goto(`${ALICE_URL}/wallet`);
  await alicePage.getByTestId("nav-witnesses").click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/witnesses`);
  await expect(alicePage.getByTestId("witness-cards")).toBeVisible();
  expect(await cardIds(alicePage, "witness-cards")).toEqual([witnessIdentity]);
  await expect(alicePage.getByTestId(`witness-default-${witnessIdentity}`)).toHaveText(
    "this node uses it by default",
  );
  // The machine that answers for it is a row of its record, which is the half
  // of the card the collapsed one folds away.
  await expandCard(alicePage, witnessIdentity);
  const machineRow = `identity-card-machine-${witnessId}-${witnessIdentity}`;
  await expect(alicePage.getByTestId(`${machineRow}-row`).locator("dt")).toHaveText("machine");
  expect(await identifier(alicePage, machineRow)).toBe(witnessId);

  // `/witnesses/<id>` is not a page of its own any more: it redirects to the
  // identity page, so a saved link still opens something.
  await alicePage.goto(`${ALICE_URL}/witnesses/${witnessIdentity}`);
  await expect(alicePage).toHaveURL(`${ALICE_URL}/identities/${witnessIdentity}`);
  // /witness is gone outright: one home on every node.
  await alicePage.goto(`${ALICE_URL}/witness`);
  await expect(alicePage.getByTestId("route-not-found")).toBeVisible();
});

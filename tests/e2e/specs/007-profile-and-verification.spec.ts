import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  apiPost,
  apiPut,
  BOB_URL,
  dcExec,
  dcSh,
  docker,
  json,
  mabel,
  resetTopologyWithResolver,
  stdoutLines,
  until,
  verifier,
  WITNESS_URL,
  writeFileBase64,
} from "../lib/docker";
import {
  BASE32_ID,
  compareIds,
  createIdentityCli,
  expectExit,
  expectHeadSeq,
  story001Steps1to7,
} from "../lib/stories";
import {
  addTrust,
  cardIds,
  expandCard,
  identifier,
  idSpan,
  openAction,
  openIdentity,
  push,
  searchIdentity,
  shown,
  trustCard,
} from "../lib/ui";

/** docs/stories/007-profile-and-verification.md */
test.describe.configure({ mode: "serial" });

/**
 * This is the one story that needs `docker/compose.dns.yaml`, so it brings the
 * topology down and up again with that overlay in its own step 1, the way
 * every other story runs `dc down -v && dc up -d --wait`. Global setup stays
 * base-only, which is what keeps stories 001 to 006 runnable on their own, and
 * `composeDown` names both files, so the resolver, the zone volume and the
 * overlay's network are gone whether the run ends here or in global teardown.
 */
const RESOLVER = "mabel-resolver";
const ZONE_PATH = "/etc/coredns/zones/example.zone";
const RESOLVER_IMAGE = "mabel-resolver:dev";

/** 25 hours: past the 24-hour freshness window of a verified result. */
const STALE_AFTER_MS = 25 * 60 * 60 * 1000;

/**
 * The label of `docker/dns/zones/example.zone` that names five endpoints, and
 * the five it names: ed25519 public keys from fixed seeds, with no container
 * behind any of them. What it proves is the parsing rule, which costs no
 * container (proposal 006 section 6).
 */
const MANY_MACHINES_IDENTITY = "xpezo4a4wovzgs7dx43f2pzwk2w7gutnvrzmsrgzuxtcfzjbw4ka";
const MANY_MACHINES = [
  "cmo62aqzrfceqo7ruqkrvzutvktcw6474cwkovxd7z7ctorce3na",
  "xyhbzzscrer36lffdqwwmhuqv45sshxx7wgfoxvlvj2suhub427a",
  "72al4if4g2w4hrpec4r66oz3gs6xypgvk6avk73uduy6ffkrceva",
  "msccj64kqyg7wqltmbwoom5kdimw7jzyb7yply2a7lnvvsxttvka",
  "hyzbxuqfwf3yq2lnc7a5civnjlrgfigue7p6itgy2on4bpnnxsgq",
];

const GRAPH_CONSENT_FIRST =
  "Every witness your wallet asks learns which people you are interested in.";
const GRAPH_CONSENT_SECOND =
  "Your wallet reads their records to answer how you know someone, and keeps no copy.";
/** The heading of the reverse list, which round 5 made the plain sentence. */
const REVERSE_HEADING = "Who your wallet has seen trusting them";
/** The caveat that used to be the heading, now the sentence its info tip holds. */
const REVERSE_LABEL =
  "Best effort: who your wallet has seen trusting them, not everyone who does";

let alicePage: Page;
let bobPage: Page;

let witnessId = "";
let witnessIdentity = "";
let aliceNodeId = "";
let aliceId = "";
let bobId = "";
let carolId = "";
let aliceAttestation = "";
let carolAttestation = "";
let aliceCheckedAtMs = 0;
let bobCheckedAtMs = 0;
let firstSyncId = "";
let beforeDns: Record<string, unknown> = {};

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  alicePage = await context.newPage();
  bobPage = await context.newPage();
});

/**
 * One `mabel-endpoints=` record, split across two character-strings.
 *
 * A TXT character-string holds 255 bytes, so a label naming five endpoints has
 * to be split; a reader joins the strings back with no separator before it
 * parses anything (proposal 006 section 6). The split falls after a comma, so
 * neither half names the whole list on its own.
 */
function endpointsRecord(label: string, endpoints: string[]): string {
  const head = endpoints.slice(0, -1).map((endpoint) => `${endpoint},`).join("");
  const tail = endpoints[endpoints.length - 1];
  return `_mabel.${label} IN TXT "mabel-endpoints=${head}" "${tail}"`;
}

/**
 * The zone the test resolver serves, rewritten with the ids this run minted.
 * The serial has to rise or CoreDNS's `file` plugin keeps serving what it
 * loaded, and the health record has to stay or the container goes unhealthy.
 *
 * `claims` is the `mabel=` record at each label, `endpoints` the
 * `mabel-endpoints=` record beside it. The `many-machines` label of
 * `docker/dns/zones/example.zone` is kept whole, because the five ids on it
 * are what the split rule is read against and no container answers at any of
 * them.
 *
 * Both TTLs are one second, positive and negative. A wallet's resolver caches
 * for the TTL it is given, so a longer one would have a check taken seconds
 * after the resolver stopped still answering from that cache.
 */
function zoneText(
  serial: number,
  claims: Record<string, string>,
  endpoints: Record<string, string[]> = {},
): string {
  const records = [
    ...Object.entries(claims).map(
      ([label, identity]) => `_mabel.${label} IN TXT "mabel=${identity}"`,
    ),
    ...Object.entries(endpoints).map(([label, listed]) => endpointsRecord(label, listed)),
  ].join("\n");
  return [
    "$ORIGIN example.",
    "$TTL 1",
    "",
    "@   IN SOA ns.example. hostmaster.example. (",
    `        ${serial} 60 30 300 1 )`,
    "@   IN NS  ns.example.",
    "ns  IN A   127.0.0.1",
    "",
    '_mabel.health IN TXT "mabel=health"',
    `_mabel.many-machines IN TXT "mabel=${MANY_MACHINES_IDENTITY}"`,
    endpointsRecord("many-machines", MANY_MACHINES),
    records,
    "",
  ].join("\n");
}

/**
 * One TXT query over the resolver a wallet actually uses.
 *
 * The mabel image carries no `dig`, so the query runs in a throwaway
 * container sharing the wallet's network namespace: same `127.0.0.11`
 * embedded resolver, same forwarding to `172.29.0.53`.
 */
function digFromWallet(container: string, name: string): string {
  const result = docker([
    "run",
    "--rm",
    "--network",
    `container:${container}`,
    "--entrypoint",
    "dig",
    RESOLVER_IMAGE,
    "+short",
    "TXT",
    name,
  ]);
  return result.stdout.trim();
}

/** The identity document's verification object, from either wallet. */
async function verification(base: string, identityId: string): Promise<any> {
  const identity = await apiGet(base, `/api/identities/${identityId}`);
  return identity.body.identity.verification;
}

/** `POST /api/identities/<id>/verification`, the check that waits. */
async function forceCheck(base: string, identityId: string): Promise<any> {
  const response = await apiPost(base, `/api/identities/${identityId}/verification`, {});
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return response.body.verification;
}

/**
 * Forces checks until one answers what the caller is waiting for, and reports
 * the last one before it.
 *
 * A wallet's resolver caches, so the first check after the resolver stops or
 * starts can still answer from that cache. `seen` is called with every
 * intermediate result, which is how the test knows the timestamp the failed
 * re-check has to leave alone.
 */
async function forceCheckUntil(
  what: string,
  base: string,
  identityId: string,
  ready: (verification: any) => boolean,
  seen: (verification: any) => void = () => {},
): Promise<any> {
  let last: any = null;
  await until(
    what,
    async () => {
      const checked = await forceCheck(base, identityId);
      if (!ready(checked)) {
        seen(checked);
        return false;
      }
      last = checked;
      return true;
    },
    90_000,
    1_000,
  );
  return last;
}

/** The pinned trust verification of story 001 step 12, from an empty home. */
function verifyTrustFromFreshHome(): any {
  return json(
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
}

/**
 * The fields of a trust report that a second run of the same command must
 * repeat. `fetched_at_ms` and the RFC 3339 time inside the statement are the
 * two values a re-read is expected to move, so the time is masked and the
 * timestamp is left out.
 */
function pinned(report: any): Record<string, unknown> {
  return {
    trusted: report.trusted,
    statement: String(report.statement).replace(/\d{4}-\d{2}-\d{2}T[\d:]+Z/, "<time>"),
    attestation_event: report.attestation_event,
    attestation_seq: report.attestation_seq,
    head_seq: report.head_seq,
    head_event: report.head_event,
    revoked_count: report.revoked_count,
    signing_principal: report.signing_principal,
    source: report.source,
  };
}

/** The hostname row of one identity's overview, as the UI renders it. */
async function openHostnameRow(page: Page, base: string, identityId: string) {
  await openIdentity(page, base, identityId);
  return page.getByTestId("identity-detail-hostname-verification");
}

test("step 1: story 001 steps 1 to 12, and carol in bob's home", async () => {
  // This step resets the topology with the resolver overlay and then runs the
  // whole of story 001, which is more than the 120 s default budget.
  test.setTimeout(300_000);
  const state = await story001Steps1to7(alicePage, bobPage, resetTopologyWithResolver);
  witnessId = state.witnessId;
  witnessIdentity = state.witnessIdentity;
  aliceNodeId = expectExit(mabel("alice", ["node", "id"]), 0).stdout.trim();
  aliceId = state.aliceId;
  bobId = state.bobId;

  await test.step("001 steps 8 to 10: one attestation each way, both pushed", async () => {
    await openIdentity(alicePage, ALICE_URL, aliceId);
    aliceAttestation = await addTrust(alicePage, bobId);
    await openIdentity(bobPage, BOB_URL, bobId);
    await addTrust(bobPage, aliceId);
    await push(alicePage, witnessId, { stored: 1 });
    await push(bobPage, witnessId, { stored: 1 });
  });

  await test.step("001 steps 11 and 12: a fresh home answers trusted", async () => {
    // Kept for the re-run after the DNS sequence: a hostname verdict changes
    // nothing a trust report says.
    beforeDns = pinned(verifyTrustFromFreshHome());
    expect(beforeDns.trusted).toBe(true);
  });

  await test.step("carol is created, witnessed, pushed and attested", async () => {
    carolId = createIdentityCli("bob", "carol");
    expect(carolId).toMatch(BASE32_ID);
    // A witness refuses a ledger whose chain does not name it, so without
    // this the push answers NOT_ADMITTED and the crawl has nothing to read.
    expectExit(
      mabel("bob", ["witness", "add", "--identity", "carol", "--witness", witnessIdentity]),
      0,
    );
    expectExit(
      dcSh("bob", 'mabel sync push --identity carol --peer "$(cat /shared/witness.ticket)"'),
      0,
    );
    carolAttestation = json(
      expectExit(
        mabel("bob", ["trust", "add", "--issuer", "bob", "--subject", carolId, "--json"]),
        0,
      ),
    ).attestation_event;
    expectExit(
      dcSh("bob", 'mabel sync push --identity bob --peer "$(cat /shared/witness.ticket)"'),
      0,
    );
  });

  const ledger = await apiGet(WITNESS_URL, `/api/identities/${carolId}`);
  expect(ledger.body.identity.head_seq).toBe(1);
  expect(ledger.body.identity.witnesses).toEqual([witnessIdentity]);

  const bob = await apiGet(BOB_URL, `/api/identities/${bobId}`);
  const attestation = bob.body.identity.trust.find(
    (record: any) => record.subject === carolId,
  );
  expect(attestation.revoked).toBe(false);
  expect(attestation.attestation_event).toBe(carolAttestation);
});

test("steps 2 and 3: the resolver answers, and the TXT records name the run's ids", async () => {
  // Ticket 032's wiring, on the shape proposal 006 gave it: the wallets start
  // pointed at the test resolver and with `<mabel id>=<endpoint id>` in
  // `MABEL_WITNESSES`, so node.json names the witness identity and the endpoint
  // that answers for it, which is the crawler's third source.
  const node = await apiGet(ALICE_URL, "/api/node");
  expect(node.body.witnesses).toEqual([witnessId]);
  const configured = await apiGet(ALICE_URL, "/api/witnesses");
  expect(configured.body.witnesses).toHaveLength(1);
  expect(configured.body.witnesses[0].identity_id).toBe(witnessIdentity);
  expect(configured.body.witnesses[0].is_node_default).toBe(true);
  expect(configured.body.witnesses[0].endpoints.map((entry: any) => entry.endpoint_id)).toEqual([
    witnessId,
  ]);

  // _mabel.bob.example names carol on purpose: a record that exists and
  // claims the wrong identity is what "mismatched" is about. alice.example
  // gains the endpoints that answer for her beside the id she claims.
  writeFileBase64(
    RESOLVER,
    ZONE_PATH,
    Buffer.from(
      zoneText(
        2,
        { alice: aliceId, bob: carolId },
        { alice: [aliceNodeId, witnessId] },
      ),
    ).toString("base64"),
  );
  // The `file` plugin rereads the zone within five seconds of the serial
  // rising, so no restart and no healthcheck wait is needed.
  await until(
    "_mabel.alice.example to answer with alice's id",
    // The label carries two records now, the claim and the endpoints beside it,
    // so the claim is looked for among them rather than being the whole answer.
    () => digFromWallet("mabel-alice", "_mabel.alice.example").includes(`"mabel=${aliceId}"`),
    30_000,
    1_000,
  );
  expect(digFromWallet("mabel-bob", "_mabel.bob.example")).toBe(`"mabel=${carolId}"`);
  expect(digFromWallet("mabel-alice", "_mabel.nobody.example")).toBe("");
});

test("steps 4 and 5: the profile is replaced, and the same replacement is refused", async () => {
  const replaced = json(
    expectExit(
      mabel("alice", [
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Example",
        "--hostname",
        "alice.example",
        "--yes",
        "--json",
      ]),
      0,
    ),
  );
  expect(replaced.ok).toBe(true);
  expect(replaced.identity_id).toBe(aliceId);
  expect(replaced.display_name).toBe("Alice Example");
  expect(replaced.hostname).toBe("alice.example");
  // Proposal 005 added email to the profile, and replacement stays whole: all
  // three fields are reported, set or not.
  expect(replaced.previous).toEqual({ display_name: null, hostname: null, email: null });
  expect(replaced.profile_seq).toBe(3);
  expect(replaced.head_seq).toBe(3);
  expect(replaced.head_event).toBe(replaced.profile_event);

  const identity = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(identity.body.identity.profile.display_name).toBe("Alice Example");
  expect(identity.body.identity.profile.hostname).toBe("alice.example");
  expect(identity.body.identity.profile.seq).toBe(identity.body.identity.head_seq);
  expect(identity.body.identity.profile.signing_principal.identity).toBe(aliceId);
  expect(identity.body.identity.profile.event).toBe(replaced.profile_event);

  // One ProfileUpdate appended, and nothing else.
  const page = await apiGet(ALICE_URL, `/api/identities/${aliceId}/ledger?since=3&limit=8`);
  expect(page.body.events).toHaveLength(1);
  expect(page.body.events[0].payload_kind).toBe("profile_update");
  expect(page.body.events[0].payload).toEqual({
    display_name: "Alice Example",
    hostname: "alice.example",
    email: null,
  });

  const again = json(
    expectExit(
      mabel("alice", [
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Example",
        "--hostname",
        "alice.example",
        "--yes",
        "--json",
      ]),
      20,
    ),
  );
  expect(again.ok).toBe(false);
  expect(again.code).toBe(20);
  expect(again.details.reason).toBe("no_op_profile_update");
  expect(again.details.ledger_id).toBe(aliceId);
  expect(again.details.profile_seq).toBe(3);
  // `message` is prose a person reads, so the ledger in it carries the prefix;
  // `details.ledger_id` above is an id-valued field and stays bare.
  expect(again.message).toBe(
    `Policy error: this profile is already the profile of ${shown(aliceId)}: nothing would change`,
  );
  const unchanged = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(unchanged.body.identity.head_seq).toBe(3);
});

test("step 6: a forced check answers verified, and the UI marks the hostname row", async () => {
  const checked = await forceCheck(ALICE_URL, aliceId);
  expect(checked.status).toBe("verified");
  expect(checked.hostname).toBe("alice.example");
  expect(checked.stale).toBe(false);
  expect(checked.unreachable).toBeNull();
  expect(typeof checked.checked_at_ms).toBe("number");
  expect(checked.last_verified_at_ms).toBe(checked.checked_at_ms);
  expect(checked.detail).toBe(
    `a TXT record at _mabel.alice.example. carries mabel=${aliceId}`,
  );
  aliceCheckedAtMs = checked.checked_at_ms;

  const identity = await verification(ALICE_URL, aliceId);
  expect(identity.status).toBe("verified");
  expect(identity.checked_at_ms).toBe(aliceCheckedAtMs);

  const row = await openHostnameRow(alicePage, ALICE_URL, aliceId);
  await expect(row).toHaveAttribute("data-verification", "verified");
  await expect(row).toContainText("alice.example");
  // Round 4 of proposal 005 calls it a handle everywhere a reader sees it, and
  // the row on the identity page is labelled that.
  await expect(alicePage.getByTestId("identity-detail-hostname-row").locator("dt")).toHaveText(
    "handle",
  );
  // Proposal 005 removed the DNS advisory sentence from every surface: the
  // verdict glyph and the hostname it is about are the whole statement.
  await expect(alicePage.getByTestId("identity-detail-verification-note")).toHaveCount(0);
  // The name is plain text and the id travels beside it, never instead of it.
  await expect(alicePage.getByTestId("identity-detail-resolved-name")).toHaveText("Alice Example");
  expect(await identifier(alicePage, "identity-detail-resolved")).toBe(aliceId);
});

test("step 7: bob.example mismatches, nobody.example is unverified, carol is unclaimed", async () => {
  expectExit(
    mabel("bob", [
      "profile",
      "replace",
      "--identity",
      "bob",
      "--display-name",
      "Bob Example",
      "--hostname",
      "bob.example",
      "--yes",
    ]),
    0,
  );
  const mismatched = await forceCheck(BOB_URL, bobId);
  expect(mismatched.status).toBe("mismatched");
  expect(mismatched.hostname).toBe("bob.example");
  expect(mismatched.last_verified_at_ms).toBeNull();
  expect(mismatched.detail).toBe(
    "the mabel= record at _mabel.bob.example. names another identity",
  );

  const mismatchedRow = await openHostnameRow(bobPage, BOB_URL, bobId);
  await expect(mismatchedRow).toHaveAttribute("data-verification", "mismatched");
  await expect(mismatchedRow).toContainText("bob.example");

  // Changing the hostname invalidates the verdict: the cache entry is bound
  // to the hostname it verified, so the new claim starts unchecked.
  expectExit(
    mabel("bob", [
      "profile",
      "replace",
      "--identity",
      "bob",
      "--display-name",
      "Bob Example",
      "--hostname",
      "nobody.example",
      "--yes",
    ]),
    0,
  );
  const rebound = await verification(BOB_URL, bobId);
  expect(rebound.hostname).toBe("nobody.example");
  expect(rebound.status).not.toBe("verified");
  // No verdict at all, which is its own word: `unverified` is a lookup that
  // found no mabel= record, and no lookup has run against this claim.
  expect(rebound.status).toBe("unchecked");
  expect(rebound.checked_at_ms).toBeNull();
  expect(rebound.stale).toBe(false);
  expect(rebound.detail).toBe("nobody.example has not been checked on this node");

  const unverified = await forceCheck(BOB_URL, bobId);
  expect(unverified.status).toBe("unverified");
  expect(unverified.detail).toBe("_mabel.nobody.example. holds no mabel= TXT record");
  bobCheckedAtMs = unverified.checked_at_ms;

  const unverifiedRow = await openHostnameRow(bobPage, BOB_URL, bobId);
  await expect(unverifiedRow).toHaveAttribute("data-verification", "unverified");

  // An identity claiming no hostname: the row says so and carries no mark,
  // and a forced check has nothing to look up.
  const carol = await verification(BOB_URL, carolId);
  expect(carol.status).toBe("unclaimed");
  expect(carol.hostname).toBeNull();
  expect(carol.checked_at_ms).toBeNull();
  await openIdentity(bobPage, BOB_URL, carolId);
  await expect(bobPage.getByTestId("identity-detail-hostname")).toHaveText("none");
  await expect(bobPage.getByTestId("identity-detail-hostname-verification")).toHaveCount(0);
  // Round 4 of proposal 005 put the check inside the action that sets the
  // handle: one action covers the handle, the line DNS needs and the check.
  // It says the same thing, and says only that, because proposal 005 removed
  // the advisory sentence that used to sit under it.
  await openAction(bobPage, "action-handle");
  await expect(bobPage.getByTestId("handle-panel")).toBeVisible();
  await expect(bobPage.getByTestId("handle-current")).toHaveText("none");
  await expect(bobPage.getByTestId("verification-panel")).toBeVisible();
  await expect(bobPage.getByTestId("verification-status")).toHaveText(
    "this identity claims no handle",
  );
  await expect(bobPage.getByTestId("verification-note")).toHaveCount(0);
  // With no handle set there is no line to publish, so the panel says so.
  await expect(bobPage.getByTestId("handle-panel")).toContainText(
    "Set a handle to see the line your DNS records need.",
  );

  const refused = await apiPost(BOB_URL, `/api/identities/${carolId}/verification`, {});
  expect(refused.status).toBe(409);
  expect(refused.body.code).toBe(20);
  expect(refused.body.details.reason).toBe("no_hostname_claimed");
});

test("with the resolver stopped, a failed check never overwrites a decisive result", async () => {
  // Each check that reaches a stopped resolver waits out the query, so this
  // one holds several of them.
  test.setTimeout(240_000);
  expectExit(docker(["stop", RESOLVER], 60_000), 0);

  // A re-check that cannot answer is recorded beside the verified result, and
  // the document reports both. An indecisive result is replaced instead,
  // which is the only way `unreachable` becomes a status of its own. Both
  // wallets are asked at once: a failed query is slow by definition.
  const [alice, bob] = await Promise.all([
    forceCheckUntil(
      "alice's re-check to fail",
      ALICE_URL,
      aliceId,
      (checked) => checked.unreachable !== null,
      (checked) => {
        aliceCheckedAtMs = checked.checked_at_ms;
      },
    ),
    forceCheckUntil(
      "bob's claim to go unreachable",
      BOB_URL,
      bobId,
      (checked) => checked.status === "unreachable",
      (checked) => {
        bobCheckedAtMs = checked.checked_at_ms;
      },
    ),
  ]);

  expect(alice.status).toBe("verified");
  expect(alice.checked_at_ms).toBe(aliceCheckedAtMs);
  expect(alice.last_verified_at_ms).toBe(aliceCheckedAtMs);
  expect(alice.stale).toBe(false);
  expect(alice.unreachable.checked_at_ms).toBeGreaterThan(aliceCheckedAtMs);
  expect(alice.unreachable.detail).toContain("the query for _mabel.alice.example. failed:");

  expect(bob.hostname).toBe("nobody.example");
  expect(bob.checked_at_ms).toBeGreaterThan(bobCheckedAtMs);
  bobCheckedAtMs = bob.checked_at_ms;

  const unreachableRow = await openHostnameRow(bobPage, BOB_URL, bobId);
  await expect(unreachableRow).toHaveAttribute("data-verification", "unreachable");

  // Listing identities never queries DNS: with the resolver down, both homes
  // still answer, from the cache, with the timestamps already recorded.
  const listed = await apiGet(ALICE_URL, "/api/identities");
  const alicePresent = listed.body.identities.find(
    (identity: any) => identity.identity_id === aliceId,
  );
  expect(alicePresent.verification.status).toBe("verified");
  expect(alicePresent.verification.checked_at_ms).toBe(aliceCheckedAtMs);
  const bobList = await apiGet(BOB_URL, "/api/identities");
  const bobPresent = bobList.body.identities.find(
    (identity: any) => identity.identity_id === bobId,
  );
  expect(bobPresent.verification.status).toBe("unreachable");
  expect(bobPresent.verification.checked_at_ms).toBe(bobCheckedAtMs);
});

test("a verified result older than a day renders as stale, not as a plain check", async () => {
  // The cache is a rebuildable file, so ageing one entry is how a day passes
  // in a suite that runs in a minute. The resolver is still down, so the
  // background re-check the stale GET starts cannot refresh it either.
  const aged = {
    hostname: "alice.example",
    status: "verified",
    checked_at_ms: aliceCheckedAtMs - STALE_AFTER_MS,
    last_verified_at_ms: aliceCheckedAtMs - STALE_AFTER_MS,
    detail: `a TXT record at _mabel.alice.example. carries mabel=${aliceId}`,
    unreachable: null,
  };
  expectExit(
    dcSh(
      "alice",
      `printf '%s' '${JSON.stringify(aged)}' > /data/verification/${aliceId}.json`,
    ),
    0,
  );

  const staleRow = await openHostnameRow(alicePage, ALICE_URL, aliceId);
  await expect(staleRow).toHaveAttribute("data-verification", "stale-verified");
  await expect(staleRow).toContainText("may be out of date");

  const document = await verification(ALICE_URL, aliceId);
  expect(document.stale).toBe(true);
  expect(document.status).toBe("verified");
});

test("the resolver comes back and alice verifies again", async () => {
  test.setTimeout(240_000);
  expectExit(docker(["start", RESOLVER], 60_000), 0);
  await until(
    "the resolver to answer again",
    () => digFromWallet("mabel-alice", "_mabel.alice.example").includes(`"mabel=${aliceId}"`),
    60_000,
    1_000,
  );

  const checked = await forceCheckUntil(
    "alice to verify again",
    ALICE_URL,
    aliceId,
    (verdict) => verdict.unreachable === null,
  );
  expect(checked.status).toBe("verified");
  expect(checked.stale).toBe(false);
  expect(checked.checked_at_ms).toBeGreaterThan(aliceCheckedAtMs);
  aliceCheckedAtMs = checked.checked_at_ms;
});

test("verification gates nothing: the pinned trust report is unchanged", async () => {
  // Alice's ledger has been verified, mismatched on bob's side, gone
  // unreachable and verified again since the first run. The same command from
  // the same empty home answers the same thing, because a hostname verdict is
  // advisory (decision 015): it gates no authorization and no ledger validity.
  expect(pinned(verifyTrustFromFreshHome())).toEqual(beforeDns);
});

test("a replacement that omits the hostname clears it", async () => {
  const cleared = json(
    expectExit(
      mabel("bob", [
        "profile",
        "replace",
        "--identity",
        "bob",
        "--display-name",
        "Bob Example",
        "--yes",
        "--json",
      ]),
      0,
    ),
  );
  expect(cleared.hostname).toBeNull();
  expect(cleared.previous.hostname).toBe("nobody.example");

  // The cleared field is absent from the event, not carried as an empty
  // string: the wire refuses an encoded default, so an event that folds is
  // an event that left the field out.
  const page = await apiGet(
    BOB_URL,
    `/api/identities/${bobId}/ledger?since=${cleared.profile_seq}&limit=1`,
  );
  expect(page.body.events[0].payload).toEqual({
    display_name: "Bob Example",
    hostname: null,
    email: null,
  });

  const document = await verification(BOB_URL, bobId);
  expect(document.status).toBe("unclaimed");
  expect(document.hostname).toBeNull();
});

test("a display name that reads as an id or hides a control character is refused", async () => {
  const before = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  const headSeq = before.body.identity.head_seq;

  // U+202E is a right-to-left override and U+200B a zero-width space: both
  // reorder or hide what a reader sees without changing the bytes.
  const cases: [string, string][] = [
    [bobId, "it parses as an identity id"],
    ["Alice\u202eExample", "it holds a bidi control character"],
    ["Alice\u200bExample", "it holds a zero-width or invisible format character"],
  ];
  for (const [displayName, reason] of cases) {
    const refused = json(
      expectExit(
        mabel("alice", [
          "profile",
          "replace",
          "--identity",
          "alice",
          "--display-name",
          displayName,
          "--hostname",
          "alice.example",
          "--yes",
          "--json",
        ]),
        10,
      ),
    );
    expect(refused.ok).toBe(false);
    expect(refused.code).toBe(10);
    // The fold names one class for every unacceptable name, so a name that
    // could pass for an id and a name carrying an invisible control both
    // answer `invalid_display_name`.
    expect(refused.details.reason).toBe("invalid_display_name");
    expect(refused.message).toBe(
      `Schema error: ProfileUpdate.display_name is not an acceptable display name: ${reason}`,
    );
  }

  const after = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(after.body.identity.head_seq).toBe(headSeq);
});

test("step 9: the private note is alice's alone", async () => {
  const set = json(
    expectExit(
      mabel("alice", [
        "contact",
        "set",
        bobId,
        "--nickname",
        "Bob from the pub",
        "--note",
        "met at the meetup",
        "--json",
      ]),
      0,
    ),
  );
  expect(set.ok).toBe(true);
  expect(set.identity_id).toBe(bobId);
  expect(set.contact.nickname).toBe("Bob from the pub");
  expect(set.contact.note).toBe("met at the meetup");

  // contacts/<bob_id>.json in alice's home, and nowhere else.
  expect(
    stdoutLines(expectExit(dcExec("alice", ["ls", "/data/contacts"]), 0)),
  ).toEqual([`${bobId}.json`]);
  expect(dcExec("bob", ["test", "-e", "/data/contacts"]).status).not.toBe(0);
  expect(dcExec("witness", ["test", "-e", "/data/contacts"]).status).not.toBe(0);

  const read = await apiGet(ALICE_URL, `/api/identities/${bobId}/contact`);
  expect(read.body.contact.nickname).toBe("Bob from the pub");
  // The store accepts a foreign id: bob's ledger is not in alice's home.
  expect(dcExec("alice", ["test", "-e", `/data/identities/${bobId}`]).status).not.toBe(0);

  const written = await apiPut(ALICE_URL, `/api/identities/${bobId}/contact`, {
    nickname: "Bob at the print shop",
    note: "met at the meetup",
  });
  expect(written.status).toBe(200);
  expect(written.body.contact.nickname).toBe("Bob at the print shop");
  const shown = json(expectExit(mabel("alice", ["contact", "show", bobId, "--json"]), 0));
  expect(shown.contact.nickname).toBe("Bob at the print shop");

  // Nothing was signed and nothing was pushed: bob's own wallet holds no note
  // about himself and his head has not moved.
  const bobSide = await apiGet(BOB_URL, `/api/identities/${bobId}/contact`);
  expect(bobSide.body.contact).toBeNull();
});

test("step 10: the graph is synchronized from the UI, and carol is two hops away", async () => {
  // Bob's profile events matter to the crawl, so his ledger goes to the
  // witness before alice reads it from there.
  expectExit(dcSh("bob", 'mabel sync push --identity bob --peer "$(cat /shared/witness.ticket)"'), 0);

  // The sync reads what witnesses hold, so the one control that starts one
  // lives on the witnesses screen (decision 017). There is no graph screen and
  // nothing in the header starts a sync.
  await alicePage.goto(`${ALICE_URL}/wallet`);
  await expect(alicePage.getByTestId("graph-sync")).toHaveCount(0);
  await alicePage.getByTestId("nav-witnesses").click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/witnesses`);
  await expect(alicePage.getByTestId("graph-sync")).toBeVisible();
  await expect(alicePage.getByTestId("graph-sync-state")).toHaveText(
    "Your wallet has not looked yet.",
  );
  await alicePage.getByTestId("graph-sync-button").click();

  // The first sync in this node home states what becomes observable before
  // anything is fetched.
  await expect(alicePage.getByTestId("graph-sync-consent")).toBeVisible();
  await expect(alicePage.getByTestId("graph-sync-consent")).toContainText(GRAPH_CONSENT_FIRST);
  await expect(alicePage.getByTestId("graph-sync-consent")).toContainText(GRAPH_CONSENT_SECOND);
  await expect(alicePage.getByTestId("graph-sync-consent-confirm")).toHaveText("Look now");
  await alicePage.getByTestId("graph-sync-consent-confirm").click();
  // The counts left the screen with developer mode; what the card says is when
  // the wallet last looked, and the counts are pinned on GET /api/graph below.
  await expect(alicePage.getByTestId("graph-sync-state")).toHaveText(
    "Your wallet last looked just now.",
  );

  // The consent is remembered per node home, so the second sync from the same
  // wallet runs without asking again.
  const synced = alicePage.waitForResponse(
    (response) =>
      response.url().endsWith("/api/graph/sync") && response.request().method() === "POST",
  );
  await alicePage.getByTestId("graph-sync-button").click();
  await synced;
  await expect(alicePage.getByTestId("graph-sync-consent")).toHaveCount(0);

  const graph = await apiGet(ALICE_URL, "/api/graph");
  expect(graph.body.graph.node_count).toBe(3);
  expect(graph.body.graph.edge_count).toBe(3);
  expect(graph.body.graph.roots).toHaveLength(1);
  expect(graph.body.graph.roots[0].identity_id).toBe(aliceId);
  expect(graph.body.graph.stale).toBe(false);
  firstSyncId = graph.body.graph.sync_id;

  const answer = await apiGet(ALICE_URL, `/api/lookup/${carolId}?from=${aliceId}`);
  expect(answer.body.degrees).toBe(2);
  expect(answer.body.paths).toHaveLength(1);
  const hops = answer.body.paths[0].hops;
  expect(hops).toHaveLength(2);
  expect(hops[0].from.identity_id).toBe(aliceId);
  expect(hops[0].to.identity_id).toBe(bobId);
  expect(hops[0].to.display_name).toBe("Bob Example");
  expect(hops[1].from.identity_id).toBe(bobId);
  expect(hops[1].to.identity_id).toBe(carolId);
  expect(hops[1].attestation_event).toBe(carolAttestation);
  for (const hop of hops) {
    expect(typeof hop.fetched_at_ms).toBe("number");
    expect(hop.stale).toBe(false);
  }
  expect(answer.body.graph_stale).toBe(false);
  expect(answer.body.graph_truncated).toBe(false);
  expect(answer.body.truncated_by).toBeNull();
  expect(answer.body.trust).toEqual([]);
  expect(answer.body.reverse.best_effort).toBe(true);
  expect(answer.body.reverse.entries).toHaveLength(1);
  expect(answer.body.reverse.entries[0].identity.identity_id).toBe(bobId);
  expect(answer.body.sync_id).toBe(firstSyncId);

  // The CLI answers the same question from the stored generation. A CLI sync
  // needs the witness ticket, because that process holds no seeded address.
  expectExit(dcSh("alice", 'mabel graph sync --peer "$(cat /shared/witness.ticket)"'), 0);
  const document = json(
    expectExit(mabel("alice", ["lookup", carolId, "--from", "alice", "--json"]), 0),
  );
  expect(document.degrees).toBe(2);
  expect(document.paths[0].hops).toHaveLength(2);

  const text = expectExit(mabel("alice", ["lookup", carolId, "--from", "alice"]), 0);
  const lines = stdoutLines(text);
  expect(lines[0]).toBe(`(no name) (${shown(carolId)})`);
  expect(lines[1]).toBe(`from Alice Example (${shown(aliceId)})`);
  expect(lines[2]).toBe("2 degrees in this crawl");
  expect(lines[5]).toBe("0 attestations out, 1 in (best effort: who this crawl read)");

  // Carol's page is the identity page, reached by pasting her id into the one
  // search box on the wallet home. The crawl's answer renders on it.
  await searchIdentity(alicePage, ALICE_URL, carolId, carolId);
  await expect(alicePage.getByTestId("identity-detail")).toBeVisible();
  expect(await identifier(alicePage, "identity-detail-resolved")).toBe(carolId);
  await expect(alicePage.getByTestId("identity-detail-ledger-summary")).toHaveText(
    "your wallet holds no copy of it",
  );
  // Round 5 removed the provenance row: which of the three sources a label came
  // from is a fact about the label, not about the identity, and the card already
  // shows what it has. A card with no copy of the record says that in a pill.
  await expect(alicePage.getByTestId("identity-detail-provenance")).toHaveCount(0);
  await expect(alicePage.getByTestId("identity-detail-unheld")).toHaveText("not stored here");
  // Nothing about a foreign page pretends this wallet can act for it. The pill
  // proposal 005 draws here is the amber distance from the stored crawl, never
  // "your identity".
  const carolPill = alicePage.getByTestId("identity-detail-resolved-pill");
  await expect(carolPill).toHaveAttribute("data-pill", "degree");
  await expect(carolPill).toHaveText("trusted (2d)");
  await expect(alicePage.getByTestId("identity-actions")).toHaveCount(0);
  // The one action a foreign page offers besides fetching is the local info,
  // and round 5 gave it the name of the task and one button that writes both
  // fields.
  await openAction(alicePage, "lookup-contact");
  await expect(alicePage.getByTestId("lookup-contact-summary")).toContainText("Update local info");
  await expect(alicePage.getByTestId("contact-save")).toHaveText("Save");
  await expect(
    alicePage.getByTestId("contact-panel").locator('button[type="submit"]'),
  ).toHaveCount(1);

  await expect(alicePage.getByTestId("lookup-result")).toBeVisible();
  await expect(alicePage.getByTestId("lookup-from")).toHaveAttribute(
    "data-identity-id",
    aliceId,
  );
  // Round 5 made the verdict a sentence rather than a number in a row.
  await expect(alicePage.getByTestId("lookup-degrees")).toHaveText("Connected through 2 steps");
  await expect(alicePage.getByTestId("lookup-degrees-row")).toHaveCount(0);
  await expect(alicePage.getByTestId("lookup-verdict-pill")).toHaveAttribute("data-pill", "degree");
  // The path is a vertical chain of the same identity cards every other screen
  // draws: the root you asked from, then one card per step.
  await expect(alicePage.getByTestId("lookup-path-0")).toBeVisible();
  await expect(alicePage.getByTestId("lookup-hop-0-0-from-name")).toHaveText("Alice Example");
  await expect(alicePage.getByTestId("lookup-hop-0-0-to-name")).toHaveText("Bob Example");
  await expect(alicePage.getByTestId("lookup-hop-0-0")).toBeVisible();
  await expect(alicePage.getByTestId("lookup-hop-0-1")).toBeVisible();
  await expect(alicePage.getByTestId("lookup-hop-0-1-fetched")).toContainText("seen ");

  // The two lists are collapsed cards. Their headings are plain sentences, and
  // the caveat the reverse heading used to carry is the sentence its info tip
  // holds, which is the tip's accessible name.
  await expect(alicePage.getByTestId("lookup-trust-label")).toHaveText("Who they trust");
  await expect(alicePage.getByTestId("lookup-reverse-label")).toHaveText(REVERSE_HEADING);
  await expect(alicePage.getByTestId("lookup-reverse-note")).toHaveAttribute(
    "aria-label",
    REVERSE_LABEL,
  );
  // A closed block holds none of its content, so each list is opened to read it.
  // The list is opened by its heading, which is what a reader clicks: the info
  // icon sits inside the same row, opens its own sentence and stops the click
  // there, so a click aimed at the middle of the row can land on the icon
  // instead of on the row.
  await expect(alicePage.getByTestId("lookup-trust-empty")).toHaveCount(0);
  await alicePage.getByTestId("lookup-trust-label").click();
  await expect(alicePage.getByTestId("lookup-trust-toggle")).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  await expect(alicePage.getByTestId("lookup-trust-empty")).toHaveText(
    "Your wallet has not seen them trust anyone.",
  );
  await alicePage.getByTestId("lookup-reverse-label").click();
  await expect(alicePage.getByTestId("lookup-reverse-toggle")).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  // Round 5 draws each entry as the same identity card, keyed by the identity,
  // so the per-entry expand controls are gone.
  await expect(
    alicePage.getByTestId("lookup-reverse").getByTestId(`identity-card-${bobId}`),
  ).toBeVisible();
  await expect(alicePage.getByTestId(`lookup-reverse-row-${bobId}`)).toHaveCount(0);
  await expect(alicePage.getByTestId(`lookup-reverse-expand-${bobId}`)).toHaveCount(0);
  // This crawl is fresh and reached everything, so neither disclosure is drawn.
  await expect(alicePage.getByTestId("lookup-graph-stale")).toHaveCount(0);
  await expect(alicePage.getByTestId("lookup-graph-truncated")).toHaveCount(0);
});

test("step 10: the wallet home lists who it knows of, and the tab narrows it", async () => {
  // Round 6 of proposal 005 added the third section and the route behind it:
  // every identity this home has a record of and does not control. Bob and
  // carol are both crawl nodes here, neither is stored, and alice noted bob in
  // step 9, so his row also comes from `contacts/`.
  const known = await apiGet(ALICE_URL, "/api/identities/known");
  expect(known.status).toBe(200);
  const rows: Record<string, any> = Object.fromEntries(
    known.body.identities.map((row: any) => [row.identity_id, row]),
  );
  // The witness is here too, and by the same rule: naming it on a chain means
  // resolving it first, and this home kept the copy it read (proposal 006
  // section 5.1). A witness is an identity, so it is a row like any other.
  expect(Object.keys(rows).sort()).toEqual([bobId, carolId, witnessIdentity].sort());
  expect(rows[witnessIdentity].stored).toBe(true);
  expect(rows[witnessIdentity].declared_kind).toBe("service");
  // Alice attested bob herself, so he is trusted at one step; carol is only
  // reachable through him, and nothing in this home signs for either.
  expect(rows[bobId].trusted).toBe(true);
  expect(rows[bobId].degrees).toBe(1);
  expect(rows[bobId].stored).toBe(false);
  expect(rows[bobId].head_seq).toBeNull();
  expect(rows[bobId].alias).toBe("Bob at the print shop");
  expect(rows[carolId].trusted).toBe(false);
  expect(rows[carolId].degrees).toBe(2);
  expect(rows[carolId].stored).toBe(false);
  // The document is sorted by the rendered id, which is what a client can
  // reproduce from it.
  expect(known.body.identities.map((row: any) => row.identity_id)).toEqual(
    [...known.body.identities.map((row: any) => row.identity_id)].sort(),
  );

  await alicePage.goto(`${ALICE_URL}/wallet`);
  await expect(alicePage.getByTestId("known-identities")).toContainText("Known identities");
  const cards = alicePage.getByTestId("known-identity-cards");
  await expect(cards.getByTestId(`identity-card-${bobId}`)).toBeVisible();
  await expect(cards.getByTestId(`identity-card-${carolId}`)).toBeVisible();
  // Every row here is a name this wallet does not sign for, and each says what
  // it says: bob's own pill is green, carol's is the amber distance.
  await expect(alicePage.getByTestId(`identity-card-name-${bobId}-pill`)).toHaveAttribute(
    "data-pill",
    "trusted",
  );
  await expect(alicePage.getByTestId(`identity-card-name-${carolId}-pill`)).toHaveAttribute(
    "data-pill",
    "degree",
  );
  // Neither record is in this home, so both cards say so in the corner pill.
  await expect(alicePage.getByTestId(`identity-card-unheld-${bobId}`)).toHaveText(
    "not stored here",
  );
  await expect(alicePage.getByTestId(`identity-card-unheld-${carolId}`)).toHaveText(
    "not stored here",
  );

  // The second tab narrows the list to the ones this wallet has a reason to
  // trust. Carol is reachable through bob, so she survives it; the empty case
  // is what story 001 reads on a wallet that has crawled nothing.
  await expect(alicePage.getByTestId("known-identities-filter")).toHaveAttribute(
    "role",
    "tablist",
  );
  const trustedOnly = alicePage.getByTestId("known-identities-trusted");
  await expect(trustedOnly).toHaveAttribute("role", "tab");
  await expect(trustedOnly).toHaveAttribute("aria-selected", "false");
  await expect(alicePage.getByTestId("known-identities-all")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await trustedOnly.click();
  await expect(trustedOnly).toHaveAttribute("aria-selected", "true");
  await expect(alicePage.getByTestId("known-identities-all")).toHaveAttribute(
    "aria-selected",
    "false",
  );
  await expect(cards.getByTestId(`identity-card-${bobId}`)).toBeVisible();
  await expect(cards.getByTestId(`identity-card-${carolId}`)).toBeVisible();
  // Alice keeps a copy of the witness and trusts nobody through it, so the
  // tab drops it: holding a record is not a reason to trust its subject.
  await expect(cards.getByTestId(`identity-card-${witnessIdentity}`)).toHaveCount(0);
});

test("step 11: an identity nobody in this crawl trusts answers with no path", async () => {
  const document = json(
    expectExit(mabel("alice", ["lookup", witnessId, "--from", "alice", "--json"]), 0),
  );
  expect(document.ok).toBe(true);
  expect(document.degrees).toBeNull();
  expect(document.paths).toEqual([]);
  expect(document.trust).toEqual([]);
  expect(document.reverse).toEqual({ best_effort: true, entries: [] });

  const text = expectExit(mabel("alice", ["lookup", witnessId, "--from", "alice"]), 0);
  expect(stdoutLines(text)[2]).toBe(
    "no path in this crawl, which is not the same as no relationship",
  );

  const answer = await apiGet(ALICE_URL, `/api/lookup/${witnessId}?from=${aliceId}`);
  expect(answer.status).toBe(200);
  expect(answer.body.degrees).toBeNull();

  await searchIdentity(alicePage, ALICE_URL, witnessId, witnessId);
  await expect(alicePage.getByTestId("lookup-result")).toBeVisible();
  await expect(alicePage.getByTestId("lookup-degrees")).toHaveText("No connection found");
  await expect(alicePage.getByTestId("lookup-degrees-none")).toHaveText(
    "No connection found yet.",
  );
  // No distance means nothing to say in a pill either: a pill reading trusted
  // beside "no connection found" would be two answers to one question.
  await expect(alicePage.getByTestId("lookup-verdict-pill")).toHaveCount(0);
  await expect(alicePage.getByTestId("lookup-paths")).toHaveCount(0);
});

test("step 12: the search box takes a hostname and opens the identity it names", async () => {
  // The wallet's one search box resolves a hostname through the node and
  // navigates to the id the TXT record names (proposal 004). It verifies
  // nothing: the identity's own advisory verdict is what the page draws.
  const resolved = await apiGet(ALICE_URL, "/api/resolve?input=alice.example");
  expect(resolved.body.status).toBe("resolved");
  expect(resolved.body.identity_id).toBe(aliceId);

  // The label names the endpoints that answer for her beside the id it claims,
  // so they ride to the identity page with her (proposal 006 section 6).
  expect(resolved.body.endpoints).toEqual([aliceNodeId, witnessId].sort());
  await searchIdentity(alicePage, ALICE_URL, "alice.example", aliceId, resolved.body.endpoints);
  await expect(alicePage.getByTestId("identity-detail")).toBeVisible();
  expect(await identifier(alicePage, "identity-detail-resolved")).toBe(aliceId);
  await expect(alicePage.getByTestId("identity-detail-resolved-pill")).toHaveText("your identity");
  await expect(alicePage.getByTestId("identity-detail-hostname-verification")).toHaveAttribute(
    "data-verification",
    "verified",
  );

  // A hostname the resolver answers for and no mabel record backs says what
  // the lookup answered, and navigates nowhere.
  await alicePage.goto(`${ALICE_URL}/wallet`);
  await alicePage.getByTestId("wallet-search-input").fill("nobody.example");
  await alicePage.getByTestId("wallet-search-submit").click();
  const status = alicePage.getByTestId("wallet-search-status");
  await expect(status).toHaveAttribute("data-status", "no_record");
  await expect(status).toContainText("_mabel.nobody.example.");
  await expect(status).toContainText("names no identity");
  await expect(alicePage).toHaveURL(`${ALICE_URL}/wallet`);

  const missing = await apiGet(ALICE_URL, "/api/resolve?input=nobody.example");
  expect(missing.body.status).toBe("no_record");
  expect(missing.body.identity_id).toBeNull();
});

test("a label's endpoints are read whole, and a link resolves the same way", async () => {
  // `mabel-endpoints=` sits beside `mabel=` at the same label and names the
  // endpoints that answer for whatever identity that label claims. Alice's
  // label carries two, split across two character-strings, and the reader
  // joins them back before it parses anything (proposal 006 section 6).
  const alice = await apiGet(ALICE_URL, "/api/resolve?input=alice.example");
  expect(alice.body.input_kind).toBe("hostname");
  expect(alice.body.status).toBe("resolved");
  expect(alice.body.identity_id).toBe(aliceId);
  expect(alice.body.endpoints).toEqual([aliceNodeId, witnessId].sort());

  // Five endpoints do not fit one character-string, so the zone splits them
  // after the fourth comma. All five come back, sorted by their rendered form.
  const many = await apiGet(ALICE_URL, "/api/resolve?input=many-machines.example");
  expect(many.body.status).toBe("resolved");
  expect(many.body.identity_id).toBe(MANY_MACHINES_IDENTITY);
  expect(many.body.endpoints).toEqual([...MANY_MACHINES].sort());

  // A label with no `mabel-endpoints=` record answers with no endpoints, which
  // is not a failure: the identity is still named.
  const bob = await apiGet(ALICE_URL, "/api/resolve?input=bob.example");
  expect(bob.body.status).toBe("resolved");
  expect(bob.body.endpoints).toEqual([]);

  // The third kind of input the box takes. The browser parses no link: it
  // hands the string to the node, which owns the grammar and answers with the
  // identity and the endpoints the link named (proposal 006 section 7).
  const link = json(
    expectExit(
      mabel("bob", ["identity", "share", "carol", "--endpoints", witnessId, "--json"]),
      0,
    ),
  ).link;
  expect(link).toBe(`mabel://${carolId}?endpoints=${witnessId}`);

  const resolved = await apiGet(ALICE_URL, `/api/resolve?input=${encodeURIComponent(link)}`);
  expect(resolved.body.input_kind).toBe("link");
  expect(resolved.body.identity_id).toBe(carolId);
  expect(resolved.body.endpoints).toEqual([witnessId]);
  // A link queries nothing, so there is no lookup status to report.
  expect(resolved.body.status).toBeNull();
  expect(resolved.body.hostname).toBeNull();

  // The box opens carol's page and carries the link's endpoints with it, so the
  // fetch there dials them; viewing still writes nothing.
  await searchIdentity(alicePage, ALICE_URL, link, carolId, [witnessId]);
  await expect(alicePage.getByTestId("identity-fetch")).toBeVisible();
  await expect(alicePage.getByTestId("identity-fetch-link-note")).toBeVisible();
  expect(stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0)).sort()).toEqual(
    [aliceId, witnessIdentity].sort(),
  );
});

test("a sync writes a new generation and swaps current.json", async () => {
  const before = json<{ sync_id: string }>(
    expectExit(dcExec("alice", ["cat", "/data/graph/current.json"]), 0),
  );

  // A lookup fired against the same node while a sync runs reads one
  // generation whole: the pointer is replaced by a rename, never edited.
  const [synced, during] = await Promise.all([
    apiPost(ALICE_URL, "/api/graph/sync", {}),
    apiGet(ALICE_URL, `/api/lookup/${carolId}?from=${aliceId}`),
  ]);
  expect(synced.status).toBe(200);
  expect(during.body.degrees).toBe(2);
  expect(during.body.paths[0].hops).toHaveLength(2);
  expect([before.sync_id, synced.body.graph.sync_id]).toContain(during.body.sync_id);

  const after = json<{ sync_id: string }>(
    expectExit(dcExec("alice", ["cat", "/data/graph/current.json"]), 0),
  );
  expect(after.sync_id).toBe(synced.body.graph.sync_id);
  expect(after.sync_id).not.toBe(before.sync_id);
  const generations = stdoutLines(
    expectExit(dcExec("alice", ["ls", "/data/graph/generations"]), 0),
  );
  expect(generations).toContain(after.sync_id);
  // Generations are caches, collected down to the last two.
  expect(generations.length).toBeLessThanOrEqual(2);
  expect(
    dcExec("alice", ["test", "-f", `/data/graph/generations/${after.sync_id}/summary.json`]).status,
  ).toBe(0);
});

test("the crawl writes no stranger's ledger", async () => {
  // Two records on disk: alice's own, and the witness's, which naming a
  // witness read and kept. The crawl added neither of the people it walked.
  const ledgers = stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0));
  expect([...ledgers].sort()).toEqual([aliceId, witnessIdentity].sort());
  expect(ledgers).not.toContain(carolId);
  expect(ledgers).not.toContain(bobId);
});

test("bob taking alice's name changes what is shown, never which id is shown", async () => {
  // Anybody may publish any display name, so bob publishes alice's. The
  // screens that render him must still say which identity he is.
  expectExit(
    mabel("bob", [
      "profile",
      "replace",
      "--identity",
      "bob",
      "--display-name",
      "Alice Example",
      "--yes",
    ]),
    0,
  );
  expectExit(dcSh("bob", 'mabel sync push --identity bob --peer "$(cat /shared/witness.ticket)"'), 0);

  await alicePage.goto(`${ALICE_URL}/witnesses`);
  const synced = alicePage.waitForResponse(
    (response) =>
      response.url().endsWith("/api/graph/sync") && response.request().method() === "POST",
  );
  await alicePage.getByTestId("graph-sync-button").click();
  await synced;

  // Alice's trust card for bob now reads alice's own name, and carries bob's
  // id: the name is what the crawl read, the id is who it is about. Round 4 of
  // proposal 005 keys that card by the subject rather than by the entry.
  await openIdentity(alicePage, ALICE_URL, aliceId);
  await expect(trustCard(alicePage, bobId)).toBeVisible();
  await expect(alicePage.getByTestId(`identity-card-name-${bobId}-name`)).toHaveText(
    "Alice Example",
  );
  // Round 6 draws the nickname this device keeps after the name they publish,
  // so a stolen public name and the name alice gave him are both readable and
  // tellable apart: Alice Example (Bob at the print shop).
  await expect(alicePage.getByTestId(`identity-card-name-${bobId}-nickname`)).toHaveText(
    "(Bob at the print shop)",
  );
  expect(await identifier(alicePage, `identity-card-name-${bobId}`)).toBe(bobId);
  await expect(idSpan(alicePage, `identity-card-name-${bobId}`)).toHaveAttribute(
    "data-truncated",
    "false",
  );
  // The entry that said it is still on the record, which is where it is read.
  const trust = await apiGet(ALICE_URL, `/api/identities/${aliceId}`);
  expect(trust.body.identity.trust[0].attestation_event).toBe(aliceAttestation);
  expect(trust.body.identity.trust[0].subject).toBe(bobId);

  // The overview of alice's own identity carries the same name and her id.
  await expect(alicePage.getByTestId("identity-detail-resolved-name")).toHaveText("Alice Example");
  expect(await identifier(alicePage, "identity-detail-resolved")).toBe(aliceId);
});

test("step 13: the witnesses screen, what one holds, and one deliberate fetch", async () => {
  // A wallet knows a witness from a ledger that names it and from its own
  // defaults; there is no global directory. A witness is an identity, so the
  // rows are identities (proposal 006 section 8).
  const listed = await apiGet(ALICE_URL, "/api/witnesses");
  expect(listed.body.witnesses).toHaveLength(1);
  expect(listed.body.witnesses[0].identity_id).toBe(witnessIdentity);
  // The one endpoint it knows for the witness, and the label the binding rule
  // gives it: the only chain this home ever read for the witness was served by
  // that same endpoint, so nothing independent vouches for it (proposal 006
  // section 4.2).
  expect(listed.body.witnesses[0].endpoints).toEqual([
    { endpoint_id: witnessId, binding: "hinted" },
  ]);
  expect(listed.body.witnesses[0].named_by).toEqual([aliceId]);
  expect(listed.body.witnesses[0].is_node_default).toBe(true);

  await alicePage.goto(`${ALICE_URL}/wallet`);
  await alicePage.getByTestId("nav-witnesses").click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/witnesses`);
  await expect(alicePage.getByTestId("witness-cards")).toBeVisible();
  // One card, and it is the identity card every other screen draws: the Mabel
  // ID, the marker saying this node uses it by default, and one row per
  // endpoint that answers for it.
  expect(await cardIds(alicePage, "witness-cards")).toEqual([witnessIdentity]);
  await expect(alicePage.getByTestId(`witness-default-${witnessIdentity}`)).toHaveText(
    "this node uses it by default",
  );
  // The endpoints that answer for it are rows of its record, which is the half
  // of the card the collapsed one folds away.
  await expandCard(alicePage, witnessIdentity);
  const machineRow = `identity-card-machine-${witnessId}-${witnessIdentity}`;
  expect(await identifier(alicePage, machineRow)).toBe(witnessId);
  // A card's parts are `identity-card-<part>-<id>`, so the sentence beside the
  // endpoint is `identity-card-machine-<endpoint>-note-<identity>`, whose testid
  // keeps the older spelling of the row.
  await expect(
    alicePage.getByTestId(`identity-card-machine-${witnessId}-note-${witnessIdentity}`),
  ).toHaveText("No record we have confirms that this endpoint answers for it.");

  // The witness's page is the identity page, and what it holds is a section of
  // it, asked live over the sync protocol.
  await alicePage.getByTestId(`identity-card-link-${witnessIdentity}`).click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/identities/${witnessIdentity}`);
  await expect(alicePage.getByTestId("witness-holdings")).toBeVisible();
  await expect(alicePage.getByTestId("witness-chosen-by")).toHaveText("1 of your identities");
  await expect(alicePage.getByTestId("witness-node-default")).toHaveText(
    "yes, for the identities that chose no witness of their own",
  );
  const held = await cardIds(alicePage);
  // Four records: the three it keeps for other people and its own, which it
  // serves like any other. A witness is an identity with a record.
  expect([...held].sort(compareIds)).toEqual(
    [aliceId, bobId, carolId, witnessIdentity].sort(compareIds),
  );
  await expect(alicePage.getByTestId(`identity-card-declared-kind-${carolId}`)).toHaveText(
    "person",
  );
  // How much of a record this witness holds is what this listing is about, so
  // the card counts the entries rather than naming the position they end at.
  await expect(alicePage.getByTestId(`identity-card-entries-${carolId}`)).toHaveText("2 entries");

  // One flat list under three tabs, All chosen when the page opens, and the
  // sentence under the heading saying which one is chosen. The tab that is not
  // chosen holds no panel at all, so only one list is ever in the document.
  await expect(alicePage.getByTestId("witness-holdings-filter")).toHaveAttribute(
    "role",
    "tablist",
  );
  for (const [filter, label] of [
    ["all", "All"],
    ["ours", "Yours"],
    ["trusted", "Trusted"],
  ] as const) {
    await expect(alicePage.getByTestId(`witness-holdings-${filter}`)).toHaveText(label);
    await expect(alicePage.getByTestId(`witness-holdings-${filter}`)).toHaveAttribute(
      "role",
      "tab",
    );
  }
  await expect(alicePage.getByTestId("witness-holdings-all")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(alicePage.getByTestId("witness-holdings")).toContainText(
    "Every record this witness holds.",
  );
  // Yours is the records alice's own wallet controls, which is her own alone.
  await alicePage.getByTestId("witness-holdings-ours").click();
  await expect(alicePage.getByTestId("witness-holdings-ours")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(alicePage.getByTestId("witness-holdings-all")).toHaveAttribute(
    "aria-selected",
    "false",
  );
  await expect(alicePage.getByTestId("witness-holdings")).toContainText(
    "The records your own identities control.",
  );
  await expect(alicePage.getByTestId(`identity-card-${aliceId}`)).toBeVisible();
  await expect(alicePage.getByTestId(`identity-card-${bobId}`)).toHaveCount(0);
  await expect(alicePage.getByTestId(`identity-card-${carolId}`)).toHaveCount(0);
  // Trusted is bob, whom alice trusts outright, and carol, whom the crawl
  // reached through him. Alice's own record is neither, so it drops out.
  await alicePage.getByTestId("witness-holdings-trusted").click();
  await expect(alicePage.getByTestId("witness-holdings")).toContainText(
    "The people you trust, and the ones your wallet reaches through them.",
  );
  await expect(alicePage.getByTestId(`identity-card-${bobId}`)).toBeVisible();
  await expect(alicePage.getByTestId(`identity-card-${carolId}`)).toBeVisible();
  await expect(alicePage.getByTestId(`identity-card-${aliceId}`)).toHaveCount(0);
  await alicePage.getByTestId("witness-holdings-all").click();
  expect(await cardIds(alicePage)).toEqual(held);

  // The route behind that section is keyed by the witness identity, and an
  // endpoint id is refused there by name (proposal 006 section 8).
  const proxied = await apiGet(
    ALICE_URL,
    `/api/witnesses/${witnessIdentity}/holdings?offset=0&limit=256`,
  );
  expect(proxied.body.identity_id).toBe(witnessIdentity);
  expect(proxied.body.endpoint_id).toBe(witnessId);
  expect(proxied.body.more).toBe(false);
  expect(proxied.body.ledgers.map((ledger: any) => ledger.ledger_id)).toEqual(held);

  const notAnIdentity = await apiGet(
    ALICE_URL,
    `/api/witnesses/${witnessId}/holdings?offset=0&limit=256`,
  );
  expect(notAnIdentity.status).toBe(404);
  expect(notAnIdentity.body.details.reason).toBe("endpoint_not_identity");
  expect(notAnIdentity.body.message).toBe(
    `${witnessId} is an endpoint this home knows, not a witness identity`,
  );

  // A card opens the identity page, and browsing a witness stored nothing:
  // this home still holds no copy of carol's ledger.
  await alicePage.getByTestId(`identity-card-link-${carolId}`).click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/identities/${carolId}`);
  await expect(alicePage.getByTestId("identity-fetch")).toBeVisible();
  expect(stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0))).not.toContain(
    carolId,
  );

  // Fetching is the one action a page offers for a ledger this home does not
  // hold, and the stored page is its confirmation.
  await alicePage.getByTestId("identity-fetch-button").click();
  await expect(alicePage.getByTestId("ledger-panel")).toBeVisible();
  await expect(alicePage.getByTestId("identity-fetch")).toHaveCount(0);
  // Two entries held, and the position the newest sits at is read on the route.
  await expect(alicePage.getByTestId("identity-detail-event-count")).toHaveText("2");
  await expect(alicePage.getByTestId("identity-detail-unheld")).toHaveCount(0);
  await expectHeadSeq(ALICE_URL, carolId, 1);
  // Storing a ledger is not controlling it: the pill stays the crawl's
  // distance, and no action appears.
  await expect(alicePage.getByTestId("identity-detail-resolved-pill")).toHaveAttribute(
    "data-pill",
    "degree",
  );
  await expect(alicePage.getByTestId("identity-actions")).toHaveCount(0);

  const stored = await apiGet(ALICE_URL, `/api/identities/${carolId}`);
  expect(stored.body.identity.head_seq).toBe(1);
  expect(
    stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0)).sort(),
  ).toEqual([aliceId, carolId, witnessIdentity].sort());

  // A fetch writes `ledgers/<carol_id>` and no link: no key here signs for
  // carol, so nothing was recorded under `identities/`, the wallet home still
  // lists one identity, and that is what leaves the page read-only.
  expect(dcExec("alice", ["test", "-e", `/data/identities/${carolId}`]).status).not.toBe(0);
  const listedAfter = await apiGet(ALICE_URL, "/api/identities");
  expect(listedAfter.body.identities.map((identity: any) => identity.identity_id)).toEqual([
    aliceId,
  ]);
});

test("the handle is set in the UI, with the line DNS needs and the check beside it", async () => {
  // Round 4 of proposal 005 gave the handle its own action: it shows the exact
  // TXT record to publish and holds the check. This runs last, because it is
  // the only step that appends to a ledger nothing after it reads.
  const before = await apiGet(BOB_URL, `/api/identities/${bobId}`);
  expect(before.body.identity.profile.hostname).toBeNull();
  const savedSeq = before.body.identity.head_seq + 1;

  await openIdentity(bobPage, BOB_URL, bobId);
  await openAction(bobPage, "action-handle");
  await expect(bobPage.getByTestId("handle-current")).toHaveText("none");
  await bobPage.getByTestId("handle-input").fill("bob.example");
  await bobPage.getByTestId("handle-submit").click();

  // A handle is public forever, which is stated once per node home before the
  // first one is published.
  await expect(bobPage.getByTestId("handle-consent")).toContainText(
    "Every name, email and handle you set here stays readable forever by anyone who knows this identity's id.",
  );
  await expect(bobPage.getByTestId("handle-consent-confirm")).toHaveText("Publish the handle");
  await bobPage.getByTestId("handle-consent-confirm").click();
  await expect(bobPage.getByTestId("handle-result")).toHaveText(`Saved at position ${savedSeq}.`);
  await expect(bobPage.getByTestId("handle-current")).toHaveText("bob.example");

  // Setting a handle replaces the profile, so the public name travels with it
  // untouched: this action changes the handle and nothing else.
  const after = await apiGet(BOB_URL, `/api/identities/${bobId}`);
  expect(after.body.identity.profile.hostname).toBe("bob.example");
  expect(after.body.identity.profile.display_name).toBe("Alice Example");
  expect(after.body.identity.profile.seq).toBe(savedSeq);

  // The line to add is on the screen, whole, so it can be copied into DNS. The
  // id inside a record value stays bare: what goes into a zone file is the DNS
  // grammar, not what a person reads (decision 019).
  await expect(
    bobPage.getByTestId("handle-txt-record").locator("[data-value]").first(),
  ).toHaveAttribute("data-value", `_mabel.bob.example. IN TXT "mabel=${bobId}"`);
  // Bob advertises no endpoint, so the second line proposal 006 section 6
  // defines has nothing to say and is not drawn.
  expect((await apiGet(BOB_URL, `/api/identities/${bobId}`)).body.identity.endpoints).toEqual([]);
  await expect(bobPage.getByTestId("handle-txt-endpoints-record")).toHaveCount(0);

  // And the check sits in the same action. _mabel.bob.example names carol, so
  // the verdict is mismatched, exactly as the forced route reported earlier.
  await bobPage.getByTestId("verification-check").click();
  await expect(bobPage.getByTestId("verification-mark")).toHaveAttribute(
    "data-verification",
    "mismatched",
  );
  await expect(bobPage.getByTestId("verification-detail")).toHaveText(
    "the mabel= record at _mabel.bob.example. names another identity",
  );
  await expect(bobPage.getByTestId("verification-checked-at-ms")).not.toHaveText("never");
});

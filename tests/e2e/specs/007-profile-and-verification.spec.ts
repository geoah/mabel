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
  story001Steps1to7,
} from "../lib/stories";
import { addTrust, cardIds, identifier, openIdentity, push, searchIdentity } from "../lib/ui";

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

const HOSTNAME_ADVISORY =
  "hostname verification is advisory: it gates no authorization and no ledger validity";
const GRAPH_CONSENT_FIRST =
  "A graph sync tells each contacted witness which identities this wallet cares about.";
const GRAPH_CONSENT_SECOND =
  "It fetches ledgers this home does not hold, and keeps them in a crawl generation, not as replicas.";
const REVERSE_LABEL =
  "best effort: who in this crawl attests to them, never who trusts them in the world";

let alicePage: Page;
let bobPage: Page;

let witnessId = "";
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
 * The zone the test resolver serves, rewritten with the ids this run minted.
 * The serial has to rise or CoreDNS's `file` plugin keeps serving what it
 * loaded, and the health record has to stay or the container goes unhealthy.
 *
 * Both TTLs are one second, positive and negative. A wallet's resolver caches
 * for the TTL it is given, so a longer one would have a check taken seconds
 * after the resolver stopped still answering from that cache.
 */
function zoneText(serial: number, claims: Record<string, string>): string {
  const records = Object.entries(claims)
    .map(([label, identity]) => `_mabel.${label} IN TXT "mabel=${identity}"`)
    .join("\n");
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
    expectExit(mabel("bob", ["witness", "add", "--identity", "carol", "--endpoint", witnessId]), 0);
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

  const ledger = await apiGet(WITNESS_URL, `/api/ledgers/${carolId}`);
  expect(ledger.body.entry.head_seq).toBe(1);
  expect(ledger.body.witnesses).toEqual([witnessId]);

  const bob = await apiGet(BOB_URL, `/api/identities/${bobId}`);
  const attestation = bob.body.identity.trust.find(
    (record: any) => record.subject === carolId,
  );
  expect(attestation.revoked).toBe(false);
  expect(attestation.attestation_event).toBe(carolAttestation);
});

test("steps 2 and 3: the resolver answers, and the TXT records name the run's ids", async () => {
  // Ticket 032's wiring: the wallets start pointed at the test resolver and
  // with the witness as their node-wide witness, which is the crawler's third
  // source (proposal 003 section 3).
  const node = await apiGet(ALICE_URL, "/api/node");
  expect(node.body.witnesses).toEqual([witnessId]);

  // _mabel.bob.example names carol on purpose: a record that exists and
  // claims the wrong identity is what "mismatched" is about.
  writeFileBase64(
    RESOLVER,
    ZONE_PATH,
    Buffer.from(zoneText(2, { alice: aliceId, bob: carolId })).toString("base64"),
  );
  // The `file` plugin rereads the zone within five seconds of the serial
  // rising, so no restart and no healthcheck wait is needed.
  await until(
    "_mabel.alice.example to answer with alice's id",
    () => digFromWallet("mabel-alice", "_mabel.alice.example") === `"mabel=${aliceId}"`,
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
  expect(replaced.previous).toEqual({ display_name: null, hostname: null });
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
  expect(again.message).toBe(
    `Policy error: this profile is already the profile of ${aliceId}: nothing would change`,
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
  await expect(alicePage.getByTestId("identity-detail-verification-note")).toHaveText(
    HOSTNAME_ADVISORY,
  );
  // The name is plain text and the id travels beside it, never instead of it.
  await expect(alicePage.getByTestId("identity-detail-resolved-name")).toHaveText("Alice Example");
  expect(await identifier(alicePage, "identity-detail-identity-id")).toBe(aliceId);
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
  expect(rebound.status).toBe("unverified");
  expect(rebound.checked_at_ms).toBeNull();
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
  await expect(bobPage.getByTestId("identity-detail-hostname")).toHaveText("none claimed");
  await expect(bobPage.getByTestId("identity-detail-hostname-verification")).toHaveCount(0);

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
  await expect(staleRow).toContainText("stale");

  const document = await verification(ALICE_URL, aliceId);
  expect(document.stale).toBe(true);
  expect(document.status).toBe("verified");
});

test("the resolver comes back and alice verifies again", async () => {
  test.setTimeout(240_000);
  expectExit(docker(["start", RESOLVER], 60_000), 0);
  await until(
    "the resolver to answer again",
    () => digFromWallet("mabel-alice", "_mabel.alice.example") === `"mabel=${aliceId}"`,
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

  // The sync control is in the header of every wallet screen, with the counts
  // of the crawl this home holds; there is no graph screen (proposal 004).
  await alicePage.goto(`${ALICE_URL}/wallet`);
  await expect(alicePage.getByTestId("graph-sync")).toBeVisible();
  await expect(alicePage.getByTestId("graph-sync-counts")).toHaveCount(0);
  await alicePage.getByTestId("graph-sync-button").click();

  // The first sync in this node home states what becomes observable before
  // anything is fetched.
  await expect(alicePage.getByTestId("graph-sync-consent")).toBeVisible();
  await expect(alicePage.getByTestId("graph-sync-consent")).toContainText(GRAPH_CONSENT_FIRST);
  await expect(alicePage.getByTestId("graph-sync-consent")).toContainText(GRAPH_CONSENT_SECOND);
  await alicePage.getByTestId("graph-sync-consent-confirm").click();
  await expect(alicePage.getByTestId("graph-sync-counts")).toHaveText(
    "3 identities, 3 attestations",
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
  expect(lines[0]).toBe(`(no name) (${carolId})`);
  expect(lines[1]).toBe(`from Alice Example (${aliceId})`);
  expect(lines[2]).toBe("2 degrees in this crawl");
  expect(lines[5]).toBe("0 attestations out, 1 in (best effort: who this crawl read)");

  // Carol's page is the identity page, reached by pasting her id into the one
  // search box on the wallet home. The crawl's answer renders on it.
  await searchIdentity(alicePage, ALICE_URL, carolId, carolId);
  await expect(alicePage.getByTestId("identity-detail")).toBeVisible();
  expect(await identifier(alicePage, "identity-detail-identity-id")).toBe(carolId);
  await expect(alicePage.getByTestId("identity-detail-ledger-summary")).toHaveText(
    "not stored in this node home",
  );
  await expect(alicePage.getByTestId("identity-detail-provenance")).toHaveText(
    "nothing this home holds, so the id is the only label",
  );
  // Nothing about a foreign page pretends this wallet can act for it.
  await expect(alicePage.getByTestId("identity-own-badge")).toHaveCount(0);
  await expect(alicePage.getByTestId("identity-actions")).toHaveCount(0);

  await expect(alicePage.getByTestId("lookup-result")).toBeVisible();
  await expect(alicePage.getByTestId("lookup-from")).toHaveAttribute(
    "data-identity-id",
    aliceId,
  );
  await expect(alicePage.getByTestId("lookup-degrees")).toHaveText("2 hops");
  // The number is only an answer next to the question the row asks.
  await expect(alicePage.getByTestId("lookup-degrees-row").locator("dt")).toHaveText(
    "shortest path found in this crawl",
  );
  await expect(alicePage.getByTestId("lookup-hop-0-0-to-name")).toHaveText("Bob Example");
  await expect(alicePage.getByTestId("lookup-hop-0-1-fetched")).toContainText("read ");
  await expect(alicePage.getByTestId("lookup-reverse-label")).toHaveText(REVERSE_LABEL);
  await expect(alicePage.getByTestId("lookup-trust-empty")).toHaveText(
    "this crawl read no attestation of theirs",
  );
  await expect(
    alicePage.getByTestId(`lookup-reverse-row-${bobId}`),
  ).toBeVisible();
  // This crawl is fresh and reached everything, so neither disclosure is drawn.
  await expect(alicePage.getByTestId("lookup-graph-stale")).toHaveCount(0);
  await expect(alicePage.getByTestId("lookup-graph-truncated")).toHaveCount(0);
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
  await expect(alicePage.getByTestId("lookup-degrees")).toHaveText("none");
  await expect(alicePage.getByTestId("lookup-degrees-none")).toContainText(
    "no path was found within this crawl's caps",
  );
});

test("step 12: the search box takes a hostname and opens the identity it names", async () => {
  // The wallet's one search box resolves a hostname through the node and
  // navigates to the id the TXT record names (proposal 004). It verifies
  // nothing: the identity's own advisory verdict is what the page draws.
  const resolved = await apiGet(ALICE_URL, "/api/resolve/alice.example");
  expect(resolved.body.status).toBe("resolved");
  expect(resolved.body.identity_id).toBe(aliceId);

  await searchIdentity(alicePage, ALICE_URL, "alice.example", aliceId);
  await expect(alicePage.getByTestId("identity-detail")).toBeVisible();
  expect(await identifier(alicePage, "identity-detail-identity-id")).toBe(aliceId);
  await expect(alicePage.getByTestId("identity-own-badge")).toHaveText("your identity");
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
  await expect(status).toContainText("holds no mabel record");
  await expect(alicePage).toHaveURL(`${ALICE_URL}/wallet`);

  const missing = await apiGet(ALICE_URL, "/api/resolve/nobody.example");
  expect(missing.body.status).toBe("no_record");
  expect(missing.body.identity_id).toBeNull();
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
  const ledgers = stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0));
  expect(ledgers).toEqual([aliceId]);
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

  await alicePage.goto(`${ALICE_URL}/wallet`);
  const synced = alicePage.waitForResponse(
    (response) =>
      response.url().endsWith("/api/graph/sync") && response.request().method() === "POST",
  );
  await alicePage.getByTestId("graph-sync-button").click();
  await synced;

  // Alice's trust row for bob now reads alice's own name, and carries bob's
  // id: the name is what the crawl read, the id is who it is about.
  await openIdentity(alicePage, ALICE_URL, aliceId);
  const row = `trust-subject-${aliceAttestation}`;
  await expect(alicePage.getByTestId(`${row}-name`)).toHaveText("Alice Example");
  expect(await identifier(alicePage, row)).toBe(bobId);

  // The overview of alice's own identity carries the same name and her id.
  await expect(alicePage.getByTestId("identity-detail-resolved-name")).toHaveText("Alice Example");
  expect(await identifier(alicePage, "identity-detail-identity-id")).toBe(aliceId);
});

test("step 13: the witnesses screen, what one holds, and one deliberate fetch", async () => {
  // A wallet knows a witness from a ledger that names it and from its own
  // defaults; there is no global directory (proposal 004).
  const listed = await apiGet(ALICE_URL, "/api/witnesses");
  expect(listed.body.witnesses).toHaveLength(1);
  expect(listed.body.witnesses[0].endpoint_id).toBe(witnessId);
  expect(listed.body.witnesses[0].named_by).toEqual([aliceId]);
  expect(listed.body.witnesses[0].is_node_default).toBe(true);

  await alicePage.goto(`${ALICE_URL}/wallet`);
  await alicePage.getByTestId("nav-witnesses").click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/witnesses`);
  await expect(alicePage.getByTestId("witness-cards")).toBeVisible();
  await expect(alicePage.getByTestId(`witness-card-named-by-${witnessId}`)).toHaveText(
    "named by 1 identity",
  );
  await expect(alicePage.getByTestId(`witness-card-default-${witnessId}`)).toHaveText(
    "node default",
  );
  // The card carries the endpoint id and every identity whose chain names it.
  const onCard = await alicePage
    .getByTestId(`witness-card-${witnessId}`)
    .locator("[data-value]")
    .evaluateAll((elements) => elements.map((element) => element.getAttribute("data-value") ?? ""));
  expect(onCard).toEqual([witnessId, aliceId]);

  // What that witness holds, asked live over the sync protocol and rendered as
  // the same identity card list.
  await alicePage.getByTestId(`witness-card-link-${witnessId}`).click();
  await expect(alicePage.getByTestId("witness-ledgers")).toBeVisible();
  const held = await cardIds(alicePage);
  expect([...held].sort(compareIds)).toEqual([aliceId, bobId, carolId].sort(compareIds));
  await expect(alicePage.getByTestId(`identity-card-declared-kind-${carolId}`)).toHaveText(
    "person",
  );
  await expect(alicePage.getByTestId(`identity-card-head-seq-${carolId}`)).toHaveText(
    "head seq 1",
  );

  const proxied = await apiGet(ALICE_URL, `/api/witnesses/${witnessId}/ledgers?offset=0&limit=256`);
  expect(proxied.body.endpoint_id).toBe(witnessId);
  expect(proxied.body.more).toBe(false);
  expect(proxied.body.ledgers.map((ledger: any) => ledger.ledger_id)).toEqual(held);

  // A card opens the identity page, and browsing a witness stored nothing:
  // this home still holds no copy of carol's ledger.
  await alicePage.getByTestId(`identity-card-link-${carolId}`).click();
  await expect(alicePage).toHaveURL(`${ALICE_URL}/identities/${carolId}`);
  await expect(alicePage.getByTestId("identity-fetch")).toBeVisible();
  expect(stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0))).toEqual([aliceId]);

  // Fetching is the one action a page offers for a ledger this home does not
  // hold, and the stored page is its confirmation.
  await alicePage.getByTestId("identity-fetch-button").click();
  await expect(alicePage.getByTestId("ledger-panel")).toBeVisible();
  await expect(alicePage.getByTestId("identity-fetch")).toHaveCount(0);
  await expect(alicePage.getByTestId("identity-detail-head-seq")).toHaveText("1");
  // Storing a ledger is not controlling it.
  await expect(alicePage.getByTestId("identity-own-badge")).toHaveCount(0);
  await expect(alicePage.getByTestId("identity-actions")).toHaveCount(0);

  const stored = await apiGet(ALICE_URL, `/api/identities/${carolId}`);
  expect(stored.body.identity.head_seq).toBe(1);
  expect(
    stdoutLines(expectExit(dcExec("alice", ["ls", "/data/ledgers"]), 0)).sort(),
  ).toEqual([aliceId, carolId].sort());

  // A fetch writes `ledgers/<carol_id>` and no link: no key here signs for
  // carol, so nothing was recorded under `identities/`, the wallet home still
  // lists one identity, and that is what leaves the page read-only.
  expect(dcExec("alice", ["test", "-e", `/data/identities/${carolId}`]).status).not.toBe(0);
  const listedAfter = await apiGet(ALICE_URL, "/api/identities");
  expect(listedAfter.body.identities.map((identity: any) => identity.identity_id)).toEqual([
    aliceId,
  ]);
});

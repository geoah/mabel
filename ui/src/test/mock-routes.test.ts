import { describe, expect, it } from "vitest";

import {
  acceptInvitation,
  admit,
  ApiError,
  createIdentity,
  fetchIdentity,
  forceVerification,
  getContact,
  getGraph,
  getIdentity,
  getMemberships,
  invite,
  listIdentities,
  listWitnessLedgers,
  listWitnesses,
  lookup,
  removePrincipal,
  replaceProfile,
  resolveInput,
  setContact,
  syncGraph,
} from "@/api/client";
import {
  ACME,
  ALICE,
  BOB,
  CAROL,
  REACHABLE_WITNESS,
  UNREACHABLE_WITNESS,
  UNSTORED_LEDGER,
  seedContact,
  seedGraph,
  seedIdentities,
} from "@/mocks/fixtures";
import { MISMATCHED_HOSTNAME, UNREACHABLE_HOSTNAME } from "@/mocks/store";

/**
 * Every wallet route driven through the client against the mock store, so the
 * harness answers the same documents the component tests assert.
 */

async function rejection(run: () => Promise<unknown>): Promise<ApiError> {
  try {
    await run();
  } catch (thrown) {
    return thrown as ApiError;
  }
  throw new Error("the request was expected to fail");
}

describe("graph", () => {
  it("serves the crawl generation this home holds", async () => {
    const response = await getGraph();

    expect(response.graph?.sync_id).toBe(seedGraph.sync_id);
    expect(response.graph?.truncated_by).toBe("depth");
    expect(response.graph?.roots.map((root) => root.identity_id)).toEqual([ACME, ALICE]);
  });

  it("mints a new generation on a sync, with every local identity as a root", async () => {
    const before = await getGraph();
    const response = await syncGraph();

    expect(response.graph.sync_id).not.toBe(before.graph?.sync_id);
    expect(response.graph.roots.map((root) => root.identity_id)).toEqual([ACME, ALICE]);
    expect(response.graph.roots[1].display_name).toBe("Alice Ashworth");
    expect(response.graph.node_count).toBeGreaterThanOrEqual(3);
    expect((await getGraph()).graph?.sync_id).toBe(response.graph.sync_id);
  });
});

describe("lookup", () => {
  it("answers how the selected identity knows a foreign one", async () => {
    const response = await lookup(CAROL, { from: ALICE });

    expect(response.degrees).toBe(2);
    expect(response.identity.identity_id).toBe(CAROL);
    expect(response.from.display_name).toBe("Alice Ashworth");
    const [path] = response.paths;
    expect(path.hops.map((hop) => hop.to.identity_id)).toEqual([BOB, CAROL]);
    expect(path.hops[0].to.display_name).toBe("Bob Baxter");
    expect(response.reverse.best_effort).toBe(true);
    expect(response.reverse.entries[0].identity.identity_id).toBe(BOB);
    expect(response.graph_truncated).toBe(true);
  });

  it("answers a 200 with no degrees for an identity outside the crawl", async () => {
    const stranger = "q".repeat(52);
    const response = await lookup(stranger, { from: ALICE });

    expect(response.degrees).toBeNull();
    expect(response.paths).toEqual([]);
    expect(response.identity.provenance).toBe("none");
  });

  it("refuses a from that names no identity in this home", async () => {
    const error = await rejection(() => lookup(CAROL, { from: BOB }));

    expect(error.status).toBe(400);
    expect(error.reason).toBe("unknown_from_identity");
  });
});

describe("memberships", () => {
  const acmeKey = seedIdentities.find((identity) => identity.identity_id === ACME)!.principals[0]
    .active_key;
  const descriptor = btoa(JSON.stringify({ identity: ACME, active_key: acmeKey }));

  it("serves the principal set and the invitations of one ledger", async () => {
    const view = await getMemberships(ALICE);

    expect(view.root).toBe("raw");
    expect(view.principals[0].is_root).toBe(true);
    expect(view.invitations).toEqual([]);
  });

  it("carries an invitation from the bundle to the acceptance to the principal set", async () => {
    const invited = await invite(ALICE, {
      by: ALICE,
      role: "controller",
      invitee_descriptor_base64: descriptor,
    });
    expect(invited.invitee).toBe(ACME);
    expect(invited.event.payload_kind).toBe("membership_invitation");
    expect((await getMemberships(ALICE)).invitations[0].status).toBe("open");

    const accepted = await acceptInvitation(ACME, {
      invitation_bundle_base64: invited.invitation_bundle_base64,
    });
    // Alice's ledger keys itself, so a controller there signs as Alice.
    expect(accepted.controller_on_raw_root).toBe(true);
    expect(accepted.warning).toContain(ALICE);

    const admitted = await admit(ALICE, {
      by: ALICE,
      acceptance_base64: accepted.acceptance_base64,
    });
    expect(admitted.invitee).toBe(ACME);
    expect((await getMemberships(ALICE)).invitations[0].status).toBe("accepted");
    expect((await getIdentity(ALICE)).identity.open_invitation_count).toBe(0);

    const removed = await removePrincipal(ALICE, { by: ALICE, target: ACME });
    expect(removed.principal_removed).toBe(true);
    expect((await getMemberships(ALICE)).principals).toHaveLength(1);
  });

  it("refuses removing the raw root of its own ledger", async () => {
    const error = await rejection(() => removePrincipal(ALICE, { by: ALICE, target: ALICE }));

    expect(error.status).toBe(409);
    expect(error.reason).toBe("root_not_removable");
  });
});

describe("contact", () => {
  it("holds a note for a foreign identity", async () => {
    const response = await getContact(BOB);

    expect(response.contact?.nickname).toBe(seedContact.nickname);
  });

  it("round-trips a replacement and names the identity it belongs to", async () => {
    const saved = await setContact(BOB, { nickname: "bob", note: null });

    expect(saved.identity_id).toBe(BOB);
    expect(saved.contact).toMatchObject({ nickname: "bob", note: null });
    expect((await getContact(BOB)).contact?.nickname).toBe("bob");

    await setContact(BOB, { nickname: null, note: null });

    expect((await getContact(BOB)).contact).toBeNull();
  });

  it("reaches the identity document when the id is local", async () => {
    await setContact(ALICE, { nickname: "me", note: "this machine" });

    const identity = await getIdentity(ALICE);
    expect(identity.identity.contact).toMatchObject({ nickname: "me", note: "this machine" });
  });
});

describe("creating an identity with a profile", () => {
  it("appends one profile_update at seq 1, right after the inception", async () => {
    const response = await createIdentity({
      alias: "dana",
      declared_kind: "person",
      display_name: "Dana Dane",
      email: "dana@dana.example",
    });

    expect(response.identity.head_seq).toBe(1);
    expect(response.identity.event_count).toBe(2);
    expect(response.identity.profile).toMatchObject({
      display_name: "Dana Dane",
      hostname: null,
      email: "dana@dana.example",
      seq: 1,
    });
    expect(response.inception_event).toBe(response.identity.identity_id);
  });

  it("leaves a new identity with no profile when neither public field is given", async () => {
    const response = await createIdentity({ alias: "quiet", declared_kind: "person" });

    expect(response.identity.head_seq).toBe(0);
    expect(response.identity.profile).toBeNull();
  });

  it("refuses a misshapen email before it mints anything", async () => {
    const before = (await listIdentities()).identities.length;
    const error = await rejection(() =>
      createIdentity({ alias: "typo", declared_kind: "person", email: "dana.example" }),
    );

    expect(error.reason).toBe("invalid_email");
    expect((await listIdentities()).identities).toHaveLength(before);
  });
});

describe("profile and verification", () => {
  it("refuses a replacement whose effect equals the current profile", async () => {
    const error = await rejection(() =>
      replaceProfile(ALICE, {
        display_name: "Alice Ashworth",
        hostname: "alice.example",
        email: "alice@alice.example",
      }),
    );

    expect(error.status).toBe(409);
    expect(error.reason).toBe("no_op_profile_update");
    expect(error.code).toBe(20);
  });

  it("refuses a body that names only some of the three keys", async () => {
    const error = await rejection(() =>
      replaceProfile(ALICE, { display_name: "Alice A." } as never),
    );

    expect(error.reason).toBe("missing_field");
    expect(error.details.field).toBe("hostname");
  });

  it("refuses an email with no local part, and signs nothing", async () => {
    const error = await rejection(() =>
      replaceProfile(ALICE, { display_name: "Alice A.", hostname: null, email: "@alice.example" }),
    );

    expect(error.status).toBe(400);
    expect(error.reason).toBe("invalid_email");
    expect(error.details.field).toBe("email");
  });

  it("replaces all three fields at once, so an omitted email clears it", async () => {
    const response = await replaceProfile(ALICE, {
      display_name: "Alice A.",
      hostname: null,
      email: null,
    });

    expect(response.previous.email).toBe("alice@alice.example");
    expect(response.profile.email).toBeNull();
    expect(response.event.payload).toEqual({ display_name: "Alice A." });
  });

  it("appends a profile_update and clears the name an omitted field names", async () => {
    const response = await replaceProfile(ALICE, {
      display_name: "Alice A.",
      hostname: null,
      email: "alice@alice.example",
    });

    expect(response.previous).toEqual({
      display_name: "Alice Ashworth",
      hostname: "alice.example",
      email: "alice@alice.example",
    });
    expect(response.event.payload_kind).toBe("profile_update");
    expect(response.event.payload).toEqual({
      display_name: "Alice A.",
      email: "alice@alice.example",
    });

    const identity = await getIdentity(ALICE);
    expect(identity.identity.profile?.hostname).toBeNull();
    // The verdict is bound to the hostname it verified, so a cleared claim is
    // unclaimed again and never keeps the old check.
    expect(identity.identity.verification.status).toBe("unclaimed");
  });

  it("starts a changed hostname at unverified, then verifies it on a forced check", async () => {
    await replaceProfile(ALICE, {
      display_name: "Alice Ashworth",
      hostname: "ashworth.example",
      email: "alice@alice.example",
    });

    const before = await getIdentity(ALICE);
    expect(before.identity.verification.status).toBe("unverified");
    expect(before.identity.verification.checked_at_ms).toBeNull();

    const checked = await forceVerification(ALICE);

    expect(checked.verification.status).toBe("verified");
    expect(checked.verification.stale).toBe(false);
    expect(checked.verification.detail).toContain("_mabel.ashworth.example.");
  });

  it("refuses a forced check on an identity claiming no hostname", async () => {
    const error = await rejection(() => forceVerification(ACME));

    expect(error.status).toBe(409);
    expect(error.reason).toBe("no_hostname_claimed");
  });
});

describe("witnesses", () => {
  it("names every endpoint a stored ledger or node.json points at", async () => {
    const response = await listWitnesses();
    const named = new Map(
      response.witnesses.map((witness) => [witness.endpoint_id, witness]),
    );

    expect(named.get(REACHABLE_WITNESS)?.is_node_default).toBe(true);
    expect(named.get(REACHABLE_WITNESS)?.named_by).toEqual([ALICE]);
    expect(named.get(UNREACHABLE_WITNESS)?.is_node_default).toBe(false);
    expect(named.get(UNREACHABLE_WITNESS)?.named_by).toEqual([ACME]);
    // Ascending endpoint id, like every other list this node serves.
    const ids = response.witnesses.map((witness) => witness.endpoint_id);
    expect(ids).toEqual([...ids].sort());
  });

  it("proxies the ledger list of a witness it can reach", async () => {
    const response = await listWitnessLedgers(REACHABLE_WITNESS);

    expect(response.endpoint_id).toBe(REACHABLE_WITNESS);
    const alice = response.ledgers.find((ledger) => ledger.ledger_id === ALICE);
    expect(alice?.declared_kind).toBe("person");
    expect(alice?.fork_count).toBe(1);
  });

  it("answers 502 witness_unreachable for a witness it cannot dial", async () => {
    const error = await rejection(() => listWitnessLedgers(UNREACHABLE_WITNESS));

    expect(error.status).toBe(502);
    expect(error.reason).toBe("witness_unreachable");
    expect(error.code).toBe(30);
  });

  it("refuses an endpoint id that is not 52 base32 characters", async () => {
    const error = await rejection(() => listWitnessLedgers("witness-one"));

    expect(error.status).toBe(400);
    expect(error.reason).toBe("malformed_endpoint_id");
  });
});

describe("resolve", () => {
  it("names the identity whose profile claims the hostname", async () => {
    const response = await resolveInput("alice.example");

    expect(response.status).toBe("resolved");
    expect(response.identity_id).toBe(ALICE);
  });

  it.each([
    ["nobody.example", "no_record"],
    [MISMATCHED_HOSTNAME, "mismatched_records"],
    [UNREACHABLE_HOSTNAME, "unreachable"],
  ])("answers %s with status %s and no identity", async (hostname, status) => {
    const response = await resolveInput(hostname);

    expect(response.status).toBe(status);
    expect(response.identity_id).toBeNull();
  });

  it("refuses a string that cannot be a hostname", async () => {
    const error = await rejection(() => resolveInput("alice_example"));

    expect(error.status).toBe(400);
    expect(error.reason).toBe("malformed_hostname");
  });
});

describe("fetch", () => {
  it("stores a ledger a witness holds and leaves it uncontrolled", async () => {
    const response = await fetchIdentity(UNSTORED_LEDGER, { from: null });

    expect(response.ledger_id).toBe(UNSTORED_LEDGER);
    expect(response.stored).toBe(response.event_count);
    expect(response.controlled_by).toBeNull();

    const stored = await getIdentity(UNSTORED_LEDGER);
    expect(stored.identity.head_seq).toBe(response.head_seq);
    // Stored is not controlled: the wallet's own list does not grow.
    const listed = (await listIdentities()).identities.map((entry) => entry.identity_id);
    expect(listed).toEqual([ACME, ALICE]);
  });

  it("stores nothing the second time and reports that", async () => {
    await fetchIdentity(UNSTORED_LEDGER, { from: null });
    const again = await fetchIdentity(UNSTORED_LEDGER, { from: null });

    expect(again.stored).toBe(0);
  });

  it("answers 502 ledger_not_held when no source holds the ledger", async () => {
    const error = await rejection(() => fetchIdentity(CAROL, { from: null }));

    expect(error.status).toBe(502);
    expect(error.reason).toBe("ledger_not_held");
  });
});

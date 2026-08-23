import { describe, expect, it } from "vitest";

import {
  ApiError,
  forceVerification,
  getContact,
  getGraph,
  getIdentity,
  lookup,
  replaceProfile,
  setContact,
  syncGraph,
} from "@/api/client";
import { ACME, ALICE, BOB, CAROL, seedContact, seedGraph } from "@/mocks/fixtures";

/**
 * The seven routes ticket 026 added, driven through the client against the mock
 * store, so dev mode and demo mode answer the same documents the tests assert.
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

describe("profile and verification", () => {
  it("refuses a replacement whose effect equals the current profile", async () => {
    const error = await rejection(() =>
      replaceProfile(ALICE, { display_name: "Alice Ashworth", hostname: "alice.example" }),
    );

    expect(error.status).toBe(409);
    expect(error.reason).toBe("no_op_profile_update");
    expect(error.code).toBe(20);
  });

  it("refuses a body that names only one of the two keys", async () => {
    const error = await rejection(() =>
      replaceProfile(ALICE, { display_name: "Alice A." } as never),
    );

    expect(error.reason).toBe("missing_field");
    expect(error.details.field).toBe("hostname");
  });

  it("appends a profile_update and clears the name an omitted field names", async () => {
    const response = await replaceProfile(ALICE, { display_name: "Alice A.", hostname: null });

    expect(response.previous).toEqual({
      display_name: "Alice Ashworth",
      hostname: "alice.example",
    });
    expect(response.event.payload_kind).toBe("profile_update");
    expect(response.event.payload).toEqual({ display_name: "Alice A." });

    const identity = await getIdentity(ALICE);
    expect(identity.identity.profile?.hostname).toBeNull();
    // The verdict is bound to the hostname it verified, so a cleared claim is
    // unclaimed again and never keeps the old check.
    expect(identity.identity.verification.status).toBe("unclaimed");
  });

  it("starts a changed hostname at unverified, then verifies it on a forced check", async () => {
    await replaceProfile(ALICE, { display_name: "Alice Ashworth", hostname: "ashworth.example" });

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

import { describe, expect, it } from "vitest";

import { createIdentity, fetchIdentity, setContact } from "@/api/client";
import { ACME, ALICE, BOB, UNSTORED_LEDGER } from "@/mocks/fixtures";
import { clearMockState, MOCK_STATE_KEY } from "@/mocks/persistence";
import { getIdentity, listIdentities, restoreStore } from "@/mocks/store";

/**
 * The mock store keeps what a visitor did in localStorage, because a fetched
 * record that disappears on the next page load is a lie about the node. These
 * are the three states that matter: a saved snapshot is loaded, a snapshot from
 * another version is thrown away, and the harness reset leaves nothing behind.
 */

function saved(): string | null {
  return globalThis.localStorage.getItem(MOCK_STATE_KEY);
}

/** The ids this home controls, which is what the wallet list answers. */
function controlled(): string[] {
  return listIdentities().identities.map((identity) => identity.identity_id);
}

describe("the mock store across a page load", () => {
  it("writes the seeded state under the versioned key", () => {
    const snapshot = saved();

    expect(snapshot).not.toBeNull();
    expect(JSON.parse(snapshot ?? "{}")).toMatchObject({ version: expect.any(String) });
  });

  it("keeps an identity created before the reload", async () => {
    const created = await createIdentity({ alias: "dana", declared_kind: "person" });

    expect(restoreStore()).toBe(true);

    expect(controlled()).toContain(created.identity.identity_id);
    expect(getIdentity(created.identity.identity_id).identity.alias).toBe("dana");
  });

  it("keeps a fetched record and the entries that came with it", async () => {
    await fetchIdentity(UNSTORED_LEDGER, { from: null });

    expect(restoreStore()).toBe(true);

    const stored = getIdentity(UNSTORED_LEDGER).identity;
    expect(stored.event_count).toBe(stored.head_seq + 1);
    // Storing is not controlling: the wallet's own list did not grow.
    expect(controlled()).toEqual([ACME, ALICE]);
  });

  it("keeps a note this device saved about someone else", async () => {
    await setContact(BOB, { nickname: "bob", note: "met at the fair" });

    expect(restoreStore()).toBe(true);

    expect(getIdentity(BOB).identity.contact?.nickname).toBe("bob");
  });

  it("reseeds when the saved state was written by another version", async () => {
    const created = await createIdentity({ alias: "dana", declared_kind: "person" });
    const snapshot = JSON.parse(saved() ?? "{}") as Record<string, unknown>;
    globalThis.localStorage.setItem(
      MOCK_STATE_KEY,
      JSON.stringify({ ...snapshot, version: "0:not-this-build" }),
    );

    expect(restoreStore()).toBe(false);

    expect(controlled()).toEqual([ACME, ALICE]);
    expect(controlled()).not.toContain(created.identity.identity_id);
  });

  it("reseeds when the saved state does not parse", () => {
    globalThis.localStorage.setItem(MOCK_STATE_KEY, "{not json");

    expect(restoreStore()).toBe(false);

    expect(controlled()).toEqual([ACME, ALICE]);
  });

  it("reseeds after the harness reset clears the key", async () => {
    await createIdentity({ alias: "dana", declared_kind: "person" });

    clearMockState();

    expect(saved()).toBeNull();
    expect(restoreStore()).toBe(false);
    expect(controlled()).toEqual([ACME, ALICE]);
    // The reseed writes itself down again, so the next load starts from it.
    expect(saved()).not.toBeNull();
  });
});

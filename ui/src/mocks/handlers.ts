import { HttpResponse, http } from "msw";

import { MockFailure } from "./store";
import * as store from "./store";

type Body = Record<string, unknown>;

/** Turns a store rejection into the error envelope with its HTTP status. */
function answer(produce: () => unknown): Response {
  try {
    return HttpResponse.json(produce() as Body);
  } catch (error) {
    if (error instanceof MockFailure) {
      return HttpResponse.json(error.body, { status: error.status });
    }
    throw error;
  }
}

/**
 * A mutating route. The store saves what changed before the answer goes out, so
 * the next page load holds what the visitor did rather than the seed.
 */
function change(produce: () => unknown): Response {
  try {
    return answer(produce);
  } finally {
    store.persistStore();
  }
}

function number(url: URL, name: string): number | undefined {
  const raw = url.searchParams.get(name);
  if (raw === null) {
    return undefined;
  }
  const parsed = Number(raw);
  return Number.isNaN(parsed) ? -1 : parsed;
}

export const handlers = [
  http.get("/api/node", () => answer(() => store.nodeInfo())),

  http.get("/api/identities", () => answer(() => store.listIdentities())),

  http.post("/api/identities", async ({ request }) => {
    const body = await request.json();
    return change(() => store.createIdentity(body as Body));
  }),

  // known is a static segment, matched before an identity id can claim it.
  http.get("/api/identities/known", () => answer(() => store.listKnownIdentities())),

  http.get("/api/identities/:identityId", ({ params }) =>
    answer(() => store.getIdentity(String(params.identityId))),
  ),

  http.get("/api/identities/:identityId/keys", ({ params }) =>
    answer(() => store.getIdentityKeys(String(params.identityId))),
  ),

  http.get("/api/identities/:identityId/ledger", ({ params, request }) => {
    const url = new URL(request.url);
    return answer(() =>
      store.getIdentityLedger(String(params.identityId), {
        since: number(url, "since"),
        limit: number(url, "limit"),
      }),
    );
  }),

  http.post("/api/identities/:identityId/witnesses", async ({ params, request }) => {
    const body = await request.json();
    return change(() =>
      store.setIdentityWitnesses(
        String(params.identityId),
        ((body as Body).witnesses ?? []) as string[],
      ),
    );
  }),

  http.post("/api/identities/:identityId/profile", async ({ params, request }) => {
    const body = await request.json();
    return change(() => store.replaceProfile(String(params.identityId), body as Body));
  }),

  http.post("/api/identities/:identityId/verification", ({ params }) =>
    change(() => store.forceVerification(String(params.identityId))),
  ),

  http.get("/api/identities/:identityId/contact", ({ params }) =>
    answer(() => store.getContact(String(params.identityId))),
  ),

  http.put("/api/identities/:identityId/contact", async ({ params, request }) => {
    const body = await request.json();
    return change(() => store.setContact(String(params.identityId), body as Body));
  }),

  http.post("/api/identities/:identityId/fetch", async ({ params, request }) => {
    const body = await request.json();
    return change(() => store.fetchIdentity(String(params.identityId), body as Body));
  }),

  http.get("/api/identities/:identityId/memberships", ({ params }) =>
    answer(() => store.memberships(String(params.identityId))),
  ),

  http.post(
    "/api/identities/:identityId/memberships/invitations",
    async ({ params, request }) => {
      const body = await request.json();
      return change(() => store.invite(String(params.identityId), body as Body));
    },
  ),

  http.post(
    "/api/identities/:identityId/memberships/acceptances",
    async ({ params, request }) => {
      const body = await request.json();
      return change(() => store.acceptInvitation(String(params.identityId), body as Body));
    },
  ),

  http.post(
    "/api/identities/:identityId/memberships/admissions",
    async ({ params, request }) => {
      const body = await request.json();
      return change(() => store.admit(String(params.identityId), body as Body));
    },
  ),

  http.post("/api/identities/:identityId/memberships/removals", async ({ params, request }) => {
    const body = await request.json();
    return change(() => store.removePrincipal(String(params.identityId), body as Body));
  }),

  http.get("/api/lookup/:identityId", ({ params, request }) => {
    const from = new URL(request.url).searchParams.get("from");
    return answer(() =>
      store.lookup(String(params.identityId), { from: from ?? undefined }),
    );
  }),

  http.get("/api/graph", () => answer(() => store.getGraph())),

  http.post("/api/graph/sync", () => change(() => store.syncGraph())),

  http.post("/api/trust", async ({ request }) => {
    const body = await request.json();
    return change(() => store.addTrust(body as Body));
  }),

  http.post("/api/trust/:eventId/revoke", async ({ params, request }) => {
    const body = await request.json();
    return change(() => store.revokeTrust(String(params.eventId), body as Body));
  }),

  http.post("/api/sync/push", async ({ request }) => {
    const body = await request.json();
    return change(() => store.syncPush(body as Body));
  }),

  http.get("/api/witnesses", () => answer(() => store.listWitnesses())),

  http.get("/api/witnesses/:endpointId/ledgers", ({ params, request }) => {
    const url = new URL(request.url);
    return answer(() =>
      store.witnessLedgerList(String(params.endpointId), {
        offset: number(url, "offset"),
        limit: number(url, "limit"),
      }),
    );
  }),

  http.get("/api/resolve", ({ request }) =>
    answer(() => store.resolveInput(new URL(request.url).searchParams.get("input") ?? "")),
  ),

  // The witness routes, all of them reads.

  http.get("/api/ledgers", ({ request }) => {
    const url = new URL(request.url);
    return answer(() =>
      store.listLedgers({ offset: number(url, "offset"), limit: number(url, "limit") }),
    );
  }),

  http.get("/api/ledgers/:ledgerId", ({ params }) =>
    answer(() => store.getLedgerEntry(String(params.ledgerId))),
  ),

  http.get("/api/ledgers/:ledgerId/events", ({ params, request }) => {
    const url = new URL(request.url);
    return answer(() =>
      store.getLedgerEvents(String(params.ledgerId), {
        since: number(url, "since"),
        limit: number(url, "limit"),
      }),
    );
  }),

  http.get("/api/forks", ({ request }) => {
    const url = new URL(request.url);
    const ledgerId = url.searchParams.get("ledger_id");
    return answer(() =>
      store.listForks({
        ledger_id: ledgerId ?? undefined,
        offset: number(url, "offset"),
        limit: number(url, "limit"),
      }),
    );
  }),
];

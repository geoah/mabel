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
    return answer(() => store.createIdentity(body as Body));
  }),

  http.get("/api/identities/:identityId", ({ params }) =>
    answer(() => store.getIdentity(String(params.identityId))),
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
    return answer(() =>
      store.setIdentityWitnesses(
        String(params.identityId),
        ((body as Body).witnesses ?? []) as string[],
      ),
    );
  }),

  http.post("/api/identities/:identityId/profile", async ({ params, request }) => {
    const body = await request.json();
    return answer(() => store.replaceProfile(String(params.identityId), body as Body));
  }),

  http.post("/api/identities/:identityId/verification", ({ params }) =>
    answer(() => store.forceVerification(String(params.identityId))),
  ),

  http.get("/api/identities/:identityId/contact", ({ params }) =>
    answer(() => store.getContact(String(params.identityId))),
  ),

  http.put("/api/identities/:identityId/contact", async ({ params, request }) => {
    const body = await request.json();
    return answer(() => store.setContact(String(params.identityId), body as Body));
  }),

  http.post("/api/identities/:identityId/fetch", async ({ params, request }) => {
    const body = await request.json();
    return answer(() => store.fetchIdentity(String(params.identityId), body as Body));
  }),

  http.get("/api/identities/:identityId/memberships", ({ params }) =>
    answer(() => store.memberships(String(params.identityId))),
  ),

  http.post(
    "/api/identities/:identityId/memberships/invitations",
    async ({ params, request }) => {
      const body = await request.json();
      return answer(() => store.invite(String(params.identityId), body as Body));
    },
  ),

  http.post(
    "/api/identities/:identityId/memberships/acceptances",
    async ({ params, request }) => {
      const body = await request.json();
      return answer(() => store.acceptInvitation(String(params.identityId), body as Body));
    },
  ),

  http.post(
    "/api/identities/:identityId/memberships/admissions",
    async ({ params, request }) => {
      const body = await request.json();
      return answer(() => store.admit(String(params.identityId), body as Body));
    },
  ),

  http.post("/api/identities/:identityId/memberships/removals", async ({ params, request }) => {
    const body = await request.json();
    return answer(() => store.removePrincipal(String(params.identityId), body as Body));
  }),

  http.get("/api/lookup/:identityId", ({ params, request }) => {
    const from = new URL(request.url).searchParams.get("from");
    return answer(() =>
      store.lookup(String(params.identityId), { from: from ?? undefined }),
    );
  }),

  http.get("/api/graph", () => answer(() => store.getGraph())),

  http.post("/api/graph/sync", () => answer(() => store.syncGraph())),

  http.post("/api/trust", async ({ request }) => {
    const body = await request.json();
    return answer(() => store.addTrust(body as Body));
  }),

  http.post("/api/trust/:eventId/revoke", async ({ params, request }) => {
    const body = await request.json();
    return answer(() => store.revokeTrust(String(params.eventId), body as Body));
  }),

  http.post("/api/sync/push", async ({ request }) => {
    const body = await request.json();
    return answer(() => store.syncPush(body as Body));
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

  http.get("/api/resolve/:hostname", ({ params }) =>
    answer(() => store.resolveHostname(String(params.hostname))),
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

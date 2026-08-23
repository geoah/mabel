import { HttpResponse, http } from "msw";

import { walletNode } from "./fixtures";
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
  http.get("/api/node", () => HttpResponse.json(walletNode)),

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

  http.post("/api/verify", async ({ request }) => {
    const body = (await request.json()) as Body;
    if (body.kind === "ledger") {
      return answer(() =>
        store.verifyLedger({
          kind: "ledger",
          ledger_id: String(body.ledger_id),
          from: (body.from as string | null) ?? null,
        }),
      );
    }
    if (body.kind === "trust") {
      return answer(() =>
        store.verifyTrust({
          kind: "trust",
          issuer: String(body.issuer),
          subject: String(body.subject),
          from: (body.from as string | null) ?? null,
        }),
      );
    }
    return HttpResponse.json(
      {
        ok: false,
        code: 10,
        message: "Schema error: kind must be one of trust, ledger",
        details: { reason: "unknown_enum_value", field: "kind", value: String(body.kind) },
      },
      { status: 400 },
    );
  }),
];

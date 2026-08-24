import { screen, waitFor, within } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import type { LookupResponse } from "@/api/types";
import { ACME, ALICE, BOB, CAROL, seedGraph, seedLookup } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { REVERSE_LABEL } from "@/routes/identity/KnowledgeSection";

import { openAction, renderApp } from "./render";

/** The frozen answer with the whole crawl aged past the 24-hour rule. */
function staleAnswer(): LookupResponse {
  return {
    ...seedLookup,
    graph_stale: true,
    stale: true,
    paths: seedLookup.paths.map((path) => ({
      hops: path.hops.map((hop) => ({ ...hop, stale: true })),
    })),
  };
}

function serveLookup(answer: LookupResponse): void {
  server.use(http.get("/api/lookup/:identityId", () => HttpResponse.json(answer)));
}

describe("an identity this wallet can sign for", () => {
  it("carries the pill and the actions, and no contact-only section", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-resolved-pill")).toHaveTextContent(
      "your identity",
    );
    expect(screen.getByTestId("identity-actions")).toBeInTheDocument();
    expect(screen.queryByTestId("lookup-result")).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-fetch")).not.toBeInTheDocument();
  });

  it("keeps the overview, the ledger, the trust list and the principals", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-declared-kind")).toHaveTextContent("person");
    expect(screen.getByTestId("identity-detail-event-count")).toHaveTextContent("9");
    expect(await screen.findByTestId("ledger-events")).toBeInTheDocument();
    expect(screen.getByTestId("trust-panel")).toBeInTheDocument();
    expect(screen.getByTestId("ledger-panel")).toBeInTheDocument();
  });

});

describe("a foreign identity this wallet holds no ledger for", () => {
  it("renders what the crawl knows and one button to fetch the chain", async () => {
    renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("lookup-result");

    expect(screen.queryByTestId("identity-own-badge")).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-actions")).not.toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-ledger-summary")).toHaveTextContent(
      "your wallet holds no copy of it",
    );
    expect(screen.getByTestId("identity-fetch-button")).toBeInTheDocument();
  });

  it("reports the witness that holds no copy of the ledger", async () => {
    const { user } = renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("identity-fetch-button");

    await user.click(screen.getByTestId("identity-fetch-button"));

    const envelope = await screen.findByTestId("identity-fetch-error");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("ledger_not_held");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 30");
  });

  it("renders as a stored ledger once the fetch lands", async () => {
    // BOB is a ledger the mock witness holds and this home does not.
    const { user } = renderApp(`/identities/${BOB}`);
    await screen.findByTestId("identity-fetch-button");
    expect(screen.queryByTestId("ledger-panel")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("identity-fetch-button"));

    // The stored page is the confirmation: the fetch panel goes with the state
    // it described.
    expect(await screen.findByTestId("ledger-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("identity-fetch")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("identity-detail-event-count")).toHaveTextContent("4"),
    );
    // Storing a ledger is not controlling it: no badge and no actions appear.
    expect(screen.queryByTestId("identity-own-badge")).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-actions")).not.toBeInTheDocument();
    expect(screen.getByTestId("lookup-result")).toBeInTheDocument();
  });

  it("offers the contact note for an id whose ledger it does not hold", async () => {
    const { user } = renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("lookup-contact");
    await openAction(user, "lookup-contact");

    await user.type(screen.getByTestId("contact-nickname"), "carol from the market");
    await user.click(screen.getByTestId("contact-save"));

    expect(await screen.findByTestId("contact-result")).toHaveTextContent("Saved ");
  });
});

describe("how you know them", () => {
  it("renders the path as named hops with the freshness of each", async () => {
    renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("lookup-result");

    expect(screen.getByTestId("lookup-from")).toHaveAttribute("data-identity-id", ACME);
    // Acme attests to nobody, so the honest answer from that root is no path.
    expect(screen.getByTestId("lookup-degrees")).toHaveTextContent("No connection found");
    expect(screen.getByTestId("lookup-degrees-none")).toHaveTextContent(
      "No connection found yet.",
    );
  });

  it("draws the path as a chain of identity cards, one step per card", async () => {
    serveLookup(seedLookup);
    renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("lookup-result");

    await waitFor(() =>
      expect(screen.getByTestId("lookup-degrees")).toHaveTextContent("Connected through 2 steps"),
    );
    // The chain reads top to bottom: the root you asked from, then one card per
    // step, each of them the same identity card the wallet draws.
    const chain = screen.getByTestId("lookup-path-0");
    expect(within(chain).getByTestId("lookup-hop-0-0-from")).toHaveAttribute(
      "data-identity-id",
      ALICE,
    );
    const first = within(chain).getByTestId("lookup-hop-0-0");
    expect(within(first).getByTestId("lookup-hop-0-0-to")).toHaveAttribute(
      "data-identity-id",
      BOB,
    );
    expect(within(first).getByTestId("lookup-hop-0-0-fetched")).toHaveTextContent("seen ");
    expect(screen.getByTestId("lookup-hop-0-1-to-link")).toHaveAttribute(
      "href",
      `/identities/${CAROL}`,
    );
  });

  it("keeps the two lists folded away, as identity cards behind their counts", async () => {
    serveLookup(seedLookup);
    const { user } = renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("lookup-reverse-toggle");

    // Closed, the toggles carry the short label and the info icon holds the
    // best-effort sentence the label used to spell out.
    expect(screen.getByTestId("lookup-reverse-label")).toHaveTextContent(
      "Who your wallet has seen trusting them",
    );
    expect(screen.getByTestId("lookup-reverse-note")).toHaveAttribute(
      "aria-label",
      REVERSE_LABEL,
    );
    expect(screen.queryByTestId("lookup-reverse")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("lookup-reverse-toggle"));

    const list = await screen.findByTestId("lookup-reverse");
    expect(within(list).getByTestId(`identity-card-${BOB}`)).toBeInTheDocument();
    expect(within(list).getByTestId(`identity-card-link-${BOB}`)).toHaveAttribute(
      "href",
      `/identities/${BOB}`,
    );
  });

  it("shows an equivocation on the hop that recorded it, with both event ids", async () => {
    serveLookup(seedLookup);
    renderApp(`/identities/${CAROL}`);

    const equivocation = await screen.findByTestId("lookup-hop-0-1-equivocation");
    expect(within(equivocation).getByTestId("lookup-hop-0-1-equivocation-seq")).toHaveTextContent(
      String(seedLookup.equivocation?.at_seq),
    );
    for (const branch of seedLookup.equivocation?.branches ?? []) {
      expect(
        within(equivocation).getByTestId(`lookup-hop-0-1-equivocation-branch-${branch.event}`),
      ).toHaveTextContent(branch.event);
    }
    // The hop says it, so the heading does not say it twice.
    expect(screen.queryByTestId("lookup-equivocation")).not.toBeInTheDocument();
  });

  it("warns above the answer when no path hop carries the equivocation", async () => {
    serveLookup({ ...seedLookup, degrees: null, paths: [] });
    renderApp(`/identities/${CAROL}`);

    const equivocation = await screen.findByTestId("lookup-equivocation");
    expect(equivocation).toHaveTextContent("Two different entries were signed at position");
    expect(screen.getByTestId("lookup-degrees-none")).toBeInTheDocument();
  });

  it("raises the staleness banner and the truncation disclosure", async () => {
    serveLookup(staleAnswer());
    renderApp(`/identities/${CAROL}`);

    const banner = await screen.findByTestId("lookup-graph-stale");
    expect(banner).toHaveTextContent("Your wallet last looked");
    expect(within(banner).getByTestId("lookup-graph-stale-sync")).toBeInTheDocument();
    expect(screen.getByTestId("lookup-graph-truncated")).toHaveTextContent(
      "Your wallet may not have seen everything.",
    );
    expect(screen.getByTestId("lookup-hop-0-0-stale")).toHaveTextContent("may be out of date");
  });

  it("says what it has not seen, rather than implying nobody trusts them", async () => {
    serveLookup({ ...seedLookup, trust: [] });
    const { user } = renderApp(`/identities/${CAROL}`);
    await screen.findByTestId("lookup-trust-toggle");

    await user.click(screen.getByTestId("lookup-trust-toggle"));

    expect(await screen.findByTestId("lookup-trust-empty")).toHaveTextContent(
      "Your wallet has not seen them trust anyone.",
    );
  });

  it("answers from the lowest local identity id, which is what the node defaults to", async () => {
    const asked: string[] = [];
    server.events.on("request:start", ({ request }) => {
      const url = new URL(request.url);
      if (url.pathname === `/api/lookup/${CAROL}`) {
        asked.push(url.searchParams.get("from") ?? "");
      }
    });
    renderApp(`/identities/${CAROL}`);

    await waitFor(() => expect(asked).toEqual([ACME]));
    expect(seedGraph.roots[0].identity_id).toBe(ACME);
  });
});

describe("the routes the four-tab wallet left behind", () => {
  it.each([
    ["/wallet/identities", `/wallet/identities/${ALICE}`],
    ["/wallet/lookup", `/wallet/lookup/${ALICE}`],
  ])("redirects %s to the identity page", async (_name, route) => {
    renderApp(route);

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-resolved")).toHaveTextContent(ALICE);
    expect(screen.queryByTestId("route-not-found")).not.toBeInTheDocument();
  });
});

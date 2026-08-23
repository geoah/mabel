import { screen, waitFor, within } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import type { LookupResponse } from "@/api/types";
import { SELECTED_IDENTITY_KEY } from "@/lib/preferences";
import { ACME, ALICE, BOB, CAROL, seedGraph, seedLookup } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

/** The lookup screen answers from the identity the selector holds. */
function fromAlice(): void {
  globalThis.localStorage.setItem(SELECTED_IDENTITY_KEY, ALICE);
}

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

describe("lookup", () => {
  it("renders the path from the selected identity as named hops", async () => {
    fromAlice();
    renderApp(`/wallet/lookup/${CAROL}`);

    await screen.findByTestId("lookup-result");
    await waitFor(() =>
      expect(screen.getByTestId("lookup-identity")).toHaveAttribute("data-identity-id", CAROL),
    );
    expect(screen.getByTestId("lookup-from")).toHaveAttribute("data-identity-id", ALICE);
    expect(screen.getByTestId("lookup-degrees")).toHaveTextContent("2 hops");

    const first = screen.getByTestId("lookup-hop-0-0");
    expect(within(first).getByTestId("lookup-hop-0-0-from")).toHaveAttribute(
      "data-identity-id",
      ALICE,
    );
    expect(within(first).getByTestId("lookup-hop-0-0-to")).toHaveAttribute(
      "data-identity-id",
      BOB,
    );
    expect(within(first).getByTestId("lookup-hop-0-0-fetched")).toHaveTextContent("read ");
    expect(screen.getByTestId("lookup-hop-0-1-to")).toHaveAttribute("data-identity-id", CAROL);
    // Each hop links to its own lookup, so a path is walkable.
    expect(screen.getByTestId("lookup-hop-0-1-to-link")).toHaveAttribute(
      "href",
      `/wallet/lookup/${CAROL}`,
    );
  });

  it("states a missing path as a fact about this crawl, not about the world", async () => {
    fromAlice();
    const stranger = "q".repeat(52);
    renderApp(`/wallet/lookup/${stranger}`);

    await screen.findByTestId("lookup-result");
    await waitFor(() => expect(screen.getByTestId("lookup-degrees")).toHaveTextContent("none"));
    expect(screen.getByTestId("lookup-degrees-row")).toHaveTextContent(
      "shortest path found in this crawl",
    );
    expect(screen.getByTestId("lookup-degrees-none")).toHaveTextContent(
      "no path was found within this crawl's caps",
    );
    expect(screen.queryByTestId("lookup-paths")).not.toBeInTheDocument();
    expect(screen.queryByTestId("lookup-error")).not.toBeInTheDocument();
  });

  it("labels the reverse list best effort wherever it is drawn", async () => {
    fromAlice();
    const { user } = renderApp(`/wallet/lookup/${CAROL}`);

    await screen.findByTestId("lookup-reverse");
    expect(screen.getByTestId("lookup-reverse-label")).toHaveTextContent(
      "who in this crawl attests to them, never who trusts them in the world",
    );

    await user.click(screen.getByTestId(`lookup-trust-expand-${BOB}`));

    const expansion = await screen.findByTestId(`lookup-trust-expansion-${BOB}`);
    // The nested list carries the label too: it is not printed once per screen.
    await waitFor(() =>
      expect(within(expansion).getByTestId("lookup-reverse-label")).toHaveTextContent(
        "best effort",
      ),
    );
  });

  it("shows an equivocation on the hop that recorded it, with both event ids", async () => {
    fromAlice();
    renderApp(`/wallet/lookup/${CAROL}`);

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
    fromAlice();
    serveLookup({ ...seedLookup, degrees: null, paths: [] });
    renderApp(`/wallet/lookup/${CAROL}`);

    const equivocation = await screen.findByTestId("lookup-equivocation");
    expect(equivocation).toHaveTextContent("two signed events at seq");
    expect(screen.getByTestId("lookup-degrees-none")).toBeInTheDocument();
  });

  it("raises the staleness banner and the truncation disclosure", async () => {
    fromAlice();
    serveLookup(staleAnswer());
    renderApp(`/wallet/lookup/${CAROL}`);

    const banner = await screen.findByTestId("lookup-graph-stale");
    expect(banner).toHaveTextContent("graph is stale, last synced");
    expect(within(banner).getByTestId("lookup-graph-stale-sync")).toBeInTheDocument();
    expect(screen.getByTestId("lookup-graph-truncated")).toHaveTextContent("truncated by depth");
    expect(screen.getByTestId("lookup-hop-0-0-stale")).toHaveTextContent("stale");
  });

  it("stops expanding at two levels", async () => {
    fromAlice();
    const { user } = renderApp(`/wallet/lookup/${CAROL}`);

    await screen.findByTestId(`lookup-trust-expand-${BOB}`);
    await user.click(screen.getByTestId(`lookup-trust-expand-${BOB}`));

    const first = await screen.findByTestId(`lookup-trust-expansion-${BOB}`);
    const deeper = await within(first).findByTestId(`lookup-trust-expand-${CAROL}`);
    await user.click(deeper);

    const second = await within(first).findByTestId(`lookup-trust-expansion-${CAROL}`);
    // Level two is the cap: the row names its own lookup instead of opening.
    await waitFor(() =>
      expect(within(second).getByTestId(`lookup-trust-expand-limit-${BOB}`)).toHaveTextContent(
        "two levels is the cap",
      ),
    );
    expect(within(second).queryByTestId(`lookup-trust-expand-${BOB}`)).not.toBeInTheDocument();
  });

  it("re-asks the question from the identity the selector moves to", async () => {
    fromAlice();
    const asked: string[] = [];
    server.events.on("request:start", ({ request }) => {
      const url = new URL(request.url);
      if (url.pathname === `/api/lookup/${CAROL}`) {
        asked.push(url.searchParams.get("from") ?? "");
      }
    });
    const { user } = renderApp(`/wallet/lookup/${CAROL}`);

    await waitFor(() => expect(asked).toEqual([ALICE]));

    await user.click(screen.getByTestId(`identity-selector-option-${ACME}`));

    await waitFor(() => expect(asked).toEqual([ALICE, ACME]));
    // Acme attests to nobody in this crawl, so the same identity is now unreached.
    await waitFor(() => expect(screen.getByTestId("lookup-degrees")).toHaveTextContent("none"));
  });
});

describe("graph status", () => {
  it("reports the generation, its roots and what truncated it", async () => {
    renderApp("/wallet/lookup");

    await screen.findByTestId("graph-panel");
    await waitFor(() =>
      expect(screen.getByTestId("graph-node-count")).toHaveTextContent(
        String(seedGraph.node_count),
      ),
    );
    expect(screen.getByTestId("graph-edge-count")).toHaveTextContent(String(seedGraph.edge_count));
    expect(screen.getByTestId("graph-truncated-by-name")).toHaveTextContent("depth");
    expect(screen.getByTestId(`graph-root-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`graph-equivocation-${CAROL}`)).toBeInTheDocument();
    expect(screen.queryByTestId("graph-stale-banner")).not.toBeInTheDocument();
  });

  it("raises the staleness banner on a crawl older than a day", async () => {
    server.use(
      http.get("/api/graph", () =>
        HttpResponse.json({ ok: true, graph: { ...seedGraph, stale: true } }),
      ),
    );
    renderApp("/wallet/lookup");

    const banner = await screen.findByTestId("graph-stale-banner");
    expect(banner).toHaveTextContent("graph is stale, last synced");
    expect(within(banner).getByTestId("graph-stale-banner-sync")).toBeInTheDocument();
  });
});

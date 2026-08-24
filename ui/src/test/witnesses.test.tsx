import { screen, within } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, BOB, REACHABLE_WITNESS, UNREACHABLE_WITNESS } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

describe("the witness card list", () => {
  it("names every endpoint this wallet knows of and where it knows it from", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    const card = screen.getByTestId(`witness-card-${REACHABLE_WITNESS}`);
    expect(card).toHaveTextContent(REACHABLE_WITNESS.slice(0, 8));
    // The card names the identities that chose it. How many of them there are
    // is a sentence on the witness's own page, not a line on its card.
    expect(within(card).getByTestId(`witness-card-named-by-${REACHABLE_WITNESS}`)).toHaveTextContent(
      "Alice Ashworth",
    );
    expect(card).not.toHaveTextContent("chosen by");
    expect(
      within(card).getByTestId(`witness-card-default-${REACHABLE_WITNESS}`),
    ).toHaveTextContent("this node uses it by default");
    expect(card).toHaveTextContent(ALICE.slice(0, 8));
  });

  it("draws no node-default marker on an endpoint only a ledger names", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    const card = screen.getByTestId(`witness-card-${UNREACHABLE_WITNESS}`);
    expect(
      within(card).queryByTestId(`witness-card-default-${UNREACHABLE_WITNESS}`),
    ).not.toBeInTheDocument();
    expect(card).toHaveTextContent(ACME.slice(0, 8));
  });

  it("opens what one witness holds as the identity card list", async () => {
    const { user } = renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    await user.click(screen.getByTestId(`witness-card-link-${REACHABLE_WITNESS}`));

    await screen.findByTestId("identity-cards");
    const card = screen.getByTestId(`identity-card-${ALICE}`);
    expect(within(card).getByTestId(`identity-card-declared-kind-${ALICE}`)).toHaveTextContent(
      "person",
    );
    // What a witness holds is how much of the record it has, not a position.
    expect(within(card).getByTestId(`identity-card-entries-${ALICE}`)).toHaveTextContent(
      "4 entries",
    );
    // A card is the identity page, never a witness-only screen.
    expect(screen.getByTestId(`identity-card-link-${ALICE}`)).toHaveAttribute(
      "href",
      `/identities/${ALICE}`,
    );
  });

  it("names the ledger with fork records the witness kept", async () => {
    renderApp(`/witnesses/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId(`identity-card-fork-count-${ALICE}`)).toHaveTextContent(
      "1 conflict",
    );
    expect(
      screen.queryByTestId(`identity-card-fork-count-${ACME}`),
    ).not.toBeInTheDocument();
  });

  it("shows the resolved name of a ledger the crawl reached", async () => {
    renderApp(`/witnesses/${REACHABLE_WITNESS}`);
    await screen.findByTestId(`identity-card-name-${ALICE}-name`);

    expect(screen.getByTestId(`identity-card-name-${ALICE}-name`)).toHaveTextContent(
      "Alice Ashworth",
    );
  });

  // The route pages, and this screen has no page control: it reads the pages.
  it("reads past the first page of what a witness holds", async () => {
    const second = "d".repeat(52);
    server.use(
      http.get("/api/witnesses/:endpointId/ledgers", ({ params, request }) => {
        const offset = Number(new URL(request.url).searchParams.get("offset") ?? "0");
        const ledger = (ledgerId: string) => ({
          ledger_id: ledgerId,
          declared_kind: "person",
          head_seq: 0,
          head_event: "e".repeat(52),
          event_count: 1,
          fork_count: 0,
        });
        return HttpResponse.json({
          ok: true,
          endpoint_id: String(params.endpointId),
          offset,
          limit: 1,
          more: offset === 0,
          ledgers: [ledger(offset === 0 ? ALICE : second)],
        });
      }),
    );

    renderApp(`/witnesses/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-cards");

    expect(await screen.findByTestId(`identity-card-${second}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.queryByTestId("witness-ledgers-capped")).not.toBeInTheDocument();
  });

  it("narrows what a witness holds to yours and to the ones you trust", async () => {
    const { user } = renderApp(`/witnesses/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-cards");

    // Everything it holds, which is what the page opens on.
    expect(screen.getByTestId("witness-holdings-all")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-${BOB}`)).toBeInTheDocument();

    await user.click(screen.getByTestId("witness-holdings-ours"));

    // The ledgers this wallet controls, and nothing else.
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`identity-card-${BOB}`)).not.toBeInTheDocument();

    await user.click(screen.getByTestId("witness-holdings-trusted"));

    // The people one of your identities has vouched for, and nothing you own.
    expect(screen.getByTestId(`identity-card-${BOB}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`identity-card-${ALICE}`)).not.toBeInTheDocument();
  });

  it("says how many of your identities chose this witness, on its own page", async () => {
    renderApp(`/witnesses/${REACHABLE_WITNESS}`);

    expect(await screen.findByTestId("witness-chosen-by")).toHaveTextContent(
      "Chosen by 1 of your identities.",
    );
    expect(screen.getByTestId("witness-chosen-by")).toHaveTextContent(
      "This node uses it by default.",
    );
  });

  it("states an unreachable witness as a fact about the connection", async () => {
    renderApp(`/witnesses/${UNREACHABLE_WITNESS}`);

    const panel = await screen.findByTestId("witness-unreachable");
    expect(panel).toHaveTextContent("could not reach this witness");
    expect(panel).toHaveTextContent("not about the records it keeps");
    expect(screen.getByTestId("witness-unreachable-message")).toHaveTextContent("Network error:");
    expect(screen.queryByTestId("identity-cards")).not.toBeInTheDocument();
  });

  it("fetches nothing into this home while browsing a witness", async () => {
    const methods: string[] = [];
    server.events.on("request:start", ({ request }) => methods.push(request.method));

    const { user } = renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    await user.click(screen.getByTestId(`witness-card-link-${REACHABLE_WITNESS}`));
    await screen.findByTestId("identity-cards");

    expect(methods.length).toBeGreaterThan(0);
    expect(methods.filter((method) => method !== "GET")).toEqual([]);
  });
});

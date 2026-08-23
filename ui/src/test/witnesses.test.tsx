import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, REACHABLE_WITNESS, UNREACHABLE_WITNESS } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

describe("the witness card list", () => {
  it("names every endpoint this wallet knows of and where it knows it from", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    const card = screen.getByTestId(`witness-card-${REACHABLE_WITNESS}`);
    expect(card).toHaveTextContent(REACHABLE_WITNESS.slice(0, 8));
    expect(within(card).getByTestId(`witness-card-named-by-${REACHABLE_WITNESS}`)).toHaveTextContent(
      "named by 1 identity",
    );
    expect(
      within(card).getByTestId(`witness-card-default-${REACHABLE_WITNESS}`),
    ).toHaveTextContent("node default");
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
    expect(within(card).getByTestId(`identity-card-head-seq-${ALICE}`)).toHaveTextContent(
      "head seq 3",
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
      "1 fork record",
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

  it("states an unreachable witness as a fact about the connection", async () => {
    renderApp(`/witnesses/${UNREACHABLE_WITNESS}`);

    const panel = await screen.findByTestId("witness-unreachable");
    expect(panel).toHaveTextContent("could not reach the witness");
    expect(panel).toHaveTextContent("not about the ledgers it keeps");
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

import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { walletNode, witnessNode } from "@/mocks/fixtures";
import { setNodeRole } from "@/mocks/store";

import { renderApp } from "./render";

/**
 * The node page says what `GET /api/node` says, in words: there is no ticket
 * route on either role, so there is no ticket on this page.
 */
describe("the node page", () => {
  it("names its role, its Iroh ID and its counts, one short row each", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.getByTestId("node-role")).toHaveTextContent("wallet");
    const id = screen.getByTestId("node-endpoint-id");
    // The id is whole, not truncated, and it can be copied.
    expect(id.querySelector("[data-value]")).toHaveAttribute("data-value", walletNode.endpoint_id);
    expect(id).toHaveTextContent(walletNode.endpoint_id);
    expect(within(id).getByLabelText("Copy Iroh ID")).toBeInTheDocument();
    // The endpoint id is the Iroh ID everywhere, and never "its id".
    expect(screen.getByTestId("node-endpoint-id-row")).toHaveTextContent("Iroh ID");
    expect(screen.getByTestId("node-relay")).toHaveTextContent("public relays");
    expect(screen.getByTestId("node-identity-count")).toHaveTextContent(
      String(walletNode.identity_count),
    );
    expect(screen.getByTestId("node-version")).toHaveTextContent(walletNode.version);
    expect(screen.getByTestId("node-storage")).toHaveTextContent(" of ");
    // The three explanations this page used to open with are gone.
    const page = screen.getByTestId("node-page").textContent ?? "";
    expect(page).not.toMatch(/program on this computer/);
    expect(page).not.toMatch(/how it is reachable/);
    expect(page).not.toMatch(/where it serves/);
  });

  it("draws every default witness as the witness card, not as a link in a row", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    const list = await screen.findByTestId("node-witness-cards");
    for (const endpointId of walletNode.witnesses) {
      const card = within(list).getByTestId(`node-witness-${endpointId}`);
      expect(within(card).getByTestId(`node-witness-kind-line-${endpointId}`)).toHaveTextContent(
        "witness",
      );
      expect(within(card).getByTestId(`node-witness-link-${endpointId}`)).toHaveAttribute(
        "href",
        `/witnesses/${endpointId}`,
      );
    }
  });

  it("carries no connection ticket, because no route serves one", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.queryByTestId("node-ticket")).not.toBeInTheDocument();
    expect(screen.getByTestId("node-page").textContent ?? "").not.toMatch(/ticket/i);
  });

  it("counts records and conflicts on a witness node", async () => {
    setNodeRole("witness");
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.getByTestId("node-role")).toHaveTextContent("witness");
    expect(screen.getByTestId("node-ledger-count-row")).toHaveTextContent("records");
    expect(screen.getByTestId("node-fork-count")).toBeInTheDocument();
    expect(screen.getByTestId("node-relay")).toHaveTextContent("direct connections only");
    expect(screen.queryByTestId("node-identity-count")).not.toBeInTheDocument();
    expect(witnessNode.relay).toBe("disabled");
  });
});

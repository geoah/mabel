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
  it("names what this node is for, its id and its counts", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.getByTestId("node-role")).toHaveTextContent(
      "holds your identities and signs for them",
    );
    const id = screen.getByTestId("node-endpoint-id");
    // The id is whole, not truncated, and it can be copied.
    expect(id.querySelector("[data-value]")).toHaveAttribute("data-value", walletNode.endpoint_id);
    expect(id).toHaveTextContent(walletNode.endpoint_id);
    expect(within(id).getByLabelText("copy")).toBeInTheDocument();
    expect(screen.getByTestId("node-relay")).toHaveTextContent("through the public relays");
    expect(screen.getByTestId("node-http-bind")).toHaveTextContent(walletNode.http_bind);
    expect(screen.getByTestId("node-identity-count")).toHaveTextContent(
      `${walletNode.identity_count} identities`,
    );
    expect(screen.getByTestId("node-version")).toHaveTextContent(walletNode.version);
    expect(screen.getByTestId("node-storage")).toHaveTextContent(" of ");
  });

  it("links every witness this node uses by default", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    const row = screen.getByTestId("node-witnesses");
    for (const endpointId of walletNode.witnesses) {
      expect(within(row).getByTestId(`node-witness-link-${endpointId}`)).toHaveAttribute(
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

    expect(screen.getByTestId("node-role")).toHaveTextContent(
      "keeps copies of other people's records",
    );
    expect(screen.getByTestId("node-ledger-count")).toHaveTextContent("records");
    expect(screen.getByTestId("node-fork-count")).toBeInTheDocument();
    expect(screen.getByTestId("node-relay")).toHaveTextContent("direct connections only");
    expect(screen.queryByTestId("node-identity-count")).not.toBeInTheDocument();
    expect(witnessNode.relay).toBe("disabled");
  });
});

import { screen, within } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { nodeDocument, REACHABLE_WITNESS } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

/**
 * The node page says what `GET /api/node` says, in words. There is one document
 * on every node and no role in it: what this home can do is read from what it
 * holds (proposal 006 section 8).
 */
describe("the node page", () => {
  it("names its Iroh ID and its counts, one short row each, and no role", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.queryByTestId("node-role")).not.toBeInTheDocument();
    const id = screen.getByTestId("node-endpoint-id");
    // The id is whole, not truncated, and it can be copied.
    expect(id.querySelector("[data-value]")).toHaveAttribute(
      "data-value",
      nodeDocument.endpoint_id,
    );
    expect(id).toHaveTextContent(nodeDocument.endpoint_id);
    expect(within(id).getByLabelText("Copy Iroh ID")).toBeInTheDocument();
    // The endpoint id is the Iroh ID everywhere, and never "its id".
    expect(screen.getByTestId("node-endpoint-id-row")).toHaveTextContent("Iroh ID");
    expect(screen.getByTestId("node-relay")).toHaveTextContent("public relays");
    expect(screen.getByTestId("node-identity-count")).toHaveTextContent("2");
    expect(screen.getByTestId("node-ledger-count-row")).toHaveTextContent("records");
    expect(screen.getByTestId("node-fork-count")).toBeInTheDocument();
    expect(screen.getByTestId("node-version")).toHaveTextContent(nodeDocument.version);
    expect(screen.getByTestId("node-storage")).toHaveTextContent(" of ");
    // The three explanations this page used to open with are gone.
    const page = screen.getByTestId("node-page").textContent ?? "";
    expect(page).not.toMatch(/program on this computer/);
    expect(page).not.toMatch(/how it is reachable/);
    expect(page).not.toMatch(/where it serves/);
  });

  it("names the identities this home keeps records for", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    const row = screen.getByTestId("node-witness-for");
    const [entry] = nodeDocument.witness_for;
    expect(
      within(row).getByTestId(`node-witness-for-${entry.identity}-link`),
    ).toHaveAttribute("href", `/identities/${entry.identity}`);
    expect(screen.getByTestId("node-witness-for-row")).toHaveTextContent("keeps records for");
  });

  it("reads none when this home witnesses for nobody", async () => {
    server.use(
      http.get("/api/node", () =>
        HttpResponse.json({ ...nodeDocument, witness_for: [], fork_count: 0 }),
      ),
    );
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.getByTestId("node-witness-for")).toHaveTextContent("none");
  });

  it("says in one sentence what a home with no keys holds", async () => {
    server.use(
      http.get("/api/node", () =>
        HttpResponse.json({ ...nodeDocument, identity_count: 0, ledger_count: 12 }),
      ),
    );
    renderApp("/node");
    await screen.findByTestId("node-page");

    const sentence = screen.getByTestId("node-no-keys");
    expect(sentence).toHaveTextContent("This home holds no keys");
    expect(sentence).toHaveTextContent("It keeps 12 records");
    expect(sentence).toHaveTextContent("accepts new entries for one identity");
  });

  it("draws every default witness as an identity card", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    const list = await screen.findByTestId("node-witness-cards");
    const card = within(list).getByTestId(`identity-card-${REACHABLE_WITNESS}`);
    expect(within(card).getByTestId(`identity-card-link-${REACHABLE_WITNESS}`)).toHaveAttribute(
      "href",
      `/identities/${REACHABLE_WITNESS}`,
    );
    expect(card).toHaveTextContent("the co-op witness");
  });

  it("carries no connection ticket, because no route serves one", async () => {
    renderApp("/node");
    await screen.findByTestId("node-page");

    expect(screen.queryByTestId("node-ticket")).not.toBeInTheDocument();
    expect(screen.getByTestId("node-page").textContent ?? "").not.toMatch(/ticket/i);
  });
});

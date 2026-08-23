import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import {
  DEVELOPER_MODE_KEY,
  GRAPH_CONSENT_KEY,
  SELECTED_IDENTITY_KEY,
} from "@/lib/preferences";
import { ACME, ALICE, seedGraph } from "@/mocks/fixtures";

import { renderApp } from "./render";

async function openDeveloperMode(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByTestId("app-menu-button"));
  await user.click(screen.getByTestId("developer-mode-toggle"));
}

describe("identity selector", () => {
  it("lists every identity by name with its id beside it", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-selector");

    const acme = screen.getByTestId(`identity-selector-name-${ACME}`);
    expect(within(acme).getByTestId(`identity-selector-name-${ACME}-name`)).toHaveTextContent(
      "Acme Corporation",
    );
    expect(acme.querySelector("[data-value]")).toHaveAttribute("data-value", ACME);
    const alice = screen.getByTestId(`identity-selector-name-${ALICE}`);
    expect(within(alice).getByTestId(`identity-selector-name-${ALICE}-verification`)).toHaveAttribute(
      "data-verification",
      "verified",
    );
  });

  it("selects the lowest identity id until a choice is made", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-selector");

    expect(screen.getByTestId("identity-selector-selected")).toHaveAttribute(
      "data-identity-id",
      ACME,
    );
    expect(globalThis.localStorage.getItem(SELECTED_IDENTITY_KEY)).toBeNull();
  });

  it("remembers the choice under mabel.selected_identity", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-selector");

    await user.click(screen.getByTestId(`identity-selector-option-${ALICE}`));

    expect(globalThis.localStorage.getItem(SELECTED_IDENTITY_KEY)).toBe(ALICE);
    expect(screen.getByTestId("identity-selector-selected")).toHaveAttribute(
      "data-identity-id",
      ALICE,
    );
  });

  it("restores the remembered choice on the next load", async () => {
    globalThis.localStorage.setItem(SELECTED_IDENTITY_KEY, ALICE);
    renderApp("/wallet");
    await screen.findByTestId("identity-selector");

    expect(screen.getByTestId("identity-selector-selected")).toHaveAttribute(
      "data-identity-id",
      ALICE,
    );
    expect(
      screen.getByTestId(`identity-selector-option-${ALICE}`),
    ).toBeChecked();
  });
});

describe("developer mode", () => {
  it("is off by default and holds the node's endpoint id and binds", async () => {
    renderApp("/wallet");
    await screen.findByTestId("node-role");

    expect(screen.queryByTestId("node-endpoint-id")).not.toBeInTheDocument();
    expect(screen.queryByTestId("node-witnesses")).not.toBeInTheDocument();
    expect(screen.getByTestId("node-role")).toHaveTextContent("wallet");
  });

  it("reveals them from the header menu and remembers the toggle", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("node-role");

    await openDeveloperMode(user);

    expect(globalThis.localStorage.getItem(DEVELOPER_MODE_KEY)).toBe("1");
    expect(screen.getByTestId("node-endpoint-id")).toBeInTheDocument();
    expect(screen.getByTestId("node-version")).toBeInTheDocument();

    await user.click(screen.getByTestId("developer-mode-toggle"));

    expect(globalThis.localStorage.getItem(DEVELOPER_MODE_KEY)).toBe("0");
    expect(screen.queryByTestId("node-endpoint-id")).not.toBeInTheDocument();
  });

  it("holds head event ids, principal keys and the raw document on an identity", async () => {
    renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-head-seq")).toHaveTextContent("8");
    expect(screen.queryByTestId("identity-detail-head-event")).not.toBeInTheDocument();
    expect(screen.queryByTestId(`principal-key-${ALICE}`)).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-detail-raw")).not.toBeInTheDocument();
    expect(screen.queryByTestId("verification-detail")).not.toBeInTheDocument();
  });

  it("shows them once the seeded preference says the mode is on", async () => {
    globalThis.localStorage.setItem(DEVELOPER_MODE_KEY, "1");
    renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-head-event")).toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-created-at-ms")).toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-raw")).toHaveTextContent(ALICE);
    expect(screen.getByTestId("verification-detail")).toHaveTextContent("_mabel.alice.example.");
    expect(await screen.findByTestId("ledger-id")).toBeInTheDocument();
  });

  it("keeps every panel reachable while it is off", async () => {
    renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    for (const panel of [
      "profile-panel",
      "verification-panel",
      "contact-panel",
      "witness-config",
      "trust-panel",
      "sync-push",
      "ledger-panel",
      "identity-actions",
    ]) {
      expect(screen.getByTestId(panel)).toBeInTheDocument();
    }
  });
});

describe("graph sync", () => {
  it("shows the counts of the crawl this home holds", async () => {
    renderApp("/wallet");

    expect(await screen.findByTestId("graph-sync-counts")).toHaveTextContent(
      `${seedGraph.node_count} identities, ${seedGraph.edge_count} attestations`,
    );
    expect(screen.getByTestId("graph-sync-truncated")).toHaveTextContent("truncated by depth");
  });

  it("asks for consent once, then synchronizes on the next click", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("graph-sync-button");

    await user.click(screen.getByTestId("graph-sync-button"));

    const consent = screen.getByTestId("graph-sync-consent");
    expect(consent).toHaveTextContent("which identities this wallet cares about");
    expect(globalThis.localStorage.getItem(GRAPH_CONSENT_KEY)).toBeNull();

    await user.click(screen.getByTestId("graph-sync-consent-confirm"));

    await waitFor(() =>
      expect(globalThis.localStorage.getItem(GRAPH_CONSENT_KEY)).toBe("1"),
    );
    expect(screen.queryByTestId("graph-sync-consent")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("graph-sync-button"));

    expect(screen.queryByTestId("graph-sync-consent")).not.toBeInTheDocument();
  });

  it("mints a generation whose provenance developer mode shows", async () => {
    globalThis.localStorage.setItem(DEVELOPER_MODE_KEY, "1");
    globalThis.localStorage.setItem(GRAPH_CONSENT_KEY, "1");
    const { user } = renderApp("/wallet");
    await screen.findByTestId("graph-sync-id");
    const before = screen.getByTestId("graph-sync-id").textContent;

    await user.click(screen.getByTestId("graph-sync-button"));

    await waitFor(() =>
      expect(screen.getByTestId("graph-sync-id").textContent).not.toBe(before),
    );
    expect(screen.getByTestId("graph-truncated-by")).toHaveTextContent("depth");
    expect(screen.getByTestId("graph-stale")).toHaveTextContent("false");
  });
});

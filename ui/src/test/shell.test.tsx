import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { DEVELOPER_MODE_KEY, GRAPH_CONSENT_KEY } from "@/lib/preferences";
import { ALICE, seedGraph } from "@/mocks/fixtures";

import { renderApp } from "./render";

async function openDeveloperMode(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByTestId("app-menu-button"));
  await user.click(screen.getByTestId("developer-mode-toggle"));
}

describe("navigation", () => {
  it("holds two entries and nothing else", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId("nav-wallet")).toHaveTextContent("Wallet");
    expect(screen.getByTestId("nav-witnesses")).toHaveTextContent("Witnesses");
    expect(screen.getAllByRole("link", { name: /^(Wallet|Witnesses)$/ })).toHaveLength(2);
    expect(screen.queryByTestId("nav-lookup")).not.toBeInTheDocument();
    expect(screen.queryByTestId("nav-verify")).not.toBeInTheDocument();
  });

  it("walks from the wallet to the witnesses and back", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    await user.click(screen.getByTestId("nav-witnesses"));
    await screen.findByTestId("witness-cards");

    await user.click(screen.getByTestId("nav-wallet"));
    await screen.findByTestId("identity-cards");
  });

  it("answers 'no such route' for the removed verify screen", async () => {
    renderApp("/wallet/verify");

    expect(await screen.findByTestId("route-not-found")).toBeInTheDocument();
  });
});

describe("developer mode", () => {
  it("is off by default and holds the head event and the raw document", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-head-seq")).toHaveTextContent("8");
    expect(screen.queryByTestId("identity-detail-head-event")).not.toBeInTheDocument();
    expect(screen.queryByTestId(`principal-key-${ALICE}`)).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-detail-raw")).not.toBeInTheDocument();
    expect(screen.queryByTestId("verification-detail")).not.toBeInTheDocument();
  });

  it("reveals them from the header menu and remembers the toggle", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    await openDeveloperMode(user);

    expect(globalThis.localStorage.getItem(DEVELOPER_MODE_KEY)).toBe("1");
    expect(screen.getByTestId("identity-detail-head-event")).toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-created-at-ms")).toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-raw")).toHaveTextContent(ALICE);
    expect(screen.getByTestId("verification-detail")).toHaveTextContent("_mabel.alice.example.");
    expect(await screen.findByTestId("ledger-id")).toBeInTheDocument();

    await user.click(screen.getByTestId("developer-mode-toggle"));

    expect(globalThis.localStorage.getItem(DEVELOPER_MODE_KEY)).toBe("0");
    expect(screen.queryByTestId("identity-detail-head-event")).not.toBeInTheDocument();
  });

  it("keeps every panel reachable while it is off", async () => {
    renderApp(`/identities/${ALICE}`);
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

    await waitFor(() => expect(globalThis.localStorage.getItem(GRAPH_CONSENT_KEY)).toBe("1"));
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

    await waitFor(() => expect(screen.getByTestId("graph-sync-id").textContent).not.toBe(before));
    expect(screen.getByTestId("graph-truncated-by")).toHaveTextContent("depth");
    expect(screen.getByTestId("graph-stale")).toHaveTextContent("false");
  });
});

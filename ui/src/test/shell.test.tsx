import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { GRAPH_CONSENT_KEY } from "@/lib/preferences";
import { ALICE } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

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

  it("answers 'no such page' for the removed verify screen", async () => {
    renderApp("/wallet/verify");

    expect(await screen.findByTestId("route-not-found")).toBeInTheDocument();
  });
});

describe("the header", () => {
  // Decision 017: no developer mode, and no counter the header cannot explain.
  it("carries the app name and the nav, and no menu or counter", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId("app-title")).toHaveTextContent("mabel");
    expect(screen.queryByTestId("app-menu-button")).not.toBeInTheDocument();
    expect(screen.queryByTestId("developer-mode-toggle")).not.toBeInTheDocument();
    expect(screen.queryByTestId("graph-sync-counts")).not.toBeInTheDocument();
    expect(screen.queryByTestId("graph-sync-button")).not.toBeInTheDocument();
    expect(globalThis.localStorage.getItem("mabel.developer_mode")).toBeNull();
  });

  it("keeps every panel on the page and prints no raw document", async () => {
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
      "identity-keys",
    ]) {
      expect(screen.getByTestId(panel)).toBeInTheDocument();
    }
    // The diagnostic surfaces are the CLI and the HTTP API, not a hidden panel.
    for (const gone of [
      "identity-detail-raw",
      "identity-detail-head-event",
      "identity-detail-created-at-ms",
      "identity-detail-active-key",
      "identity-detail-reserve-commit",
      `principal-key-${ALICE}`,
      "verification-stale",
      "verification-unreachable",
      "profile-event",
      "profile-signing-principal",
      "ledger-id",
      "ledger-head-event",
      "graph-sync-provenance",
    ]) {
      expect(screen.queryByTestId(gone)).not.toBeInTheDocument();
    }
  });
});

describe("finding people through the people you trust", () => {
  it("lives on the witnesses page, not the header", async () => {
    renderApp("/witnesses");

    const card = await screen.findByTestId("graph-sync");
    expect(card).toHaveTextContent("It only looks when you press the button.");
    expect(screen.getByTestId("graph-sync-state")).toHaveTextContent("Your wallet last looked");
    expect(screen.getByTestId("graph-sync-truncated")).toHaveTextContent(
      "Your wallet may not have seen everything.",
    );
    expect(screen.getByTestId("graph-sync-button")).toBeInTheDocument();
  });

  it("asks for consent once, then synchronizes on the next click", async () => {
    const { user } = renderApp("/witnesses");
    await screen.findByTestId("graph-sync-button");

    await user.click(screen.getByTestId("graph-sync-button"));

    const consent = screen.getByTestId("graph-sync-consent");
    expect(consent).toHaveTextContent("learns which people you are interested in");
    expect(globalThis.localStorage.getItem(GRAPH_CONSENT_KEY)).toBeNull();

    await user.click(screen.getByTestId("graph-sync-consent-confirm"));

    await waitFor(() => expect(globalThis.localStorage.getItem(GRAPH_CONSENT_KEY)).toBe("1"));
    expect(screen.queryByTestId("graph-sync-consent")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("graph-sync-button"));

    expect(screen.queryByTestId("graph-sync-consent")).not.toBeInTheDocument();
  });

  it("drops the panel when the consent is declined, and syncs nothing", async () => {
    const posted: string[] = [];
    server.events.on("request:start", ({ request }) => {
      if (request.method === "POST") {
        posted.push(new URL(request.url).pathname);
      }
    });
    const { user } = renderApp("/witnesses");
    await screen.findByTestId("graph-sync-button");

    await user.click(screen.getByTestId("graph-sync-button"));
    await user.click(screen.getByTestId("graph-sync-consent-cancel"));

    expect(screen.queryByTestId("graph-sync-consent")).not.toBeInTheDocument();
    expect(globalThis.localStorage.getItem(GRAPH_CONSENT_KEY)).toBeNull();
    expect(posted).toEqual([]);
  });
});

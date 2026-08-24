import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { GRAPH_CONSENT_KEY } from "@/lib/preferences";
import { ALICE } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

describe("navigation", () => {
  it("holds three entries and nothing else", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId("nav-wallet")).toHaveTextContent("Wallet");
    expect(screen.getByTestId("nav-witnesses")).toHaveTextContent("Witnesses");
    expect(screen.getByTestId("nav-node")).toHaveTextContent("Node");
    expect(screen.getAllByRole("link", { name: /^(Wallet|Witnesses|Node)$/ })).toHaveLength(3);
    expect(screen.queryByTestId("nav-lookup")).not.toBeInTheDocument();
    expect(screen.queryByTestId("nav-verify")).not.toBeInTheDocument();
  });

  it("is one navigation menu whose entries are links", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const menu = screen.getByRole("navigation");
    expect(menu).toHaveAttribute("data-slot", "navigation-menu");
    expect(screen.getByTestId("nav-wallet")).toHaveAttribute(
      "data-slot",
      "navigation-menu-link",
    );
    // The entry for the screen you are on says so, and it is the only one.
    expect(screen.getByTestId("nav-wallet")).toHaveAttribute("aria-current", "page");
    expect(screen.getByTestId("nav-node")).not.toHaveAttribute("aria-current");
  });

  it("marks the entry you are on with a background, and underlines nothing", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    // The shadcn active state: a muted background on the current entry, the
    // same height and typography on all three.
    const current = screen.getByTestId("nav-wallet");
    expect(current.className).toMatch(/aria-\[current=page\]:bg-accent/);
    expect(current.className).not.toMatch(/underline/);
    // The router adds its own "active" class to the current entry; every other
    // class is shared, so no entry carries a size or a weight of its own.
    const shared = (testId: string) =>
      screen
        .getByTestId(testId)
        .className.split(" ")
        .filter((name) => name !== "active")
        .join(" ");
    for (const testId of ["nav-witnesses", "nav-node"]) {
      expect(shared(testId)).toBe(shared("nav-wallet"));
    }
  });

  it("walks the entries with the arrow keys", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    screen.getByTestId("nav-wallet").focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByTestId("nav-witnesses")).toHaveFocus();

    await user.keyboard("{End}");
    expect(screen.getByTestId("nav-node")).toHaveFocus();

    await user.keyboard("{Home}");
    expect(screen.getByTestId("nav-wallet")).toHaveFocus();
  });

  it("walks from the wallet to the witnesses and the node and back", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    await user.click(screen.getByTestId("nav-witnesses"));
    await screen.findByTestId("witness-cards");

    await user.click(screen.getByTestId("nav-node"));
    await screen.findByTestId("node-page");

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
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    // The state of the identity is on the page; every action holds its panel
    // once it is opened.
    for (const panel of ["trust-panel", "ledger-panel", "identity-actions"]) {
      expect(screen.getByTestId(panel)).toBeInTheDocument();
    }
    for (const [action, panel] of [
      ["action-profile", "profile-panel"],
      ["action-handle", "handle-panel"],
      ["action-handle", "verification-panel"],
      ["action-contact", "contact-panel"],
      ["action-witnesses", "witness-config"],
      ["action-push", "sync-push"],
      ["action-keys", "identity-keys"],
    ]) {
      await openAction(user, action);
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
    expect(card).toHaveTextContent("only when you press the button");
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

import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE } from "@/mocks/fixtures";
import { server } from "@/mocks/server";
import { MISMATCHED_HOSTNAME, UNREACHABLE_HOSTNAME } from "@/mocks/store";

import { renderApp } from "./render";

/** Every hostname the screen sent to GET /api/resolve, in order. */
function resolveCalls(): string[] {
  const asked: string[] = [];
  server.events.on("request:start", ({ request }) => {
    const { pathname } = new URL(request.url);
    if (pathname.startsWith("/api/resolve/")) {
      asked.push(decodeURIComponent(pathname.slice("/api/resolve/".length)));
    }
  });
  return asked;
}

describe("the wallet page's shape", () => {
  it("is three flat sections under their own headings, and no card holds a card", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const headings = screen.getAllByRole("heading", { level: 2 }).map((node) => node.textContent);
    expect(headings).toEqual(["Open an identity", "Your identities", "Known identities"]);
    // No section is a card: the cards on this page are the identities.
    for (const section of ["wallet-search", "identity-list", "known-identities"]) {
      expect(screen.getByTestId(section).className).not.toMatch(/bg-card/);
    }
    // And no card holds another card.
    for (const card of screen.getAllByTestId(/^identity-card-[a-z2-7]{52}$/)) {
      for (const inner of card.querySelectorAll("div")) {
        expect(inner.className).not.toMatch(/bg-card/);
      }
    }
  });

  it("puts the create control in the section about your own identities", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const section = screen.getByTestId("identity-list");
    expect(within(section).getByTestId("identity-cards")).toBeInTheDocument();
    expect(within(section).getByTestId("identity-create-summary")).toBeInTheDocument();
    expect(within(section).queryByTestId("known-identity-cards")).not.toBeInTheDocument();
  });

  it("offers the demo reset in the footer, under a plain label", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const reset = screen.getByTestId("demo-reset");
    expect(reset).toHaveTextContent("Reset demo data");
    expect(reset.closest("footer")).not.toBeNull();
  });
});

describe("the identity card list", () => {
  it("draws one card per local identity, with the name, id and kind", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const card = screen.getByTestId(`identity-card-${ALICE}`);
    const name = within(card).getByTestId(`identity-card-name-${ALICE}`);
    expect(within(name).getByTestId(`identity-card-name-${ALICE}-name`)).toHaveTextContent(
      "Alice Ashworth",
    );
    expect(name).toHaveAttribute("data-identity-id", ALICE);
    expect(
      within(name).getByTestId(`identity-card-name-${ALICE}-verification`),
    ).toHaveAttribute("data-verification", "verified");
    expect(within(card).getByTestId(`identity-card-declared-kind-${ALICE}`)).toHaveTextContent(
      "person",
    );
    // No card names a position on the record: nobody reads a card for that.
    expect(card).not.toHaveTextContent("at position");
  });

  it("links the card at the identity page, on the id its heading draws", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const link = screen.getByTestId(`identity-card-link-${ACME}`);
    expect(link).toHaveAttribute("href", `/identities/${ACME}`);

    await user.click(link);

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-resolved")).toHaveTextContent(ACME);
  });

  it("offers no selection: no radio, no remembered identity", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.queryByRole("radio")).toBeNull();
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByTestId("identity-selector")).not.toBeInTheDocument();
    expect(globalThis.localStorage.getItem("mabel.selected_identity")).toBeNull();
  });

  it("folds the create form away behind one button", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId("identity-create")).not.toHaveAttribute("open");

    await user.click(screen.getByTestId("identity-create-summary"));

    expect(screen.getByTestId("identity-create-alias")).toBeVisible();
  });
});

describe("the wallet search box", () => {
  it("opens the identity page for an identity id without asking DNS", async () => {
    const resolved = resolveCalls();
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), ALICE);
    await user.click(screen.getByTestId("wallet-search-submit"));

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-resolved")).toHaveTextContent(ALICE);
    expect(resolved).toEqual([]);
  });

  it("resolves a hostname through the node and opens what it named", async () => {
    const resolved = resolveCalls();
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), "alice.example");
    await user.click(screen.getByTestId("wallet-search-submit"));

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-resolved")).toHaveTextContent(ALICE);
    expect(resolved).toEqual(["alice.example"]);
  });

  it.each([
    ["nobody.example", "no_record", "names no identity"],
    [MISMATCHED_HOSTNAME, "mismatched_records", "nothing it said is a Mabel ID"],
    [UNREACHABLE_HOSTNAME, "unreachable", "gave no answer"],
  ])("says what the TXT lookup answered for %s", async (hostname, status, sentence) => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), hostname);
    await user.click(screen.getByTestId("wallet-search-submit"));

    const answer = await screen.findByTestId("wallet-search-status");
    expect(answer).toHaveAttribute("data-status", status);
    expect(answer).toHaveTextContent(`_mabel.${hostname}.`);
    expect(answer).toHaveTextContent(sentence);
    // Nothing navigated: the wallet is still the wallet.
    expect(screen.getByTestId("identity-cards")).toBeInTheDocument();
  });

  it("renders the envelope for a string that is neither an id nor a hostname", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), "alice_example");
    await user.click(screen.getByTestId("wallet-search-submit"));

    const envelope = await screen.findByTestId("wallet-search-error");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("malformed_hostname");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 10");
  });

  it("drops the last answer when the box is submitted again", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), "nobody.example");
    await user.click(screen.getByTestId("wallet-search-submit"));
    await screen.findByTestId("wallet-search-status");

    await user.clear(screen.getByTestId("wallet-search-input"));
    await user.type(screen.getByTestId("wallet-search-input"), UNREACHABLE_HOSTNAME);
    await user.click(screen.getByTestId("wallet-search-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("wallet-search-status")).toHaveAttribute(
        "data-status",
        "unreachable",
      ),
    );
  });
});

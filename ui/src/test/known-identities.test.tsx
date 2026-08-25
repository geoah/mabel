import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { listKnownIdentities } from "@/api/client";
import { ACME, ALICE, BOB, CAROL, knownBob } from "@/mocks/fixtures";
import { isTrusted } from "@/routes/wallet/KnownIdentities";

import { renderApp } from "./render";

/**
 * The second list on the wallet page: every identity this home has a record of
 * and does not control, as the same card the first list draws.
 */

describe("GET /api/identities/known", () => {
  it("answers the identities this home knows of and does not control", async () => {
    const response = await listKnownIdentities();
    const ids = response.identities.map((row) => row.identity_id);

    // Ascending identity id alone, and never an identity this home controls.
    expect(ids).toEqual([...ids].sort());
    expect(ids).not.toContain(ALICE);
    expect(ids).not.toContain(ACME);
    expect(ids).toContain(BOB);
    expect(ids).toContain(CAROL);
  });

  it("says of Bob what the frozen row says: stored, trusted, one step away", async () => {
    const response = await listKnownIdentities();
    const bob = response.identities.find((row) => row.identity_id === BOB);

    expect(bob).toMatchObject({
      display_name: knownBob.display_name,
      email: knownBob.email,
      hostname: knownBob.hostname,
      declared_kind: "person",
      stored: true,
      trusted: true,
      degrees: 1,
    });
    expect(bob?.head_seq).not.toBeNull();
  });

  it("says of Carol that no copy is stored and the crawl reaches her in two", async () => {
    const response = await listKnownIdentities();
    const carol = response.identities.find((row) => row.identity_id === CAROL);

    expect(carol).toMatchObject({
      display_name: null,
      declared_kind: null,
      stored: false,
      trusted: false,
      degrees: 2,
      head_seq: null,
    });
  });
});

describe("the trusted-only filter", () => {
  it.each([
    ["an attestation of your own", { trusted: true, degrees: null }, true],
    ["a path through other people", { trusted: false, degrees: 2 }, true],
    ["nothing the crawl reached", { trusted: false, degrees: null }, false],
  ])("counts %s", (_name, row, expected) => {
    expect(isTrusted({ trusted: row.trusted, degrees: row.degrees } as never)).toBe(expected);
  });
});

describe("the known identities section", () => {
  it("draws one card per known identity, with the pill the row implies", async () => {
    renderApp("/wallet");
    await screen.findByTestId("known-identity-cards");

    const bob = screen.getByTestId(`identity-card-${BOB}`);
    expect(within(bob).getByTestId(`identity-card-name-${BOB}-name`)).toHaveTextContent(
      "Bob Baxter",
    );
    expect(within(bob).getByTestId(`identity-card-name-${BOB}-pill`)).toHaveTextContent("trusted");
    expect(within(bob).getByTestId(`identity-card-declared-kind-${BOB}`)).toHaveTextContent(
      "person",
    );
    // A copy is stored, so nothing on this card says otherwise.
    expect(within(bob).queryByTestId(`identity-card-unheld-${BOB}`)).not.toBeInTheDocument();

    const carol = screen.getByTestId(`identity-card-${CAROL}`);
    expect(within(carol).getByTestId(`identity-card-name-${CAROL}-pill`)).toHaveTextContent(
      "trusted (2d)",
    );
    expect(within(carol).getByTestId(`identity-card-unheld-${CAROL}`)).toHaveTextContent(
      "not stored here",
    );
    // Nothing is known beyond the collapsed lines, so there is nothing to open.
    expect(within(carol).queryByTestId(`identity-card-expand-${CAROL}`)).not.toBeInTheDocument();
  });

  it("opens a stored known record onto its email and how much of it is here", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("known-identity-cards");

    await user.click(screen.getByTestId(`identity-card-expand-${BOB}`));

    const bob = screen.getByTestId(`identity-card-${BOB}`);
    expect(within(bob).getByTestId(`identity-card-email-${BOB}`)).toHaveTextContent(
      "bob@bob.example",
    );
    expect(within(bob).getByTestId(`identity-card-ledger-summary-${BOB}`)).toHaveTextContent(
      "entries",
    );
    expect(within(bob).getByTestId(`identity-card-ledger-summary-${BOB}`)).not.toHaveTextContent(
      "no copy",
    );
  });

  it("narrows the list to the trusted ones and back", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("known-identity-cards");

    const all = screen.getByTestId("known-identities-all");
    const trusted = screen.getByTestId("known-identities-trusted");
    expect(all).toHaveAttribute("aria-selected", "true");
    expect(trusted).toHaveAttribute("aria-selected", "false");

    await user.click(trusted);

    expect(trusted).toHaveAttribute("aria-selected", "true");
    expect(all).toHaveAttribute("aria-selected", "false");
    // Both seeded rows are trusted, one explicitly and one through the graph.
    expect(screen.getByTestId(`identity-card-${BOB}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-${CAROL}`)).toBeInTheDocument();

    await user.click(all);

    expect(all).toHaveAttribute("aria-selected", "true");
    expect(trusted).toHaveAttribute("aria-selected", "false");
  });

  it("reaches the trusted tab with the arrow keys alone", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("known-identity-cards");

    const all = screen.getByTestId("known-identities-all");
    const trusted = screen.getByTestId("known-identities-trusted");
    all.focus();
    await user.keyboard("{ArrowRight}");

    // Activation follows focus, so arrowing across the row shows the panel.
    expect(trusted).toHaveFocus();
    expect(trusted).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("known-identity-cards")).toBeInTheDocument();
  });

  it("links a known card at the identity's own page", async () => {
    renderApp("/wallet");
    await screen.findByTestId("known-identity-cards");

    expect(screen.getByTestId(`identity-card-link-${CAROL}`)).toHaveAttribute(
      "href",
      `/identities/${CAROL}`,
    );
  });
});

import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ALICE, BOB, REACHABLE_WITNESS } from "@/mocks/fixtures";

import { openAction, renderApp } from "./render";

/**
 * The controls whose own border is part of the control: a button, a box you type
 * in, a link you press. A border on one of these is never a box inside a box.
 */
const CONTROLS = new Set(["BUTTON", "INPUT", "TEXTAREA", "SELECT", "A", "LABEL"]);

/** Every element drawing a box on all four sides, controls and pills excluded. */
function boxes(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>("*")].filter((element) => {
    if (CONTROLS.has(element.tagName) || element.dataset.slot === "badge") {
      return false;
    }
    const classes = element.className;
    return typeof classes === "string" && /(^|\s)border($|\s)/.test(classes);
  });
}

/**
 * The one rule this app's layout has: never a border inside a border. A section
 * is a heading, a line under it and its content; only the leaf content draws a
 * box, so no box on any screen contains another.
 */
function expectNoNestedBorder(root: HTMLElement) {
  const found = boxes(root);
  for (const box of found) {
    const inside = found.filter((other) => other !== box && box.contains(other));
    expect(
      inside.map((element) => element.getAttribute("data-testid") ?? element.className),
    ).toEqual([]);
  }
}

describe("never a border inside a border", () => {
  it("holds on the wallet front page", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");
    await screen.findByTestId("known-identity-cards");

    expectNoNestedBorder(screen.getByTestId("identity-list").parentElement!);
  });

  it("holds on an identity this wallet signs for, with an action open", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");
    await screen.findByTestId("trust-list");
    // The one action that lists cards inside itself, which is where a nested
    // border used to be unavoidable.
    await openAction(user, "action-witnesses");

    expectNoNestedBorder(screen.getByTestId("identity-detail").parentElement!);
  });

  it("holds on a stored identity this wallet does not control", async () => {
    renderApp(`/identities/${BOB}`);
    await screen.findByTestId("ledger-events");

    expectNoNestedBorder(screen.getByTestId("identity-detail").parentElement!);
  });

  it("holds on the witnesses page and on one witness", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    expectNoNestedBorder(screen.getByTestId("witness-list").parentElement!);

    renderApp(`/witnesses/${REACHABLE_WITNESS}`);
    await screen.findAllByTestId("identity-cards");

    for (const holdings of screen.getAllByTestId("witness-ledgers")) {
      expectNoNestedBorder(holdings);
    }
  });

  it("holds on the node page", async () => {
    renderApp("/node");
    await screen.findByTestId("node-witness-cards");

    expectNoNestedBorder(screen.getByTestId("node-page").parentElement!);
  });
});

describe("the heading hierarchy", () => {
  it("names the identity in an h1 and every section under it in an h2", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    const name = screen.getByTestId("identity-detail-resolved-name");
    expect(name.tagName).toBe("H1");
    expect(name.className).toContain("text-2xl");
    // The sections under it, in the order the page reads.
    const sections = ["trust-panel", "ledger-panel"];
    for (const section of sections) {
      const heading = within(screen.getByTestId(section)).getAllByRole("heading")[0];
      expect(heading.tagName).toBe("H2");
      expect(heading.className).toContain("text-lg");
    }
  });

  it("names a card in an h3, under the section's h2", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const section = within(screen.getByTestId("identity-list")).getAllByRole("heading")[0];
    expect(section.tagName).toBe("H2");
    const card = screen.getByTestId(`identity-card-name-${ALICE}-name`);
    expect(card.tagName).toBe("H3");
    expect(card.className).toContain("text-base");
  });
});

describe("one way back", () => {
  it("leaves the nav as the way back from a witness and from a record", async () => {
    renderApp(`/witnesses/${REACHABLE_WITNESS}`);
    await screen.findByTestId("witness-ledgers");

    expect(screen.queryByTestId("witness-ledgers-back")).not.toBeInTheDocument();
    expect(screen.getByTestId("nav-witnesses")).toBeInTheDocument();
  });
});

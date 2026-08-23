import { screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import { Identifier, middleTruncate, splitIdentifier } from "@/components/Identifier";
import { ALICE } from "@/mocks/fixtures";

import { renderComponent } from "./render";

const PARTS = splitIdentifier(ALICE);

function host(element: ReactElement) {
  return renderComponent(
    <MemoryRouter>
      <div data-testid="host">{element}</div>
    </MemoryRouter>,
  );
}

function identifier(): HTMLElement {
  return within(screen.getByTestId("host")).getByTitle(ALICE);
}

describe("splitIdentifier", () => {
  it("keeps the first and last eight characters of a 52 character id", () => {
    expect(PARTS.head).toHaveLength(8);
    expect(PARTS.tail).toHaveLength(8);
    expect(PARTS.head + PARTS.middle + PARTS.tail).toBe(ALICE);
    expect(middleTruncate(ALICE)).toBe(`${ALICE.slice(0, 8)}…${ALICE.slice(-8)}`);
  });

  it("returns a short value whole, with nothing hidden", () => {
    expect(splitIdentifier("seq-4")).toEqual({ head: "seq-4", middle: "", tail: "" });
    expect(middleTruncate("seq-4")).toBe("seq-4");
  });
});

describe("Identifier", () => {
  it("shows head and tail, hides the middle and keeps the whole value readable", () => {
    host(<Identifier value={ALICE} />);

    const value = identifier();
    expect(value).toHaveTextContent(ALICE);
    expect(value).toHaveAttribute("title", ALICE);
    expect(screen.getByTestId("host").querySelector("[data-value]")).toHaveAttribute(
      "data-truncated",
      "true",
    );
    // The middle stays in the document for a screen reader and a copy of the
    // page, and out of the way of a reader on a phone.
    expect(within(value).getByText(PARTS.middle)).toHaveClass("sr-only");
    expect(within(value).getByText(PARTS.head)).not.toHaveClass("sr-only");
    expect(within(value).getByText(PARTS.tail)).toHaveClass("identifier-ellipsis");
  });

  it("toggles the whole value on click and back", async () => {
    const { user } = host(<Identifier value={ALICE} />);

    const toggle = screen.getByRole("button", { name: ALICE });
    expect(toggle).toHaveAttribute("aria-expanded", "false");

    await user.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("host").querySelector("[data-value]")).toHaveAttribute(
      "data-truncated",
      "false",
    );
    expect(screen.queryByText(PARTS.middle)).not.toBeInTheDocument();
    expect(toggle).toHaveTextContent(ALICE);

    await user.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText(PARTS.middle)).toHaveClass("sr-only");
  });

  it("copies the whole value and reports it until the button is left", async () => {
    const { user } = host(<Identifier value={ALICE} />);

    await user.click(screen.getByRole("button", { name: "copy" }));

    expect(await navigator.clipboard.readText()).toBe(ALICE);
    const copied = screen.getByRole("button", { name: "copied" });
    expect(copied).toHaveAttribute("data-copied", "true");

    await user.click(screen.getByRole("button", { name: ALICE }));

    expect(screen.getByRole("button", { name: "copy" })).toHaveAttribute("data-copied", "false");
  });

  it("wraps the whole value with no toggle in full mode, for a verify report", () => {
    host(<Identifier value={ALICE} full />);

    const value = identifier();
    expect(value).toHaveTextContent(ALICE);
    expect(value.tagName).toBe("SPAN");
    expect(screen.getByTestId("host").querySelector("[data-value]")).toHaveAttribute(
      "data-truncated",
      "false",
    );
    expect(screen.queryByRole("button", { name: ALICE })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "copy" })).toBeInTheDocument();
  });

  it("routes the value when a route is given, and keeps its testid", () => {
    host(<Identifier value={ALICE} to={`/witness/ledgers/${ALICE}`} linkTestId="ledger-link" />);

    const link = screen.getByTestId("ledger-link");
    expect(link).toHaveAttribute("href", `/witness/ledgers/${ALICE}`);
    expect(link).toHaveTextContent(ALICE);
    expect(screen.queryByRole("button", { name: ALICE })).not.toBeInTheDocument();
  });

  it("renders a missing value as the null the document carries", () => {
    host(<Identifier value={null} />);

    expect(screen.getByTestId("host")).toHaveTextContent("null");
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});

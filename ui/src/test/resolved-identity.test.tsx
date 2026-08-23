import { screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { ResolvedIdentity as ResolvedIdentityDocument, VerificationStatus } from "@/api/types";
import {
  ResolvedIdentity,
  ResolvedIdentityScope,
  resolveName,
} from "@/components/ResolvedIdentity";
import { ALICE, BOB } from "@/mocks/fixtures";

import { renderComponent } from "./render";

function document(
  overrides: Partial<ResolvedIdentityDocument> = {},
): ResolvedIdentityDocument {
  return {
    identity_id: ALICE,
    display_name: "Alice Ashworth",
    alias: null,
    hostname: null,
    verification_status: "unclaimed",
    provenance: "profile",
    ...overrides,
  };
}

function host(element: ReactElement) {
  return renderComponent(
    <MemoryRouter>
      <div data-testid="host">{element}</div>
    </MemoryRouter>,
  );
}

describe("name resolution", () => {
  it("tries the profile display name, then the alias, then the id", () => {
    expect(resolveName(document())).toEqual({ name: "Alice Ashworth", source: "profile" });
    expect(resolveName(document({ display_name: null, alias: "alice" }))).toEqual({
      name: "alice",
      source: "alias",
    });
    expect(resolveName(document({ display_name: null, alias: null }))).toEqual({
      name: null,
      source: "id",
    });
  });
});

describe("ResolvedIdentity", () => {
  it("shows a hostname-shaped display name as a name, with the id beside it", () => {
    // A display name of alice.example must never pass for a verified hostname:
    // it renders as plain text, and the monospace styling stays with the id.
    host(<ResolvedIdentity identity={document({ display_name: "alice.example" })} testId="who" />);

    const name = screen.getByTestId("who-name");
    expect(name).toHaveTextContent("alice.example");
    expect(name.className).not.toMatch(/font-mono/);
    expect(screen.getByTestId("who")).toHaveAttribute("data-name-source", "profile");

    const id = screen.getByTestId("who").querySelector("[data-value]");
    expect(id).toHaveAttribute("data-value", ALICE);
    expect(id?.className).toMatch(/font-mono/);
    expect(id).not.toBe(name);
  });

  it("renders the id alone when nothing names the identity", () => {
    host(<ResolvedIdentity identity={document({ display_name: null, alias: null })} testId="who" />);

    expect(screen.queryByTestId("who-name")).not.toBeInTheDocument();
    expect(screen.getByTestId("who")).toHaveAttribute("data-name-source", "id");
    expect(screen.getByTestId("who").querySelector("[data-value]")).toHaveAttribute(
      "data-truncated",
      "true",
    );
  });

  it("shows both full ids when two entries of one list resolve to the same name", () => {
    const entries = [
      document({ identity_id: ALICE, display_name: "Robin" }),
      document({ identity_id: BOB, display_name: "Robin" }),
    ];
    host(
      <ResolvedIdentityScope identities={entries}>
        <ResolvedIdentity identity={entries[0]} testId="first" />
        <ResolvedIdentity identity={entries[1]} testId="second" />
      </ResolvedIdentityScope>,
    );

    for (const testId of ["first", "second"]) {
      const entry = screen.getByTestId(testId);
      expect(entry).toHaveAttribute("data-shared-name", "true");
      expect(entry.querySelector("[data-value]")).toHaveAttribute("data-truncated", "false");
    }
    expect(screen.getByTestId("first")).toHaveTextContent(ALICE);
    expect(screen.getByTestId("second")).toHaveTextContent(BOB);
  });

  it("truncates the id of a name only one entry of the list holds", () => {
    const entries = [
      document({ identity_id: ALICE, display_name: "Robin" }),
      document({ identity_id: BOB, display_name: "Sam" }),
    ];
    host(
      <ResolvedIdentityScope identities={entries}>
        <ResolvedIdentity identity={entries[0]} testId="first" />
        <ResolvedIdentity identity={entries[1]} testId="second" />
      </ResolvedIdentityScope>,
    );

    expect(screen.getByTestId("first")).toHaveAttribute("data-shared-name", "false");
    expect(screen.getByTestId("first").querySelector("[data-value]")).toHaveAttribute(
      "data-truncated",
      "true",
    );
  });

  it.each([
    ["verified", false, "verified"],
    ["verified", true, "stale-verified"],
    ["mismatched", false, "mismatched"],
    ["unverified", false, "unverified"],
    ["unreachable", false, "unreachable"],
  ])("renders %s (stale %s) as the %s mark, never without the hostname", (status, stale, state) => {
    host(
      <ResolvedIdentity
        identity={document({
          hostname: "alice.example",
          verification_status: status as VerificationStatus,
        })}
        stale={stale as boolean}
        testId="who"
      />,
    );

    const mark = screen.getByTestId("who-verification");
    expect(mark).toHaveAttribute("data-verification", state);
    // The glyph never travels alone: the hostname it is about is beside it, in
    // the monospace style, and the tooltip repeats the advisory sentence.
    expect(within(mark).getByText("alice.example").className).toMatch(/font-mono/);
    expect(mark.getAttribute("title")).toContain("advisory");
    expect(mark).toHaveTextContent(state);
  });

  it("renders no mark at all for an identity claiming no hostname", () => {
    host(<ResolvedIdentity identity={document({ verification_status: "unclaimed" })} testId="who" />);

    expect(screen.queryByTestId("who-verification")).not.toBeInTheDocument();
  });
});

import { screen, within } from "@testing-library/react";
import type { ReactElement, ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { ResolvedIdentity as ResolvedIdentityDocument, VerificationStatus } from "@/api/types";
import {
  IdentityInline,
  IdentityListScope,
  IdentityPillScope,
  type PillFacts,
  pillFor,
  resolveName,
  trustedSubjects,
} from "@/components/identity";
import { ACME, ALICE, BOB, CAROL, seedIdentities } from "@/mocks/fixtures";

import { renderComponent } from "./render";

function document(overrides: Partial<ResolvedIdentityDocument> = {}): ResolvedIdentityDocument {
  return {
    identity_id: ALICE,
    display_name: "Alice Ashworth",
    email: null,
    alias: null,
    hostname: null,
    verification_status: "unclaimed",
    provenance: "profile",
    ...overrides,
  };
}

function facts(overrides: Partial<PillFacts> = {}): PillFacts {
  return {
    own: new Set<string>(),
    trusted: new Set<string>(),
    degrees: new Map<string, number>(),
    ...overrides,
  };
}

function host(element: ReactElement, pills: PillFacts = facts()) {
  return renderComponent(
    <MemoryRouter>
      <IdentityPillScope facts={pills}>
        <div data-testid="host">{element}</div>
      </IdentityPillScope>
    </MemoryRouter>,
  );
}

describe("name resolution", () => {
  it("tries the profile display name, then the alias, then the id", () => {
    expect(resolveName(document())).toEqual({
      name: "Alice Ashworth",
      source: "profile",
      nickname: null,
    });
    expect(resolveName(document({ display_name: null, alias: "alice" }))).toEqual({
      name: "alice",
      source: "alias",
      nickname: null,
    });
    expect(resolveName(document({ display_name: null, alias: null }))).toEqual({
      name: null,
      source: "id",
      nickname: null,
    });
  });
});

describe("IdentityInline", () => {
  it("shows a hostname-shaped display name as a name, with the id beside it", () => {
    // A display name of alice.example must never pass for a verified hostname:
    // it renders as plain text, and the monospace styling stays with the id.
    host(<IdentityInline identity={document({ display_name: "alice.example" })} testId="who" />);

    const name = screen.getByTestId("who-name");
    expect(name).toHaveTextContent("alice.example");
    expect(name.className).not.toMatch(/font-mono/);
    expect(screen.getByTestId("who")).toHaveAttribute("data-name-source", "profile");

    const id = screen.getByTestId("who").querySelector("[data-value]");
    expect(id).toHaveAttribute("data-value", ALICE);
    expect(id?.className).toMatch(/font-mono/);
    expect(id).not.toBe(name);
  });

  it("offers a copy button for the id on every line it draws", () => {
    host(<IdentityInline identity={document()} testId="who" />);

    expect(within(screen.getByTestId("who")).getByLabelText("copy")).toBeInTheDocument();
  });

  it("renders the id alone when nothing names the identity", () => {
    host(<IdentityInline identity={document({ display_name: null, alias: null })} testId="who" />);

    expect(screen.queryByTestId("who-name")).not.toBeInTheDocument();
    expect(screen.getByTestId("who")).toHaveAttribute("data-name-source", "id");
    expect(screen.getByTestId("who").querySelector("[data-value]")).toHaveAttribute(
      "data-truncated",
      "true",
    );
  });

  it("routes the id, under the testid the caller names", () => {
    host(
      <IdentityInline
        identity={document()}
        testId="who"
        to={`/identities/${ALICE}`}
        linkTestId="named-link"
      />,
    );

    expect(screen.getByTestId("named-link")).toHaveAttribute("href", `/identities/${ALICE}`);
    expect(screen.queryByTestId("who-link")).not.toBeInTheDocument();
  });

  it("shows both full ids when two entries of one list resolve to the same name", () => {
    const entries = [
      document({ identity_id: ALICE, display_name: "Robin" }),
      document({ identity_id: BOB, display_name: "Robin" }),
    ];
    host(
      <IdentityListScope identities={entries}>
        <IdentityInline identity={entries[0]} testId="first" />
        <IdentityInline identity={entries[1]} testId="second" />
      </IdentityListScope>,
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
      <IdentityListScope identities={entries}>
        <IdentityInline identity={entries[0]} testId="first" />
        <IdentityInline identity={entries[1]} testId="second" />
      </IdentityListScope>,
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
      <IdentityInline
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
    // the monospace style, and the tooltip says what the verdict means.
    expect(within(mark).getByText("alice.example").className).toMatch(/font-mono/);
    expect(mark).toHaveTextContent(state);
    // Proposal 005 removed the standing DNS advisory sentence outright.
    expect(mark.getAttribute("title")).not.toContain("It grants nothing.");
  });

  it("renders no mark at all for an identity claiming no hostname", () => {
    host(<IdentityInline identity={document({ verification_status: "unclaimed" })} testId="who" />);

    expect(screen.queryByTestId("who-verification")).not.toBeInTheDocument();
  });
});

describe("the pill", () => {
  it("picks own over trusted over a distance, and nothing when nothing is known", () => {
    const all = facts({
      own: new Set([ALICE]),
      trusted: new Set([ALICE, BOB]),
      degrees: new Map([
        [ALICE, 4],
        [BOB, 4],
        [CAROL, 2],
      ]),
    });

    expect(pillFor(ALICE, all)).toEqual({ kind: "own", label: "your identity", degrees: null });
    expect(pillFor(BOB, all)).toEqual({ kind: "trusted", label: "trusted", degrees: null });
    expect(pillFor(CAROL, all)).toEqual({ kind: "degree", label: "trusted (2d)", degrees: 2 });
    expect(pillFor(ACME, all)).toBeNull();
  });

  it("draws no distance pill for one step, which the trust lists already answer", () => {
    expect(pillFor(CAROL, facts({ degrees: new Map([[CAROL, 1]]) }))).toBeNull();
  });

  it("reads the unrevoked attestations of every identity this home holds", () => {
    const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;
    const unrevoked = alice.trust.find((record) => !record.revoked)!;
    // Alice trusted Bob, took it back, then said it again: the live record is
    // what counts, so the pill is green and the taken-back one changes nothing.
    const takenBackOnly = { ...alice, trust: alice.trust.filter((record) => record.revoked) };

    expect(trustedSubjects(seedIdentities).has(unrevoked.subject)).toBe(true);
    expect(trustedSubjects([takenBackOnly]).size).toBe(0);
  });

  const CASES: [string, string, Partial<PillFacts>][] = [
    ["own", "your identity", { own: new Set([ALICE]) }],
    ["trusted", "trusted", { trusted: new Set([ALICE]) }],
    ["degree", "trusted (3d)", { degrees: new Map([[ALICE, 3]]) }],
  ];

  it.each(CASES)("draws the %s pill beside the name", (kind, label, overrides) => {
    host(<IdentityInline identity={document()} testId="who" />, facts(overrides));

    const pill = screen.getByTestId("who-pill");
    expect(pill).toHaveAttribute("data-pill", kind);
    expect(pill).toHaveTextContent(label);
  });

  it("draws no pill when the screen knows nothing about the identity", () => {
    host(<IdentityInline identity={document()} testId="who" />);

    expect(screen.queryByTestId("who-pill")).not.toBeInTheDocument();
  });

  it("lets a caller override the pill its screen would give", () => {
    host(
      <IdentityInline identity={document()} testId="who" pill={null} />,
      facts({ own: new Set([ALICE]) }),
    );

    expect(screen.queryByTestId("who-pill")).not.toBeInTheDocument();
  });

  it("reaches an identity drawn any depth inside the scope", () => {
    function Deep({ children }: { children: ReactNode }) {
      return (
        <div>
          <div>{children}</div>
        </div>
      );
    }
    host(
      <Deep>
        <IdentityInline identity={document()} testId="who" />
      </Deep>,
      facts({ trusted: new Set([ALICE]) }),
    );

    expect(screen.getByTestId("who-pill")).toHaveTextContent("trusted");
  });
});

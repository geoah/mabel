import { screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { Identity, VerificationStatus } from "@/api/types";
import { factsFromIdentity, IdentityCard, pageTestIds } from "@/components/identity";
import { ACME, ALICE, seedIdentities } from "@/mocks/fixtures";

import { renderApp, renderComponent } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

function withVerification(status: VerificationStatus, stale = false): Identity {
  return {
    ...alice,
    verification: {
      ...alice.verification,
      hostname: status === "unclaimed" ? null : "alice.example",
      status,
      stale,
    },
  };
}

function overview(identity: Identity) {
  return renderComponent(
    <MemoryRouter>
      <IdentityCard facts={factsFromIdentity(identity)} state="page" testIds={pageTestIds} />
    </MemoryRouter>,
  );
}

describe("the identity page's top section", () => {
  it.each([
    ["verified", false, "verified"],
    ["verified", true, "stale-verified"],
    ["mismatched", false, "mismatched"],
    ["unverified", false, "unverified"],
    ["unreachable", false, "unreachable"],
  ])("marks the hostname row for %s (stale %s)", (status, stale, state) => {
    overview(withVerification(status as VerificationStatus, stale as boolean));

    const mark = screen.getByTestId("identity-detail-hostname-verification");
    expect(mark).toHaveAttribute("data-verification", state);
    expect(mark).toHaveTextContent("alice.example");
  });

  it("renders no marker at all for an identity claiming no hostname", () => {
    overview(withVerification("unclaimed"));

    expect(screen.queryByTestId("identity-detail-hostname-verification")).not.toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-hostname")).toHaveTextContent("none");
  });

  it("holds the address book fields on one line each, with the counts", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    // Every row is a key and a value on one line: the label column and the
    // value column are siblings, never stacked (decision 014).
    const row = screen.getByTestId("identity-detail-created-row");
    expect(within(row).getByText("created").tagName).toBe("DT");
    expect(screen.getByTestId("identity-detail-created")).toHaveTextContent("2023-11-14");
    expect(screen.getByTestId("identity-detail-declared-kind")).toHaveTextContent("person");
    expect(screen.getByTestId("identity-detail-event-count")).toHaveTextContent("9");
    expect(screen.getByTestId("identity-detail-trusted-count")).toHaveTextContent("1 identity");
    expect(screen.getByTestId("identity-detail-open-invitations")).toHaveTextContent("none");
  });

  it("carries the your-identity pill at the end of the name's own line", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    const pill = await screen.findByTestId("identity-detail-resolved-pill");
    expect(pill).toHaveTextContent("your identity");
    expect(pill).toHaveAttribute("data-pill", "own");
    // The name is this page's h1, and the pill sits at the end of its line.
    const name = screen.getByTestId("identity-detail-resolved-name");
    expect(name.tagName).toBe("H1");
    expect(pill.parentElement?.previousElementSibling).toBe(name.parentElement);
  });

  it("puts the kind beside the name, on the same line", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    const kind = screen.getByTestId("identity-detail-declared-kind");
    const name = screen.getByTestId("identity-detail-resolved-name");
    expect(kind).toHaveTextContent("person");
    // The kind leads the pill row at the end of the name's line, which is one
    // row up from the name itself.
    expect(kind.parentElement?.firstElementChild).toBe(kind);
    expect(kind.parentElement?.parentElement).toBe(name.parentElement?.parentElement);
    // There is no small line above the name any more.
    expect(screen.queryByTestId("identity-detail-kind-line")).not.toBeInTheDocument();
  });

  it("drops the back link: the browser has one, and the nav has two entries", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.queryByTestId("identity-back")).not.toBeInTheDocument();
  });
});

describe("ledger lines", () => {
  it("shows one line per event as its position and what it did, with no payload until it is opened", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("ledger-events");

    expect(screen.getByTestId("event-seq-0")).toHaveTextContent("0");
    expect(screen.getByTestId("event-gloss-0")).toHaveTextContent("created this identity");
    // A closed line is a position and a plain sentence: the kind string and the
    // payload are inside the entry, which is where a reader who wants them goes.
    expect(screen.queryByTestId("event-payload-kind-0")).not.toBeInTheDocument();
    expect(screen.queryByTestId("event-payload-0")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("event-expand-0"));

    expect(screen.getByTestId("event-detail-0")).toBeInTheDocument();
    expect(screen.getByTestId("event-payload-kind-0")).toHaveTextContent("inception");
    expect(screen.getByTestId("event-payload-0")).toHaveTextContent('"nonce"');
    expect(screen.getByTestId("event-id-0")).toBeInTheDocument();

    await user.click(screen.getByTestId("event-expand-0"));

    expect(screen.queryByTestId("event-detail-0")).not.toBeInTheDocument();
  });

  it("draws the rows as a list, not a table", async () => {
    renderApp(`/identities/${ALICE}`);

    const rows = await screen.findByTestId("ledger-events");
    expect(rows.tagName).toBe("UL");
    expect(rows.querySelector("table")).toBeNull();
  });

  it("opens one event without opening the others", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("ledger-events");

    await user.click(screen.getByTestId("event-expand-2"));

    expect(screen.getByTestId("event-detail-2")).toBeInTheDocument();
    expect(screen.queryByTestId("event-detail-1")).not.toBeInTheDocument();
  });
});

describe("state and actions", () => {
  it("draws who this identity trusts as cards, and trust it took back not at all", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("trust-panel");

    const [, unrevoked] = alice.trust;
    const list = await screen.findByTestId("trust-list");
    // Alice's fixture holds two entries for one subject, one taken back and one
    // standing: the standing one is a card, and it is the only card.
    expect(within(list).getByTestId(`identity-card-${unrevoked.subject}`)).toBeInTheDocument();
    expect(within(list).getAllByTestId(/^identity-card-[a-z2-7]{52}$/)).toHaveLength(1);
    // The folded list of taken-back trust is gone, and so is every row of it.
    expect(screen.queryByTestId("trust-revoked")).not.toBeInTheDocument();
    for (const record of alice.trust) {
      expect(screen.queryByTestId(`trust-row-${record.attestation_event}`)).not.toBeInTheDocument();
      expect(
        screen.queryByTestId(`trust-revoke-${record.attestation_event}`),
      ).not.toBeInTheDocument();
    }
  });

  it("links each card at the identity page of its subject", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("trust-list");

    const unrevoked = alice.trust[1];
    const card = screen.getByTestId(`identity-card-${unrevoked.subject}`);
    expect(
      within(card).getByTestId(`identity-card-link-${unrevoked.subject}`),
    ).toHaveAttribute("href", `/identities/${unrevoked.subject}`);
  });

  it("puts who they trust above the record", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("trust-panel");

    const trust = screen.getByTestId("trust-panel");
    const ledger = screen.getByTestId("ledger-panel");
    expect(trust.compareDocumentPosition(ledger)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
  });

  it("says in one line what the trust list is, and what an empty one means", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("trust-panel");

    expect(screen.getByTestId("trust-panel-description")).toHaveTextContent(
      "People this identity currently trusts.",
    );

    renderApp(`/identities/${ACME}`);

    expect(await screen.findByTestId("trust-list-empty")).toHaveTextContent(
      "This identity does not trust anyone yet.",
    );
  });

  it("groups the actions under four plain headings", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");

    const groups = [
      ["action-group-profile", "Profile", ["action-profile", "action-handle", "action-contact"]],
      ["action-group-trust", "Trust", ["action-trust", "action-revoke"]],
      ["action-group-witnesses", "Witnesses and sync", ["action-witnesses", "action-push"]],
      [
        "action-group-control",
        "Control and keys",
        ["action-invite", "action-accept", "action-admit", "action-remove", "action-keys"],
      ],
    ] as const;
    for (const [testId, heading, rows] of groups) {
      const group = screen.getByTestId(testId);
      expect(within(group).getAllByRole("heading")[0]).toHaveTextContent(heading);
      for (const row of rows) {
        expect(within(group).getByTestId(row)).toBeInTheDocument();
      }
    }
  });

  it("names every operation with one line saying what it does", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");

    const actions = [
      "action-trust",
      "action-revoke",
      "action-witnesses",
      "action-push",
      "action-profile",
      "action-handle",
      "action-keys",
      "action-contact",
      "action-invite",
      "action-accept",
      "action-admit",
      "action-remove",
    ];
    for (const action of actions) {
      const summary = screen.getByTestId(`${action}-summary`);
      // A title and a description, so the closed list says what each one does.
      expect(summary.textContent ?? "").toMatch(/\w.*\./);
      // Every one of them starts closed (decision 017).
      expect(screen.getByTestId(action)).toHaveAttribute("data-state", "closed");
    }
    // The sync moved to the witnesses page, so no action points at the header.
    expect(screen.queryByTestId("action-graph")).not.toBeInTheDocument();
  });

  it("names each action by the task it performs, not by the payload it signs", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");

    expect(screen.getByTestId("action-invite-summary")).toHaveTextContent(
      "Invite someone to help control this identity",
    );
    expect(screen.getByTestId("action-invite-summary")).toHaveTextContent(
      "You give them a file, they accept and send it back, and you confirm.",
    );
    const words = screen.getByTestId("identity-actions").textContent ?? "";
    expect(words).not.toMatch(/attestation|descriptor|bundle|append/i);
  });

  it("draws no principals card for a ledger holding nothing but its root", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("identity-detail");

    expect(screen.queryByTestId("principals-panel")).not.toBeInTheDocument();
    // The one principal it has is still named on the card, as an identity.
    expect(
      screen.getByTestId(`identity-detail-principal-${acmeFounder()}-link`),
    ).toHaveAttribute("href", `/identities/${acmeFounder()}`);
  });
});

function acmeFounder(): string {
  return seedIdentities.find((identity) => identity.identity_id === ACME)!.principals[0].identity;
}

import { screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { Identity, VerificationStatus } from "@/api/types";
import { OverviewCard } from "@/routes/wallet/OverviewCard";
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
      <OverviewCard identity={identity} />
    </MemoryRouter>,
  );
}

describe("overview table", () => {
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
    expect(screen.getByTestId("identity-detail-verification-note")).toHaveTextContent(
      "It grants nothing.",
    );
  });

  it("renders no marker at all for an identity claiming no hostname", () => {
    overview(withVerification("unclaimed"));

    expect(
      screen.queryByTestId("identity-detail-hostname-verification"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-hostname")).toHaveTextContent("none");
  });

  it("holds the address book fields on one line each, with the counts", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    // Every row is a key and a value on one line: the label column and the
    // value column are siblings, never stacked (decision 014).
    const row = screen.getByTestId("identity-detail-identity-id-row");
    expect(within(row).getByText("identity id").tagName).toBe("DT");
    expect(screen.getByTestId("identity-detail-created")).toHaveTextContent("2023-11-14");
    expect(screen.getByTestId("identity-detail-declared-kind")).toHaveTextContent("person");
    expect(screen.getByTestId("identity-detail-event-count")).toHaveTextContent("9");
    expect(screen.getByTestId("identity-detail-head-seq")).toHaveTextContent("8");
    expect(screen.getByTestId("identity-detail-trusted-count")).toHaveTextContent("1 identity");
    expect(screen.getByTestId("identity-detail-open-invitations")).toHaveTextContent("0");
  });

  // The two roots differ in one fact, and it is a sentence, never a null.
  it("says in words that a raw-rooted identity holds a key of its own", () => {
    overview(alice);

    expect(screen.getByTestId("identity-detail-keys")).toHaveTextContent(
      "this identity signs with a key of its own, and holds a spare to replace it with",
    );
  });

  it("says in words that an identity-rooted one is signed for by its controllers", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-keys")).toHaveTextContent(
      "this identity holds no key of its own; its controllers sign for it",
    );
    expect(screen.getByTestId("identity-detail-keys")).not.toHaveTextContent("null");
  });

  it("carries the owner badge beside the name, not in the back-link row", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    const badge = screen.getByTestId("identity-own-badge");
    expect(badge).toHaveTextContent("your identity");
    // Beside the name in the card heading, not in the row the back link owns.
    expect(screen.getByTestId("identity-detail-resolved").parentElement?.contains(badge)).toBe(
      true,
    );
    expect(screen.getByTestId("identity-back").contains(badge)).toBe(false);
  });
});

describe("ledger lines", () => {
  it("shows one line per event as its sequence and type, with no payload until it is opened", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("ledger-events");

    expect(screen.getByTestId("event-seq-0")).toHaveTextContent("0");
    expect(screen.getByTestId("event-payload-kind-0")).toHaveTextContent("inception");
    expect(screen.getByTestId("event-gloss-0")).toHaveTextContent("created this identity");
    expect(screen.queryByTestId("event-payload-0")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("event-expand-0"));

    expect(screen.getByTestId("event-detail-0")).toBeInTheDocument();
    expect(screen.getByTestId("event-payload-0")).toHaveTextContent('"nonce"');
    expect(screen.getByTestId("event-id-0")).toBeInTheDocument();

    await user.click(screen.getByTestId("event-expand-0"));

    expect(screen.queryByTestId("event-detail-0")).not.toBeInTheDocument();
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
  it("lists who this identity trusts, with the revoked attestations folded away", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("trust-panel");

    const [revoked, unrevoked] = alice.trust;
    expect(screen.getByTestId(`trust-state-${unrevoked.attestation_event}`)).toHaveTextContent(
      `trusted since position ${unrevoked.attestation_seq}`,
    );
    expect(within(screen.getByTestId("trust-list")).queryByTestId(
      `trust-row-${revoked.attestation_event}`,
    )).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("trust-revoked")).getByTestId(
        `trust-row-${revoked.attestation_event}`,
      ),
    ).toBeInTheDocument();
  });

  it("links each trusted row at the identity page of its subject", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("trust-list");

    const unrevoked = alice.trust[1];
    const link = within(screen.getByTestId(`trust-row-${unrevoked.attestation_event}`)).getByTestId(
      `trust-subject-${unrevoked.attestation_event}-link`,
    );
    expect(link).toHaveAttribute("href", `/identities/${unrevoked.subject}`);
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
      "action-verification",
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
      expect(screen.getByTestId(action)).not.toHaveAttribute("open");
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
    expect(screen.getByTestId("identity-detail-principal-count")).toHaveTextContent("1");
  });
});

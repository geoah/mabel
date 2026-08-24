import { screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, seedIdentities } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

describe("identity detail", () => {
  it("pages the ledger with the shadcn pagination and nothing else", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("ledger-events");

    // The page size is a constant: no from-position box, no how-many box, no
    // Load button and no range sentence.
    expect(screen.queryByTestId("ledger-since")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ledger-limit")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ledger-load")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ledger-range")).not.toBeInTheDocument();

    // Page one holds the first eight positions, and nothing before it.
    expect(screen.getByTestId("event-seq-0")).toHaveTextContent("0");
    expect(screen.getByTestId("ledger-page-1")).toHaveAttribute("aria-current", "page");
    expect(screen.getByTestId("ledger-previous")).toBeDisabled();
    expect(screen.getByTestId("ledger-next")).not.toBeDisabled();
    expect(screen.queryByTestId("event-seq-8")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("ledger-next"));

    // since is inclusive, so the second page opens at seq 8.
    await waitFor(() => expect(screen.getByTestId("event-seq-8")).toHaveTextContent("8"));
    expect(screen.getByTestId("ledger-page-2")).toHaveAttribute("aria-current", "page");
    expect(screen.getByTestId("ledger-next")).toBeDisabled();
    expect(screen.getByTestId("ledger-previous")).not.toBeDisabled();
    expect(screen.queryByTestId("event-seq-0")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("ledger-page-1"));

    await waitFor(() => expect(screen.getByTestId("event-seq-0")).toHaveTextContent("0"));
    expect(screen.getByTestId("ledger-event-count")).toHaveTextContent(String(alice.event_count));
  });

  it("draws no pagination for a record that fits on one page", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("ledger-events");

    expect(screen.queryByTestId("ledger-footer")).not.toBeInTheDocument();
  });

  it("titles the section Ledger, not Record", async () => {
    renderApp(`/identities/${ALICE}`);

    const panel = await screen.findByTestId("ledger-panel");
    expect(panel).toHaveTextContent("Ledger");
    expect(panel.textContent ?? "").not.toMatch(/^Record/);
  });

  // Decision 017: a summary without the entries behind it says so, rather than
  // printing zero entries against a head position that is not zero.
  it("seeds a coherent record for every identity the mock store holds", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("ledger-events");

    const acme = seedIdentities.find((identity) => identity.identity_id === ACME)!;
    expect(screen.getByTestId("ledger-event-count")).toHaveTextContent(String(acme.event_count));
    // Acme is founded by another identity, so its record opens with an
    // inception naming that founder.
    expect(screen.getByTestId("event-payload-kind-0")).toHaveTextContent("inception");
    expect(screen.queryByTestId("ledger-not-fetched")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ledger-partial")).not.toBeInTheDocument();

    await screen.findByTestId("event-expand-0");
    expect(screen.getByTestId("ledger-events")).toBeInTheDocument();
  });

  it("says a record is only a summary when the entries were never fetched", async () => {
    server.use(
      http.get("/api/identities/:identityId/ledger", ({ params }) =>
        HttpResponse.json({
          ok: true,
          ledger_id: String(params.identityId),
          declared_kind: "person",
          since: 0,
          limit: 8,
          head_seq: 4,
          head_event: "b".repeat(52),
          event_count: 0,
          more: false,
          events: [],
        }),
      ),
    );
    renderApp(`/identities/${ALICE}`);

    expect(await screen.findByTestId("ledger-not-fetched")).toHaveTextContent(
      "Your wallet holds none of this record's 5 entries yet.",
    );
    expect(screen.queryByTestId("ledger-event-count")).not.toBeInTheDocument();
  });

  it("reports one row per witness on a push", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");
    await openAction(user, "action-push");

    await user.click(screen.getByTestId("sync-push-submit"));

    await screen.findByTestId("sync-push-results");
    const [first, second] = alice.witnesses;
    expect(screen.getByTestId(`push-status-${first}`)).toHaveTextContent("accepted");
    expect(screen.getByTestId(`push-stored-${first}`)).toHaveTextContent(
      String(alice.event_count),
    );
    expect(screen.getByTestId(`push-reject-code-${first}`)).toHaveTextContent("none");
    expect(screen.getByTestId(`push-status-${second}`)).toHaveTextContent("unreachable");
    expect(screen.getByTestId(`push-head-seq-${second}`)).toHaveTextContent("none");
    expect(screen.getByTestId(`push-message-${second}`)).toHaveTextContent("Network error:");
  });

  it("adds a witness endpoint to the set the route replaces", async () => {
    const endpoint = "b".repeat(52);
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-witnesses");
    await screen.findByTestId("witness-list");

    await user.type(screen.getByTestId("witness-add-endpoint"), endpoint);
    await user.click(screen.getByTestId("witness-add-submit"));

    expect(await screen.findByTestId(`witness-row-${endpoint}`)).toBeInTheDocument();
    expect(screen.getByTestId("witness-add-head-seq")).toHaveTextContent(
      `Saved at position ${alice.head_seq + 1}.`,
    );
  });
});

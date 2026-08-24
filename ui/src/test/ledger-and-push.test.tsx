import { screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, seedIdentities } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

describe("identity detail", () => {
  it("pages the ledger with an inclusive since", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("ledger-events");

    await user.clear(screen.getByTestId("ledger-limit"));
    await user.type(screen.getByTestId("ledger-limit"), "2");
    await user.click(screen.getByTestId("ledger-load"));
    await waitFor(() =>
      expect(screen.getByTestId("ledger-more")).toHaveTextContent("There are more after these."),
    );
    expect(screen.getByTestId("event-seq-0")).toHaveTextContent("0");
    expect(screen.queryByTestId("ledger-event-2")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("ledger-next"));

    await waitFor(() => expect(screen.getByTestId("ledger-page-since")).toHaveTextContent("2"));
    // since is inclusive, so the page opens at seq 2.
    expect(screen.getByTestId("event-seq-2")).toHaveTextContent("2");
    expect(screen.getByTestId("event-payload-kind-3")).toHaveTextContent("trust_revocation");
    expect(screen.getByTestId("ledger-more")).toHaveTextContent("There are more after these.");
    expect(screen.queryByTestId("ledger-event-1")).not.toBeInTheDocument();

    // The last page says so, and reports the whole record rather than its page.
    await user.clear(screen.getByTestId("ledger-since"));
    await user.type(screen.getByTestId("ledger-since"), String(alice.head_seq));
    await user.click(screen.getByTestId("ledger-load"));

    await waitFor(() =>
      expect(screen.getByTestId("ledger-more")).toHaveTextContent("These are the last of them."),
    );
    expect(screen.getByTestId("ledger-event-count")).toHaveTextContent(String(alice.event_count));
    expect(screen.getByTestId("ledger-head-seq")).toHaveTextContent(String(alice.head_seq));
  });

  // Decision 017: a summary without the entries behind it says so, rather than
  // printing zero entries against a head position that is not zero.
  it("seeds a coherent record for every identity the demo store holds", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("ledger-events");

    const acme = seedIdentities.find((identity) => identity.identity_id === ACME)!;
    expect(screen.getByTestId("ledger-event-count")).toHaveTextContent(String(acme.event_count));
    expect(screen.getByTestId("ledger-head-seq")).toHaveTextContent(String(acme.head_seq));
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
      "Your wallet knows this record reaches position 4 but has not fetched any of its entries.",
    );
    expect(screen.queryByTestId("ledger-event-count")).not.toBeInTheDocument();
  });

  it("reports one row per witness on a push", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

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
    await screen.findByTestId("witness-list");

    await user.type(screen.getByTestId("witness-add-endpoint"), endpoint);
    await user.click(screen.getByTestId("witness-add-submit"));

    expect(await screen.findByTestId(`witness-row-${endpoint}`)).toBeInTheDocument();
    expect(screen.getByTestId("witness-add-head-seq")).toHaveTextContent(
      `Saved at position ${alice.head_seq + 1}.`,
    );
  });
});

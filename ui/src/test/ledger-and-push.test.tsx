import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ALICE, seedIdentities } from "@/mocks/fixtures";

import { renderApp } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

describe("identity detail", () => {
  it("pages the ledger with an inclusive since", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("ledger-events");

    await user.clear(screen.getByTestId("ledger-limit"));
    await user.type(screen.getByTestId("ledger-limit"), "2");
    await user.click(screen.getByTestId("ledger-load"));
    await waitFor(() => expect(screen.getByTestId("ledger-more")).toHaveTextContent("true"));
    expect(screen.getByTestId("event-seq-0")).toHaveTextContent("0");
    expect(screen.queryByTestId("ledger-event-2")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("ledger-next"));

    await waitFor(() => expect(screen.getByTestId("ledger-page-since")).toHaveTextContent("2"));
    // since is inclusive, so the page opens at seq 2.
    expect(screen.getByTestId("event-seq-2")).toHaveTextContent("2");
    expect(screen.getByTestId("event-payload-kind-3")).toHaveTextContent("trust_revocation");
    expect(screen.getByTestId("ledger-more")).toHaveTextContent("false");
    expect(screen.queryByTestId("ledger-event-1")).not.toBeInTheDocument();
  });

  it("reports one row per witness on a push", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    await user.click(screen.getByTestId("sync-push-submit"));

    await screen.findByTestId("sync-push-results");
    const [first, second] = alice.witnesses;
    expect(screen.getByTestId(`push-status-${first}`)).toHaveTextContent("accepted");
    expect(screen.getByTestId(`push-stored-${first}`)).toHaveTextContent("4");
    expect(screen.getByTestId(`push-reject-code-${first}`)).toHaveTextContent("null");
    expect(screen.getByTestId(`push-status-${second}`)).toHaveTextContent("unreachable");
    expect(screen.getByTestId(`push-head-seq-${second}`)).toHaveTextContent("null");
    expect(screen.getByTestId(`push-message-${second}`)).toHaveTextContent("Network error:");
  });

  it("adds a witness endpoint to the set the route replaces", async () => {
    const endpoint = "b".repeat(52);
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("witness-list");

    await user.type(screen.getByTestId("witness-add-endpoint"), endpoint);
    await user.click(screen.getByTestId("witness-add-submit"));

    expect(await screen.findByTestId(`witness-row-${endpoint}`)).toBeInTheDocument();
    expect(screen.getByTestId("witness-add-head-seq")).toHaveTextContent("head_seq 4");
  });
});

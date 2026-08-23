import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, seedIdentities } from "@/mocks/fixtures";
import { server } from "@/mocks/server";
import { MISMATCHED_HOSTNAME, UNREACHABLE_HOSTNAME } from "@/mocks/store";

import { renderApp } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

/** Every hostname the screen sent to GET /api/resolve, in order. */
function resolveCalls(): string[] {
  const asked: string[] = [];
  server.events.on("request:start", ({ request }) => {
    const { pathname } = new URL(request.url);
    if (pathname.startsWith("/api/resolve/")) {
      asked.push(decodeURIComponent(pathname.slice("/api/resolve/".length)));
    }
  });
  return asked;
}

describe("the identity card list", () => {
  it("draws one card per local identity, with the name, id, kind and head seq", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const card = screen.getByTestId(`identity-card-${ALICE}`);
    const name = within(card).getByTestId(`identity-card-name-${ALICE}`);
    expect(within(name).getByTestId(`identity-card-name-${ALICE}-name`)).toHaveTextContent(
      "Alice Ashworth",
    );
    expect(name).toHaveAttribute("data-identity-id", ALICE);
    expect(
      within(name).getByTestId(`identity-card-name-${ALICE}-verification`),
    ).toHaveAttribute("data-verification", "verified");
    expect(within(card).getByTestId(`identity-card-declared-kind-${ALICE}`)).toHaveTextContent(
      "person",
    );
    expect(within(card).getByTestId(`identity-card-head-seq-${ALICE}`)).toHaveTextContent(
      `head seq ${alice.head_seq}`,
    );
  });

  it("makes the whole card the link to the identity page", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const link = screen.getByTestId(`identity-card-link-${ACME}`);
    expect(link).toHaveAttribute("href", `/identities/${ACME}`);

    await user.click(link);

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-identity-id")).toHaveTextContent(ACME);
  });

  it("offers no selection: no radio, no remembered identity", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.queryByRole("radio")).toBeNull();
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByTestId("identity-selector")).not.toBeInTheDocument();
    expect(globalThis.localStorage.getItem("mabel.selected_identity")).toBeNull();
  });

  it("folds the create form away behind one button", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId("identity-create")).not.toHaveAttribute("open");

    await user.click(screen.getByTestId("identity-create-summary"));

    expect(screen.getByTestId("identity-create-alias")).toBeVisible();
  });
});

describe("the wallet search box", () => {
  it("opens the identity page for an identity id without asking DNS", async () => {
    const resolved = resolveCalls();
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), ALICE);
    await user.click(screen.getByTestId("wallet-search-submit"));

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-identity-id")).toHaveTextContent(ALICE);
    expect(resolved).toEqual([]);
  });

  it("resolves a hostname through the node and opens what it named", async () => {
    const resolved = resolveCalls();
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), "alice.example");
    await user.click(screen.getByTestId("wallet-search-submit"));

    await screen.findByTestId("identity-detail");
    expect(screen.getByTestId("identity-detail-identity-id")).toHaveTextContent(ALICE);
    expect(resolved).toEqual(["alice.example"]);
  });

  it.each([
    ["nobody.example", "no_record", "holds no mabel record"],
    [MISMATCHED_HOSTNAME, "mismatched_records", "none of them parses as an identity id"],
    [UNREACHABLE_HOSTNAME, "unreachable", "could not be answered by the resolver"],
  ])("says what the TXT lookup answered for %s", async (hostname, status, sentence) => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), hostname);
    await user.click(screen.getByTestId("wallet-search-submit"));

    const answer = await screen.findByTestId("wallet-search-status");
    expect(answer).toHaveAttribute("data-status", status);
    expect(answer).toHaveTextContent(`_mabel.${hostname}.`);
    expect(answer).toHaveTextContent(sentence);
    // Nothing navigated: the wallet is still the wallet.
    expect(screen.getByTestId("identity-cards")).toBeInTheDocument();
  });

  it("renders the envelope for a string that is neither an id nor a hostname", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), "alice_example");
    await user.click(screen.getByTestId("wallet-search-submit"));

    const envelope = await screen.findByTestId("wallet-search-error");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("malformed_hostname");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 10");
  });

  it("drops the last answer when the box is submitted again", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-form");

    await user.type(screen.getByTestId("wallet-search-input"), "nobody.example");
    await user.click(screen.getByTestId("wallet-search-submit"));
    await screen.findByTestId("wallet-search-status");

    await user.clear(screen.getByTestId("wallet-search-input"));
    await user.type(screen.getByTestId("wallet-search-input"), UNREACHABLE_HOSTNAME);
    await user.click(screen.getByTestId("wallet-search-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("wallet-search-status")).toHaveAttribute(
        "data-status",
        "unreachable",
      ),
    );
  });
});

import { screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { Identity } from "@/api/types";
import {
  bareIdentity,
  factsFromIdentity,
  factsFromResolved,
  IdentityCard,
  IdentityCardList,
  IdentityPillScope,
  listTestIds,
  pageTestIds,
  type PillFacts,
} from "@/components/identity";
import { ACME, ALICE, BOB, seedIdentities } from "@/mocks/fixtures";

import { renderApp, renderComponent } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;
const acme = seedIdentities.find((identity) => identity.identity_id === ACME)!;

function facts(overrides: Partial<PillFacts> = {}): PillFacts {
  return {
    own: new Set<string>(),
    trusted: new Set<string>(),
    degrees: new Map<string, number>(),
    ...overrides,
  };
}

function card(identity: Identity, state: "collapsed" | "expanded" | "page" = "collapsed") {
  return renderComponent(
    <MemoryRouter>
      <IdentityPillScope facts={facts({ own: new Set([identity.identity_id]) })}>
        <IdentityCard
          facts={factsFromIdentity(identity, `/identities/${identity.identity_id}`)}
          state={state}
          testIds={listTestIds(identity.identity_id)}
          linkTestId={`identity-card-link-${identity.identity_id}`}
        />
      </IdentityPillScope>
    </MemoryRouter>,
  );
}

describe("the card's layout", () => {
  it("reads kind, then name, with the pill in the top right corner", () => {
    card(alice);

    const kind = screen.getByTestId(`identity-card-kind-line-${ALICE}`);
    const title = screen.getByTestId(`identity-card-name-${ALICE}`).closest("[data-slot=item-title]");
    const pill = screen.getByTestId(`identity-card-name-${ALICE}-pill`);

    expect(kind).toHaveAttribute("data-slot", "item-description");
    expect(kind).toHaveTextContent("person");
    // The kind line comes before the name line in the DOM, so it reads first.
    expect(kind.compareDocumentPosition(title!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    // The pill is in the item's actions slot, which is the top right corner.
    expect(pill.closest("[data-slot=item-actions]")).not.toBeNull();
    expect(screen.getByTestId(`identity-card-name-${ALICE}`).contains(pill)).toBe(false);
  });

  it("opens the identity page on a click anywhere that is not a control", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    await user.click(screen.getByTestId(`identity-card-kind-line-${ALICE}`));

    expect(await screen.findByTestId("identity-detail")).toBeInTheDocument();
  });

  it("keeps the expand control and the copy button off the card's own click", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    await user.click(screen.getByTestId(`identity-card-expand-${ALICE}`));

    expect(screen.getByTestId(`identity-card-details-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId("identity-cards")).toBeInTheDocument();
    expect(screen.queryByTestId("identity-detail")).not.toBeInTheDocument();

    const heading = screen.getByTestId(`identity-card-name-${ALICE}`);
    await user.click(within(heading).getByLabelText("copy"));

    expect(screen.getByTestId("identity-cards")).toBeInTheDocument();
    expect(screen.queryByTestId("identity-detail")).not.toBeInTheDocument();
  });

  it("draws no card click for an entry that routes nowhere", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(alice)} testIds={listTestIds(ALICE)} />
      </MemoryRouter>,
    );

    expect(screen.getByTestId(`identity-card-${ALICE}`).className).not.toMatch(/cursor-pointer/);
  });
});

describe("the collapsed card", () => {
  it("holds the name, the id with a copy button, the pill, the email and the kind", () => {
    card(alice);

    expect(screen.getByTestId(`identity-card-name-${ALICE}-name`)).toHaveTextContent(
      "Alice Ashworth",
    );
    expect(screen.getByTestId(`identity-card-link-${ALICE}`)).toHaveAttribute(
      "href",
      `/identities/${ALICE}`,
    );
    const heading = screen.getByTestId(`identity-card-name-${ALICE}`);
    expect(within(heading).getByLabelText("copy")).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-name-${ALICE}-pill`)).toHaveTextContent(
      "your identity",
    );
    expect(screen.getByTestId(`identity-card-email-${ALICE}`)).toHaveTextContent(
      "alice@alice.example",
    );
    expect(screen.getByTestId(`identity-card-declared-kind-${ALICE}`)).toHaveTextContent("person");
    expect(screen.getByTestId(`identity-card-head-seq-${ALICE}`)).toHaveTextContent(
      `at position ${alice.head_seq}`,
    );
  });

  it("draws no email line for an identity publishing none", () => {
    card(acme);

    expect(screen.queryByTestId(`identity-card-email-${ACME}`)).not.toBeInTheDocument();
  });

  it("keeps the record out of the DOM until the expand control is pressed", async () => {
    const { user } = card(alice);

    expect(screen.queryByTestId(`identity-card-details-${ALICE}`)).not.toBeInTheDocument();
    const expand = screen.getByTestId(`identity-card-expand-${ALICE}`);
    expect(expand).toHaveAttribute("aria-expanded", "false");

    await user.click(expand);

    expect(screen.getByTestId(`identity-card-details-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-created-${ALICE}`)).toHaveTextContent("2023-11-14");
    expect(expand).toHaveAttribute("aria-expanded", "true");
    // The short line is the closed card's version of a row the open one holds
    // in full, so the two never say the same thing twice.
    expect(screen.queryByTestId(`identity-card-head-seq-${ALICE}`)).toHaveTextContent(
      String(alice.head_seq),
    );

    await user.click(expand);

    expect(screen.queryByTestId(`identity-card-details-${ALICE}`)).not.toBeInTheDocument();
  });

  it("starts open in the expanded state", () => {
    card(alice, "expanded");

    expect(screen.getByTestId(`identity-card-details-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-expand-${ALICE}`)).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });
});

describe("the page state", () => {
  it("is the same block, always open, with no toggle", () => {
    card(alice, "page");

    expect(screen.getByTestId(`identity-card-details-${ALICE}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`identity-card-expand-${ALICE}`)).not.toBeInTheDocument();
  });

  it("names its rows with the identity page's own testids", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(alice)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    expect(screen.getByTestId("identity-detail")).toBeInTheDocument();
    expect(screen.getByTestId("identity-detail-resolved-name")).toHaveTextContent("Alice Ashworth");
    expect(screen.getByTestId("identity-detail-email")).toHaveTextContent("alice@alice.example");
    expect(screen.getByTestId("identity-detail-alias")).toHaveTextContent(alice.alias);
    expect(screen.getByTestId("identity-detail-event-count")).toHaveTextContent(
      String(alice.event_count),
    );
    expect(screen.getByTestId("identity-detail-head-seq")).toHaveTextContent(
      String(alice.head_seq),
    );
    expect(screen.getByTestId("identity-detail-trusted-count")).toHaveTextContent("1 identity");
  });

  it("renders every principal as a linked identity, never as a count", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(alice)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    const row = screen.getByTestId("identity-detail-principals");
    for (const principal of alice.principals) {
      expect(
        within(row).getByTestId(`identity-detail-principal-${principal.identity}-link`),
      ).toHaveAttribute("href", `/identities/${principal.identity}`);
    }
    expect(screen.queryByTestId("identity-detail-principal-count")).not.toBeInTheDocument();
  });

  it("says once, on the principals row, that a founded identity's controllers sign for it", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(acme)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    expect(screen.getByTestId("identity-detail-founded")).toHaveTextContent(
      "Its controllers sign for it.",
    );
    // The key-facts sentence and its keyless sibling are gone (proposal 005).
    expect(screen.queryByTestId("identity-detail-keys")).not.toBeInTheDocument();
    expect(screen.getByTestId("identity-detail")).not.toHaveTextContent(
      "signs with a key of its own",
    );
  });

  it("names the invitations row by what an unanswered invitation is", () => {
    const invited: Identity = { ...alice, open_invitation_count: 2 };
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(invited)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    const row = screen.getByTestId("identity-detail-open-invitations-row");
    expect(row).toHaveTextContent("invitations not yet answered");
    expect(screen.getByTestId("identity-detail-open-invitations")).toHaveTextContent(
      "2 invitations to help control this identity, still waiting for an answer",
    );
  });

  it("says a record this wallet does not hold is not held, rather than printing zeroes", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard
          facts={factsFromResolved(bareIdentity(BOB))}
          state="page"
          testIds={pageTestIds}
        />
      </MemoryRouter>,
    );

    expect(screen.getByTestId("identity-detail-ledger-summary")).toHaveTextContent(
      "your wallet holds no copy of it",
    );
    expect(screen.queryByTestId("identity-detail-event-count")).not.toBeInTheDocument();
  });

  it("carries no advisory sentence about the declared kind", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(alice)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    expect(screen.getByTestId("identity-detail")).not.toHaveTextContent("It grants nothing");
    expect(screen.queryByTestId("identity-detail-declared-kind-note")).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-detail-verification-note")).not.toBeInTheDocument();
  });
});

describe("the card list", () => {
  it("draws one column at every width, and one card per entry", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCardList
          entries={[alice, acme].map((identity) => ({
            facts: factsFromIdentity(identity, `/identities/${identity.identity_id}`),
          }))}
          testId="identity-cards"
          empty="nothing"
        />
      </MemoryRouter>,
    );

    const list = screen.getByTestId("identity-cards");
    // A second column at any breakpoint is the thing proposal 005 forbids.
    expect(list.className).not.toMatch(/grid-cols/);
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-${ACME}`)).toBeInTheDocument();
  });

  it("says what it holds nothing of, under its own testid", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCardList entries={[]} testId="identity-cards" empty="You have no identities yet." />
      </MemoryRouter>,
    );

    expect(screen.getByTestId("identity-cards-empty")).toHaveTextContent(
      "You have no identities yet.",
    );
  });
});

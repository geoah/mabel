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
  resolvedFrom,
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
  it("reads name, kind and pills on one line, with the id under it", () => {
    card(alice);

    const heading = screen.getByTestId(`identity-card-name-${ALICE}`);
    const name = screen.getByTestId(`identity-card-name-${ALICE}-name`);
    const kind = screen.getByTestId(`identity-card-declared-kind-${ALICE}`);
    const pill = screen.getByTestId(`identity-card-name-${ALICE}-pill`);
    const expand = screen.getByTestId(`identity-card-expand-${ALICE}`);

    expect(kind).toHaveTextContent("person");
    // The kind leads the pill row, which shares the name's row and ends it.
    expect(kind.parentElement).toBe(pill.parentElement);
    expect(kind.parentElement?.firstElementChild).toBe(kind);
    expect(pill.parentElement).toBe(expand.parentElement);
    expect(pill.parentElement?.parentElement).toBe(name.parentElement?.parentElement);
    // The id comes under the row, across the whole card.
    const id = heading.querySelector(`[data-value="${ALICE}"]`);
    expect(name.parentElement?.parentElement?.contains(id!)).toBe(false);
    // One surface: the card draws the only border, and nothing inside it draws
    // a second one.
    const card_ = screen.getByTestId(`identity-card-${ALICE}`);
    for (const inner of card_.querySelectorAll("div")) {
      expect(inner.className).not.toMatch(/(^|\s)border($|\s)/);
    }
  });

  it("opens the identity page from the name, whose link covers the card", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    const link = screen.getByTestId(`identity-card-link-${ALICE}`);
    // The anchor's own box is stretched over the card, which is what makes the
    // whole card clickable without a click handler on a div.
    expect(link.className).toMatch(/after:absolute/);

    await user.click(link);

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
    await user.click(within(heading).getByLabelText("Copy Mabel ID"));

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
  it("holds the name, the nickname, the whole id with a copy button, the pill and the kind", () => {
    card(alice);

    expect(screen.getByTestId(`identity-card-name-${ALICE}-name`)).toHaveTextContent(
      "Alice Ashworth",
    );
    // The name you gave them, in parentheses after the name they publish.
    expect(screen.getByTestId(`identity-card-name-${ALICE}-nickname`)).toHaveTextContent(
      `(${alice.alias})`,
    );
    expect(screen.getByTestId(`identity-card-link-${ALICE}`)).toHaveAttribute(
      "href",
      `/identities/${ALICE}`,
    );
    const heading = screen.getByTestId(`identity-card-name-${ALICE}`);
    expect(within(heading).getByLabelText("Copy Mabel ID")).toBeInTheDocument();
    // A card has the room for the whole Mabel ID, and it draws all of it.
    const id = heading.querySelector(`[data-value="${ALICE}"]`);
    expect(id).toHaveAttribute("data-truncated", "false");
    expect(id).toHaveTextContent(ALICE);
    expect(screen.getByTestId(`identity-card-name-${ALICE}-pill`)).toHaveTextContent(
      "your identity",
    );
    expect(screen.getByTestId(`identity-card-declared-kind-${ALICE}`)).toHaveTextContent("person");
    // A position on the record is not a fact about the identity: no card says one.
    expect(screen.getByTestId(`identity-card-${ALICE}`)).not.toHaveTextContent("at position");
  });

  it("keeps the public email out of the closed card and in the record it opens", async () => {
    const { user } = card(alice);

    expect(screen.queryByTestId(`identity-card-email-${ALICE}`)).not.toBeInTheDocument();

    await user.click(screen.getByTestId(`identity-card-expand-${ALICE}`));

    expect(screen.getByTestId(`identity-card-email-${ALICE}`)).toHaveTextContent(
      "alice@alice.example",
    );
    expect(screen.getByTestId(`identity-card-email-${ALICE}-row`)).toHaveTextContent("email");
  });

  it("draws no email row for an identity publishing none", async () => {
    const { user } = card(acme);

    await user.click(screen.getByTestId(`identity-card-expand-${ACME}`));

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

    await user.click(expand);

    expect(screen.queryByTestId(`identity-card-details-${ALICE}`)).not.toBeInTheDocument();
  });

  it("draws the expand control as a pressable button with a vertical chevron", async () => {
    const { user } = card(alice);

    const expand = screen.getByTestId(`identity-card-expand-${ALICE}`);
    // It reads as a button: a border of its own and a hover state.
    expect(expand.tagName).toBe("BUTTON");
    expect(expand.className).toMatch(/border/);
    expect(expand.className).toMatch(/hover:bg-accent/);
    expect(expand).toHaveAttribute("aria-label", "Show the record");

    const chevron = expand.querySelector("[data-slot='collapsible-chevron']");
    // Closed it points down, at the content it opens; open it points back up.
    expect(chevron).toHaveAttribute("data-state", "closed");
    expect(chevron?.getAttribute("class")).not.toMatch(/rotate-90/);
    expect(chevron?.getAttribute("class")).not.toMatch(/rotate-180/);

    await user.click(expand);

    expect(expand).toHaveAttribute("aria-label", "Hide the record");
    expect(
      expand.querySelector("[data-slot='collapsible-chevron']")?.getAttribute("class"),
    ).toMatch(/rotate-180/);
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
    // One casing for every row label on every card: lowercase.
    expect(screen.getByTestId("identity-detail-alias-row")).toHaveTextContent("nickname");
    expect(screen.getByTestId("identity-detail-contact-row")).toHaveTextContent("note");
    for (const row of screen.getByTestId("identity-detail-details").querySelectorAll("dt")) {
      expect(row.textContent ?? "").toMatch(/^[a-z]/);
    }
    // The note sits directly under the nickname, which is the pair a reader edits.
    expect(
      screen
        .getByTestId("identity-detail-alias-row")
        .compareDocumentPosition(screen.getByTestId("identity-detail-contact-row")),
    ).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByTestId("identity-detail-event-count")).toHaveTextContent(
      String(alice.event_count),
    );
    expect(screen.getByTestId("identity-detail-trusted-count")).toHaveTextContent("1 identity");
  });

  it("renders every principal as a linked identity, never as a count", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(acme)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    const row = screen.getByTestId("identity-detail-principals");
    for (const principal of acme.principals) {
      expect(
        within(row).getByTestId(`identity-detail-principal-${principal.identity}-link`),
      ).toHaveAttribute("href", `/identities/${principal.identity}`);
    }
    expect(screen.queryByTestId("identity-detail-principal-count")).not.toBeInTheDocument();
  });

  it("names the principals with the names the screen resolved, not their ids", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard
          facts={factsFromIdentity(acme)}
          state="page"
          testIds={pageTestIds}
          resolvePrincipal={() => resolvedFrom(alice)}
        />
      </MemoryRouter>,
    );

    expect(
      screen.getByTestId(`identity-detail-principal-${ALICE}-name`),
    ).toHaveTextContent("Alice Ashworth");
  });

  it("draws no principals row for an identity whose only principal is itself", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard facts={factsFromIdentity(alice)} state="page" testIds={pageTestIds} />
      </MemoryRouter>,
    );

    // Alice keys herself: "who can act for it" would answer with the identity
    // the heading already names.
    expect(alice.principals.map((principal) => principal.identity)).toEqual([ALICE]);
    expect(screen.queryByTestId("identity-detail-principals")).not.toBeInTheDocument();
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
    expect(row).toHaveTextContent("invitations");
    expect(screen.getByTestId("identity-detail-open-invitations")).toHaveTextContent(
      "2 waiting for an answer",
    );
  });

  it("says a record this wallet does not hold is not held, rather than printing zeroes", () => {
    renderComponent(
      <MemoryRouter>
        <IdentityCard
          facts={factsFromResolved(bareIdentity(BOB), { stored: false })}
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

import { screen, within } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it } from "vitest";

import {
  ACME,
  ALICE,
  SYNTHETIC_ENTRIES,
  TRUNCATED_LEDGER,
  witnessForkFixture,
  witnessLedgerEntries,
} from "@/mocks/fixtures";
import { server } from "@/mocks/server";
import { setNodeRole } from "@/mocks/store";

import { renderApp } from "./render";

const acme = witnessLedgerEntries.find((entry) => entry.ledger_id === ACME)!;
const truncated = SYNTHETIC_ENTRIES.find((entry) => entry.ledger_id === TRUNCATED_LEDGER)!;
/** Ascending ledger id: the two frozen entries plus the four the mock mints. */
const ORDERED_IDS = [ACME, ...SYNTHETIC_ENTRIES.map((entry) => entry.ledger_id), ALICE].sort();
const FORK_KEY = `${ALICE}-${witnessForkFixture.seq}`;

function cardIds(): string[] {
  return screen
    .getAllByTestId(/^identity-card-[a-z2-7]{52}$/)
    .map((card) => card.getAttribute("data-testid")!.replace("identity-card-", ""));
}

describe("the witness node's debug route", () => {
  // A node has one role; the mock serves the witness document for these tests.
  beforeEach(() => setNodeRole("witness"));

  it("lists what this witness holds as identity cards, ordered by ledger id", async () => {
    renderApp("/witness");
    await screen.findByTestId("identity-cards");

    expect(cardIds()).toEqual(ORDERED_IDS);
    expect(screen.getByTestId("witness-holdings-note")).toHaveTextContent(
      "A record missing here may still be on another witness.",
    );
    expect(screen.getByTestId("witness-read-only-note")).toHaveTextContent(
      "This page only reads. Nothing here changes anything.",
    );
    const card = screen.getByTestId(`identity-card-${ACME}`);
    expect(within(card).getByTestId(`identity-card-declared-kind-${ACME}`)).toHaveTextContent(
      "organization",
    );
    expect(within(card).getByTestId(`identity-card-entries-${ACME}`)).toHaveTextContent(
      `${acme.head_seq + 1} entries`,
    );
  });

  it("marks a ledger's fork count on its card and says when recording stopped", async () => {
    renderApp("/witness");
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId(`identity-card-fork-count-${TRUNCATED_LEDGER}`)).toHaveTextContent(
      `${truncated.fork_count} conflicts, and it stopped recording more`,
    );
    expect(
      screen.queryByTestId(`identity-card-fork-count-${ACME}`),
    ).not.toBeInTheDocument();
  });

  // No page control means the screen owes the reader every record, so it
  // follows the route's `more` instead of drawing the first page as the whole.
  it("reads past the first page of records, with no control to do it", async () => {
    const asked: string[] = [];
    server.events.on("request:start", ({ request }) => {
      const url = new URL(request.url);
      if (url.pathname === "/api/ledgers") {
        asked.push(url.search);
      }
    });
    const second = "d".repeat(52);
    server.use(
      http.get("/api/ledgers", ({ request }) => {
        const offset = Number(new URL(request.url).searchParams.get("offset") ?? "0");
        return HttpResponse.json({
          ok: true,
          offset,
          limit: 1,
          more: offset === 0,
          entries: [
            {
              ledger_id: offset === 0 ? ALICE : second,
              declared_kind: "person",
              head_seq: 0,
              head_event: "e".repeat(52),
              event_count: 1,
              first_seen_ms: 1_700_000_000_000,
              updated_ms: 1_700_000_000_000,
              fork_count: 0,
              forks_truncated: false,
              source_endpoint: "f".repeat(52),
            },
          ],
        });
      }),
    );

    renderApp("/witness");
    await screen.findByTestId("identity-cards");

    expect(await screen.findByTestId(`identity-card-${second}`)).toBeInTheDocument();
    expect(asked).toHaveLength(2);
    expect(screen.queryByTestId("witness-ledger-list-capped")).not.toBeInTheDocument();
    expect(screen.queryByTestId("witness-ledger-next")).not.toBeInTheDocument();
  });

  it("offers no paging controls and no operator table", async () => {
    const { container } = renderApp("/witness");
    await screen.findByTestId("identity-cards");

    expect(screen.queryByTestId("witness-ledger-next")).not.toBeInTheDocument();
    expect(screen.queryByTestId("witness-ledger-offset")).not.toBeInTheDocument();
    expect(screen.queryByTestId("witness-node-info")).not.toBeInTheDocument();
    expect(screen.queryByTestId("witness-forks")).not.toBeInTheDocument();
    expect(container.querySelectorAll("table")).toHaveLength(0);
    expect(container.querySelectorAll("form")).toHaveLength(0);
  });

  it("opens one ledger as the identity page, with its chain as expandable lines", async () => {
    const { user } = renderApp("/witness");
    await screen.findByTestId(`identity-card-link-${ALICE}`);

    await user.click(screen.getByTestId(`identity-card-link-${ALICE}`));

    await screen.findByTestId("ledger-events");
    expect(screen.getByTestId("witness-detail-declared-kind")).toHaveTextContent("person");
    expect(screen.getByTestId("witness-detail-head-seq")).toHaveTextContent("3");
    expect(screen.getByTestId("event-gloss-0")).toHaveTextContent("created this identity");
    expect(screen.queryByTestId("event-payload-0")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("event-expand-0"));

    expect(screen.getByTestId("event-detail-0")).toBeInTheDocument();
    expect(screen.getByTestId("event-payload-0")).toHaveTextContent('"nonce"');
  });

  it("shows both events of a fork record on the ledger's own page", async () => {
    renderApp(`/witness/ledgers/${ALICE}`);
    const record = await screen.findByTestId(`fork-record-${FORK_KEY}`);

    expect(screen.getByTestId(`fork-seq-${FORK_KEY}`)).toHaveTextContent(
      String(witnessForkFixture.seq),
    );
    expect(screen.getByTestId(`fork-kept-${FORK_KEY}-event-id`)).toHaveTextContent(
      witnessForkFixture.kept.event_id,
    );
    expect(screen.getByTestId(`fork-conflicting-${FORK_KEY}-event-id`)).toHaveTextContent(
      witnessForkFixture.conflicting.event_id,
    );
    expect(within(record).getAllByTestId(/^fork-(kept|conflicting)-.*-seq$/)).toHaveLength(2);
    // The statement is the node's own wording, rendered verbatim.
    expect(screen.getByTestId(`fork-statement-${FORK_KEY}`)).toHaveTextContent(
      "equivocation or of a lost race between honest controllers",
    );
    expect(screen.getByTestId("fork-evidence-note")).toHaveTextContent(
      "proves nothing beyond the conflict",
    );
    expect(document.body.textContent).not.toMatch(/malicious|attacker|cheating|fraud/i);
    // Only this ledger's records, and no other ledger's, are on this page.
    expect(screen.queryByTestId(`fork-record-${TRUNCATED_LEDGER}-1`)).not.toBeInTheDocument();
  });

  it("draws no fork section on a ledger with no fork records", async () => {
    renderApp(`/witness/ledgers/${ACME}`);
    await screen.findByTestId("witness-ledger-detail");

    expect(screen.queryByTestId("witness-forks")).not.toBeInTheDocument();
    expect(screen.getByTestId("witness-detail-fork-count")).toHaveTextContent("0");
  });

  it("issues only reads and offers no mutating control", async () => {
    const methods: string[] = [];
    server.events.on("request:start", ({ request }) => methods.push(request.method));

    const { user, container } = renderApp("/witness");
    await screen.findByTestId("identity-cards");
    expect(screen.getByTestId("nav-witness")).toHaveTextContent("Records");
    expect(screen.queryByTestId("nav-wallet")).not.toBeInTheDocument();

    await user.click(await screen.findByTestId(`identity-card-link-${ALICE}`));
    await screen.findByTestId("ledger-events");
    for (const button of screen.getAllByRole("button")) {
      if (!(button as HTMLButtonElement).disabled) {
        await user.click(button);
      }
    }

    expect(methods.length).toBeGreaterThan(0);
    expect(methods.filter((method) => method !== "GET")).toEqual([]);
    expect(container.querySelectorAll("form")).toHaveLength(0);
    // Every button on the route opens something already fetched: an event line,
    // a truncated identifier, or the clipboard.
    expect(container.querySelectorAll("button[type=submit]")).toHaveLength(0);
  });
});

import { screen, waitFor, within } from "@testing-library/react";
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

function rowIds(): string[] {
  return screen
    .getAllByTestId(/^witness-ledger-row-/)
    .map((row) => row.getAttribute("data-testid")!.replace("witness-ledger-row-", ""));
}

describe("witness route", () => {
  // A node has one role; the mock serves the witness document for these tests.
  beforeEach(() => setNodeRole("witness"));

  it("lists what this witness holds, ordered by ledger id, one page at a time", async () => {
    const { user } = renderApp("/witness");
    await screen.findByTestId("witness-ledger-table");

    expect(rowIds()).toEqual(ORDERED_IDS.slice(0, 4));
    expect(screen.getByTestId("witness-ledger-more")).toHaveTextContent("more true");
    expect(screen.getByTestId("witness-holdings-note")).toHaveTextContent(
      "a diagnostic and not an index",
    );
    expect(screen.getByTestId(`witness-ledger-head-seq-${ACME}`)).toHaveTextContent(
      String(acme.head_seq),
    );
    expect(screen.getByTestId(`witness-ledger-head-event-${ACME}`)).toHaveTextContent(
      acme.head_event,
    );
    expect(screen.getByTestId(`witness-ledger-event-count-${ACME}`)).toHaveTextContent(
      String(acme.event_count),
    );
    expect(screen.getByTestId(`witness-ledger-first-seen-ms-${ACME}`)).toHaveTextContent(
      String(acme.first_seen_ms),
    );
    expect(screen.getByTestId(`witness-ledger-updated-ms-${ACME}`)).toHaveTextContent(
      String(acme.updated_ms),
    );
    expect(screen.getByTestId(`witness-ledger-source-endpoint-${ACME}`)).toHaveTextContent(
      acme.source_endpoint,
    );

    await user.click(screen.getByTestId("witness-ledger-next"));

    await waitFor(() => expect(rowIds()).toEqual(ORDERED_IDS.slice(4)));
    expect(screen.getByTestId("witness-ledger-offset")).toHaveTextContent("offset 4");
    expect(screen.getByTestId("witness-ledger-more")).toHaveTextContent("more false");
    expect(screen.queryByTestId(`witness-ledger-row-${ACME}`)).not.toBeInTheDocument();

    await user.click(screen.getByTestId("witness-ledger-previous"));

    await waitFor(() => expect(rowIds()).toEqual(ORDERED_IDS.slice(0, 4)));
  });

  it("labels the kind column declared and repeats that it gates nothing", async () => {
    renderApp("/witness");
    await screen.findByTestId("witness-ledger-table");

    const headers = within(screen.getByTestId("witness-ledger-table"))
      .getAllByRole("columnheader")
      .map((header) => header.textContent);
    expect(headers).toContain("declared");
    expect(headers).not.toContain("kind");
    expect(screen.getByTestId(`witness-ledger-declared-kind-${ACME}`)).toHaveTextContent(
      "organization",
    );
    expect(screen.getByTestId("witness-ledger-declared-kind-note")).toHaveTextContent(
      "declared kind is advisory",
    );
  });

  it("flags the ledger whose fork records are truncated", async () => {
    renderApp("/witness");
    await screen.findByTestId("witness-ledger-table");

    expect(
      screen.getByTestId(`witness-ledger-fork-count-${TRUNCATED_LEDGER}`),
    ).toHaveTextContent(String(truncated.fork_count));
    expect(
      screen.getByTestId(`witness-ledger-forks-truncated-${TRUNCATED_LEDGER}`),
    ).toHaveTextContent("forks_truncated true");
    expect(screen.getByTestId(`witness-ledger-forks-truncated-${ACME}`)).toHaveTextContent(
      "forks_truncated false",
    );
  });

  it("shows both events of a fork record as evidence, not as an accusation", async () => {
    renderApp("/witness");
    const record = await screen.findByTestId(`fork-record-${FORK_KEY}`);

    expect(screen.getByTestId(`fork-seq-${FORK_KEY}`)).toHaveTextContent(
      String(witnessForkFixture.seq),
    );
    expect(screen.getByTestId(`fork-observed-ms-${FORK_KEY}`)).toHaveTextContent(
      String(witnessForkFixture.observed_ms),
    );
    expect(screen.getByTestId(`fork-source-endpoint-${FORK_KEY}`)).toHaveTextContent(
      witnessForkFixture.source_endpoint,
    );
    expect(screen.getByTestId(`fork-kept-${FORK_KEY}-event-id`)).toHaveTextContent(
      witnessForkFixture.kept.event_id,
    );
    expect(screen.getByTestId(`fork-conflicting-${FORK_KEY}-event-id`)).toHaveTextContent(
      witnessForkFixture.conflicting.event_id,
    );
    expect(screen.getByTestId(`fork-kept-${FORK_KEY}-payload-kind`)).toHaveTextContent(
      "trust_revocation",
    );
    expect(screen.getByTestId(`fork-conflicting-${FORK_KEY}-payload-kind`)).toHaveTextContent(
      "trust_attestation",
    );
    // Both events sit in the record, so a reader checks the conflict here.
    expect(within(record).getAllByTestId(/^fork-(kept|conflicting)-.*-seq$/)).toHaveLength(2);
    expect(screen.getByTestId(`fork-statement-${FORK_KEY}`)).toHaveTextContent(
      "equivocation or of a lost race between honest controllers",
    );
    expect(screen.getByTestId("fork-evidence-note")).toHaveTextContent("it authorizes nothing");
    expect(document.body.textContent).not.toMatch(/malicious|attacker|cheating|fraud/i);
    // The second seeded fork record is on the truncated ledger.
    expect(screen.getByTestId(`fork-record-${TRUNCATED_LEDGER}-1`)).toBeInTheDocument();
  });

  it("pages one ledger's events with an inclusive since", async () => {
    const { user } = renderApp("/witness");
    await screen.findByTestId("witness-ledger-table");
    await user.click(screen.getByTestId("witness-ledger-next"));
    await screen.findByTestId(`witness-ledger-link-${ALICE}`);

    await user.click(screen.getByTestId(`witness-ledger-link-${ALICE}`));
    await screen.findByTestId("witness-events-table");
    expect(screen.getByTestId("witness-detail-declared-kind")).toHaveTextContent("person");
    expect(screen.getByTestId("witness-detail-head-seq")).toHaveTextContent("3");

    await user.clear(screen.getByTestId("witness-events-limit"));
    await user.type(screen.getByTestId("witness-events-limit"), "2");
    await user.click(screen.getByTestId("witness-events-load"));

    await waitFor(() =>
      expect(screen.getByTestId("witness-events-more")).toHaveTextContent("true"),
    );
    expect(screen.getByTestId("witness-event-seq-0")).toHaveTextContent("0");
    expect(screen.queryByTestId("witness-event-2")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("witness-events-next"));

    await waitFor(() =>
      expect(screen.getByTestId("witness-events-page-since")).toHaveTextContent("2"),
    );
    // since is inclusive, so the page opens at seq 2.
    expect(screen.getByTestId("witness-event-seq-2")).toHaveTextContent("2");
    expect(screen.getByTestId("witness-event-payload-kind-3")).toHaveTextContent(
      "trust_revocation",
    );
    expect(screen.queryByTestId("witness-event-1")).not.toBeInTheDocument();
  });

  it("filters the fork list to the ledger being viewed", async () => {
    renderApp(`/witness/ledgers/${ALICE}`);
    await screen.findByTestId(`fork-record-${FORK_KEY}`);

    expect(screen.getByTestId("witness-forks-filter")).toHaveTextContent(ALICE);
    expect(screen.queryByTestId(`fork-record-${TRUNCATED_LEDGER}-1`)).not.toBeInTheDocument();
  });

  it("issues only reads and offers no mutating control", async () => {
    const methods: string[] = [];
    server.events.on("request:start", ({ request }) => methods.push(request.method));

    const { user, container } = renderApp("/witness");
    await screen.findByTestId("witness-ledger-table");
    expect(screen.getByTestId("witness-node-role")).toHaveTextContent("witness");
    expect(screen.getByTestId("witness-node-ledger-count")).toHaveTextContent("6");
    expect(screen.getByTestId("witness-node-fork-count")).toHaveTextContent("2");

    for (const button of screen.getAllByRole("button")) {
      if (!(button as HTMLButtonElement).disabled) {
        await user.click(button);
      }
    }
    await user.click(await screen.findByTestId(`witness-ledger-link-${ALICE}`));
    await screen.findByTestId("witness-events-table");
    for (const button of screen.getAllByRole("button")) {
      if (!(button as HTMLButtonElement).disabled) {
        await user.click(button);
      }
    }

    expect(methods.length).toBeGreaterThan(0);
    expect(methods.filter((method) => method !== "GET")).toEqual([]);
    expect(container.querySelectorAll("form")).toHaveLength(0);
    expect(
      screen.queryByRole("button", { name: /create|add|revoke|push|delete|save|submit|verify/i }),
    ).toBeNull();
  });
});

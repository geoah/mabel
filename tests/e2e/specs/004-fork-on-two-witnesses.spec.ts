import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_TWO_URL,
  ALICE_URL,
  apiGet,
  dcSh,
  docker,
  json,
  mustRun,
  verifier,
  WITNESS_TWO_URL,
  WITNESS_URL,
} from "../lib/docker";
import { expectExit, story004Steps1to7, type ForkState } from "../lib/stories";
import { identifier } from "../lib/ui";

/**
 * docs/stories/004-fork-on-two-witnesses.md
 *
 * Step 10's teardown is story 005 step 11's, which runs next in this suite and
 * needs what this story leaves up; the global teardown clears it either way.
 */
test.describe.configure({ mode: "serial" });

const FORK_EVIDENCE_NOTE =
  "Two valid entries were signed at the same position by whoever held the key. " +
  "That can be deliberate or two controllers acting at once, and this record " +
  "proves nothing beyond the conflict.";

let page: Page;
let state: ForkState;

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext();
  page = await context.newPage();
});

test("steps 1 to 7: two witnesses, two branches, one refused push", async () => {
  state = await story004Steps1to7();
});

test("step 5: the same prev on both machines, and both branches verify", async () => {
  const first = await apiGet(ALICE_URL, `/api/identities/${state.aliceId}/ledger?since=3`);
  const second = await apiGet(ALICE_TWO_URL, `/api/identities/${state.aliceId}/ledger?since=3`);
  expect(first.body.events[0].prev).toBe(second.body.events[0].prev);
  const seqTwo = await apiGet(ALICE_URL, `/api/identities/${state.aliceId}/ledger?since=2`);
  expect(first.body.events[0].prev).toBe(seqTwo.body.events[0].event_id);
  expect(first.body.events[0].event_id).toBe(state.keptEvent);
  expect(second.body.events[0].event_id).toBe(state.conflictingEvent);

  // The story verifies each machine's own copy with `mabel verify ledger
  // alice --json`. That command reads from a source over the network, and a
  // CLI process holds no address for one, so each branch is verified where it
  // landed: witness one holds the kept branch, witness two the conflicting one.
  const keptBranch = json(
    expectExit(
      dcSh(
        "alice",
        `mabel verify ledger alice --from ${state.witnessId} --peer "$(cat /shared/witness.ticket)" --json`,
      ),
      0,
    ),
  );
  expect(keptBranch.valid).toBe(true);
  expect(keptBranch.head_event).toBe(state.keptEvent);

  const conflictingBranch = json(
    expectExit(
      docker([
        "exec",
        "mabel-alice-two",
        "sh",
        "-c",
        `mabel verify ledger alice --from ${state.witnessTwoId} --peer "$(cat /shared/witness-two.ticket)" --json`,
      ]),
      0,
    ),
  );
  expect(conflictingBranch.valid).toBe(true);
  expect(conflictingBranch.head_event).toBe(state.conflictingEvent);
});

test("first seen wins: witness one still serves the kept branch", async () => {
  const ledger = await apiGet(WITNESS_URL, `/api/ledgers/${state.aliceId}`);
  expect(ledger.body.entry.head_seq).toBe(3);
  expect(ledger.body.entry.head_event).toBe(state.keptEvent);
  expect(ledger.body.entry.fork_count).toBe(1);
  // The UI no longer draws this flag on the list, so the route is where it is
  // pinned: nothing stopped recording, so the count is exact.
  expect(ledger.body.entry.forks_truncated).toBe(false);
  expect([...ledger.body.witnesses].sort()).toEqual([state.witnessId, state.witnessTwoId].sort());
});

test("step 8: the fork record in witness one's UI", async () => {
  await page.goto(`${WITNESS_URL}/witness`);
  // The card list is the witness route now (proposal 004), and a fork count is
  // drawn on a card only when the witness recorded one.
  await expect(page.getByTestId(`identity-card-fork-count-${state.aliceId}`)).toHaveText(
    "1 conflict",
  );
  await expect(page.locator('[data-testid^="identity-card-fork-count-"]')).toHaveCount(1);

  await page.getByTestId(`identity-card-link-${state.aliceId}`).click();
  await expect(page.getByTestId("witness-ledger-detail")).toBeVisible();
  await expect(page.getByTestId("witness-detail-fork-count")).toHaveText("1");

  const key = `${state.aliceId}-3`;
  await expect(page.getByTestId(`fork-record-${key}`)).toBeVisible();
  await expect(page.getByTestId(`fork-statement-${key}`)).toHaveText(
    `two distinct validly signed events exist at seq 3 of ${state.aliceId}, produced by whoever held signing authority there; this is evidence of equivocation or of a lost race between honest controllers`,
  );
  await expect(page.getByTestId("fork-evidence-note")).toHaveText(FORK_EVIDENCE_NOTE);

  expect(await identifier(page, `fork-kept-${key}-event-id`)).toBe(state.keptEvent);
  expect(await identifier(page, `fork-conflicting-${key}-event-id`)).toBe(state.conflictingEvent);
  for (const side of ["kept", "conflicting"]) {
    await expect(page.getByTestId(`fork-${side}-${key}-payload-kind`)).toHaveText(
      "trust_attestation",
    );
    await expect(page.getByTestId(`fork-${side}-${key}-seq`)).toHaveText("3");
  }
  expect(await identifier(page, `fork-kept-${key}-prev`)).toBe(
    await identifier(page, `fork-conflicting-${key}-prev`),
  );
  expect(await identifier(page, `fork-kept-${key}-author-key`)).toBe(
    await identifier(page, `fork-conflicting-${key}-author-key`),
  );

  const secondMachineEndpoint = mustRun("docker", [
    "exec",
    "mabel-alice-two",
    "mabel",
    "node",
    "id",
  ]).stdout.trim();
  expect(await identifier(page, `fork-source-endpoint-${key}`)).toBe(secondMachineEndpoint);
});

test("witness two recorded nothing", async () => {
  const forks = await apiGet(WITNESS_TWO_URL, "/api/forks");
  expect(forks.body.entries).toEqual([]);
  const ledger = await apiGet(WITNESS_TWO_URL, `/api/ledgers/${state.aliceId}`);
  expect(ledger.body.entry.head_event).toBe(state.conflictingEvent);
  expect(ledger.body.entry.fork_count).toBe(0);
});

test("step 9: a verifier that asks both sources exits 20 naming both", async () => {
  const result = expectExit(
    verifier(["verify", "ledger", state.aliceId, "--peer", state.witnessTwoTicket, "--json"]),
    20,
  );
  const document = json(result);
  expect(document.ok).toBe(false);
  expect(document.code).toBe(20);
  expect(document.message).toBe(
    `Ledger error: two sources hold divergent events at seq 3 of ${state.aliceId}`,
  );
  expect(document.details.reason).toBe("equivocation");
  expect(document.details.at_seq).toBe(3);
  expect(document.details.candidates).toHaveLength(2);
  const candidates = [...document.details.candidates].sort((left: any, right: any) =>
    left.source.localeCompare(right.source),
  );
  const expected = [
    { source: state.witnessId, event_id: state.keptEvent },
    { source: state.witnessTwoId, event_id: state.conflictingEvent },
  ].sort((left, right) => left.source.localeCompare(right.source));
  expect(candidates.map((candidate: any) => candidate.source)).toEqual(
    expected.map((candidate) => candidate.source),
  );
  expect(candidates.map((candidate: any) => candidate.event_id)).toEqual(
    expected.map((candidate) => candidate.event_id),
  );
});

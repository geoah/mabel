import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_TWO_URL,
  ALICE_URL,
  apiGet,
  dcSh,
  docker,
  json,
  mustRun,
  verifierWithNoWitness,
  WITNESS_TWO_URL,
  WITNESS_URL,
} from "../lib/docker";
import { expectExit, story004Steps1to7, type ForkState } from "../lib/stories";
import { shown } from "../lib/ui";

/**
 * docs/stories/004-fork-on-two-witnesses.md
 *
 * Step 10's teardown is story 005 step 11's, which runs next in this suite and
 * needs what this story leaves up; the global teardown clears it either way.
 */
test.describe.configure({ mode: "serial" });

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
  const identity = await apiGet(WITNESS_URL, `/api/identities/${state.aliceId}`);
  expect(identity.body.identity.head_seq).toBe(3);
  expect(identity.body.identity.head_event).toBe(state.keptEvent);
  // The set on the chain names the two witness identities, not their endpoints.
  expect([...identity.body.identity.witnesses].sort()).toEqual(
    [state.witnessIdentity, state.witnessTwoIdentity].sort(),
  );
});

test("step 8: the fork record, on the route that reports it", async () => {
  // A fork is a fact about a stored record, and `GET /api/forks` is the one
  // route that reports it on every node (proposal 006 section 8). The witness
  // detail screen it used to be read on is gone with the witness routes.
  const forks = await apiGet(WITNESS_URL, `/api/forks?ledger_id=${state.aliceId}`);
  expect(forks.body.entries).toHaveLength(1);
  const record = forks.body.entries[0];
  // `ledger_id` is an id-valued field and stays bare; `statement` is prose a
  // person reads, so the ledger in it carries the prefix (decision 019).
  expect(record.ledger_id).toBe(state.aliceId);
  expect(record.seq).toBe(3);
  expect(record.statement).toBe(
    `two distinct validly signed events exist at seq 3 of ${shown(state.aliceId)}, produced by whoever held signing authority there; this is evidence of equivocation or of a lost race between honest controllers`,
  );
  expect(record.kept.event_id).toBe(state.keptEvent);
  expect(record.conflicting.event_id).toBe(state.conflictingEvent);
  for (const side of [record.kept, record.conflicting]) {
    expect(side.seq).toBe(3);
    expect(side.payload_kind).toBe("trust_attestation");
  }
  // Both branches extend the same entry and were signed by the same key: that
  // is what makes this a conflict rather than two unrelated records.
  expect(record.kept.prev).toBe(record.conflicting.prev);
  expect(record.kept.author_key).toBe(record.conflicting.author_key);

  // The endpoint that offered the branch witness one refused, which is alice's
  // second machine.
  const secondMachineEndpoint = mustRun("docker", [
    "exec",
    "mabel-alice-two",
    "mabel",
    "node",
    "id",
  ]).stdout.trim();
  expect(record.source_endpoint).toBe(secondMachineEndpoint);
});

test("step 8: alice's wallet reads the conflict on the witness's own page", async () => {
  // A witness is an identity, so what it holds is a section of its identity
  // page, asked live over the sync protocol (proposal 006 section 8).
  await page.goto(`${ALICE_URL}/identities/${state.witnessIdentity}`);
  await expect(page.getByTestId("witness-holdings")).toBeVisible();
  await expect(page.getByTestId("witness-node-default")).toHaveText(
    "yes, for the identities that chose no witness of their own",
  );
  await expect(page.getByTestId(`identity-card-fork-count-${state.aliceId}`)).toHaveText(
    "1 conflict",
  );
  await expect(page.locator('[data-testid^="identity-card-fork-count-"]')).toHaveCount(1);
  await expect(page.getByTestId(`identity-card-entries-${state.aliceId}`)).toHaveText("4 entries");

  // Witness two holds the other branch and recorded no conflict, so its page
  // draws no count at all.
  await page.goto(`${ALICE_URL}/identities/${state.witnessTwoIdentity}`);
  await expect(page.getByTestId("witness-holdings")).toBeVisible();
  await expect(page.locator('[data-testid^="identity-card-fork-count-"]')).toHaveCount(0);
});

test("witness two recorded nothing", async () => {
  const forks = await apiGet(WITNESS_TWO_URL, "/api/forks");
  expect(forks.body.entries).toEqual([]);
  const identity = await apiGet(WITNESS_TWO_URL, `/api/identities/${state.aliceId}`);
  expect(identity.body.identity.head_event).toBe(state.conflictingEvent);
});

test("step 9: a verifier that asks both sources exits 20 naming both", async () => {
  // A home that was told nothing but where to look: no witness in node.json,
  // and one ticket per witness on the command line. Both are asked in
  // parallel, and both answer with a different event at seq 3.
  const result = expectExit(
    verifierWithNoWitness([
      "verify",
      "ledger",
      state.aliceId,
      "--peer",
      state.witnessTicket,
      "--peer",
      state.witnessTwoTicket,
      "--json",
    ]),
    20,
  );
  const document = json(result);
  expect(document.ok).toBe(false);
  expect(document.code).toBe(20);
  expect(document.message).toBe(
    `Ledger error: two sources hold divergent events at seq 3 of ${shown(state.aliceId)}`,
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

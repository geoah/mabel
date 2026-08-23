import { expect, test, type Page } from "@playwright/test";

import {
  ALICE_URL,
  apiGet,
  apiPost,
  BOB_URL,
  carry,
  dcSh,
  json,
  mabel,
  mustRun,
  readFileBase64,
  removeExtras,
  resetTopology,
  stdoutLines,
  witnessId as readWitnessId,
  writeFileBase64,
} from "./docker";
import { addTrust, addWitness, createIdentity, identifier, openIdentity, push } from "./ui";

export const BASE32_ID = /^[a-z2-7]{52}$/;

export interface MeetState {
  witnessId: string;
  aliceId: string;
  bobId: string;
}

/**
 * Story 001 steps 1 to 7, with the outcomes those steps verify. Stories 002,
 * 003 and 006 all open with "run story 001 steps 1 to N", so this is the one
 * implementation of them.
 */
export async function story001Steps1to7(alicePage: Page, bobPage: Page): Promise<MeetState> {
  const state: MeetState = { witnessId: "", aliceId: "", bobId: "" };

  await test.step("001 step 1: the topology from nothing", async () => {
    resetTopology();
    state.witnessId = readWitnessId();
    expect(state.witnessId).toMatch(BASE32_ID);
  });

  await test.step("001 steps 2 to 4: an identity in each wallet UI", async () => {
    for (const [page, url] of [
      [alicePage, ALICE_URL],
      [bobPage, BOB_URL],
    ] as const) {
      await page.goto(`${url}/wallet`);
      await expect(page.getByTestId("node-role")).toHaveText("wallet");
      await expect(page.getByTestId("identity-list-empty")).toHaveText(
        "no identities in this node home",
      );
    }

    const alice = await createIdentity(alicePage, { alias: "alice", kind: "person" });
    // An identity is the digest of its own inception event.
    expect(alice.identityId).toBe(alice.inceptionEvent);
    expect(alice.identityId).toMatch(BASE32_ID);
    state.aliceId = alice.identityId;

    const bob = await createIdentity(bobPage, { alias: "bob", kind: "person" });
    expect(bob.identityId).toBe(bob.inceptionEvent);
    state.bobId = bob.identityId;
  });

  await test.step("001 step 5: descriptors exchanged out of band", async () => {
    exportDescriptor("bob", "bob", state.bobId, "/tmp/bob.descriptor");
    carry("mabel-bob", "/tmp/bob.descriptor", "mabel-alice", "/tmp/bob.descriptor");
    exportDescriptor("alice", "alice", state.aliceId, "/tmp/alice.descriptor");
    carry("mabel-alice", "/tmp/alice.descriptor", "mabel-bob", "/tmp/alice.descriptor");
  });

  await test.step("001 step 6: both name the witness", async () => {
    await openIdentity(alicePage, ALICE_URL, state.aliceId);
    await addWitness(alicePage, state.witnessId, 1);
    await openIdentity(bobPage, BOB_URL, state.bobId);
    await addWitness(bobPage, state.witnessId, 1);

    const identity = await apiGet(ALICE_URL, `/api/identities/${state.aliceId}`);
    expect(identity.body.identity.witnesses).toEqual([state.witnessId]);
    expect(identity.body.identity.head_seq).toBe(1);
    expect(identity.body.identity.event_count).toBe(2);
  });

  await test.step("001 step 7: both push", async () => {
    await push(alicePage, state.witnessId, { stored: 2, headSeq: 1 });
    await push(bobPage, state.witnessId, { stored: 2, headSeq: 1 });
  });

  return state;
}

/** `mabel identity export`, with the two lines story 001 step 5 quotes. */
function exportDescriptor(
  service: string,
  alias: string,
  identityId: string,
  outPath: string,
): void {
  const result = mabel(service, ["identity", "export", alias, "--out", outPath]);
  expect(result.status, result.stderr).toBe(0);
  const lines = stdoutLines(result);
  expect(lines[0]).toMatch(
    new RegExp(`^exported ${identityId} to ${outPath} \\(\\d+ bytes\\)$`),
  );
  expect(lines[1]).toBe("declared kind person, raw root, 0 witnesses");
}

export interface SharedLedgerState extends MeetState {
  orgId: string;
  invitationBundleBase64: string;
}

/**
 * Story 002 steps 1 to 8: the shared ledger founded, bob invited, his
 * acceptance admitted, the membership read back. Story 006 opens with it.
 */
export async function story002Steps1to8(
  alicePage: Page,
  bobPage: Page,
): Promise<SharedLedgerState> {
  const meet = await story001Steps1to7(alicePage, bobPage);
  const state: SharedLedgerState = { ...meet, orgId: "", invitationBundleBase64: "" };

  await test.step("002 step 2: alice founds the shared ledger", async () => {
    await alicePage.goto(`${ALICE_URL}/wallet`);
    const org = await createIdentity(alicePage, {
      alias: "mabel-demo-co",
      kind: "organization",
      founder: state.aliceId,
    });
    state.orgId = org.identityId;
  });

  await test.step("002 step 3: an identity root holds no key of its own", async () => {
    await openIdentity(alicePage, ALICE_URL, state.orgId);
    await expect(alicePage.getByTestId("identity-detail-declared-kind")).toHaveText("organization");
    await expect(alicePage.getByTestId("identity-detail-active-key")).toHaveText("null");
    await expect(alicePage.getByTestId("identity-detail-reserve-commit")).toHaveText("null");
  });

  await test.step("002 step 4: alice invites bob as a controller", async () => {
    const invite = mabel("alice", [
      "membership",
      "invite",
      "--ledger",
      "mabel-demo-co",
      "--by",
      "alice",
      "--invitee",
      "/tmp/bob.descriptor",
      "--role",
      "controller",
      "--out",
      "/tmp/invitation.bundle",
    ]);
    expect(invite.status, invite.stderr).toBe(0);
    const lines = stdoutLines(invite);
    expect(lines[0]).toBe(`invited ${state.bobId} as controller at seq 1 of ${state.orgId}`);
    expect(lines[1]).toMatch(/^wrote \/tmp\/invitation\.bundle \(2 events, \d+ bytes\)$/);
  });

  await test.step("002 step 5: the bundle travels to bob's machine", async () => {
    state.invitationBundleBase64 = readFileBase64("mabel-alice", "/tmp/invitation.bundle");
    expect(state.invitationBundleBase64.length).toBeGreaterThan(0);
  });

  let acceptanceBase64 = "";
  await test.step("002 step 6: bob's wallet folds the bundle and signs", async () => {
    const surface = await apiPost(
      BOB_URL,
      `/api/identities/${state.bobId}/memberships/acceptances`,
      { invitation_bundle_base64: state.invitationBundleBase64 },
    );
    expect(surface.status).toBe(200);
    expect(surface.body.ledger_id).toBe(state.orgId);
    expect(surface.body.declared_kind).toBe("organization");
    expect(surface.body.root).toBe("identity");
    expect(surface.body.controllers).toHaveLength(1);
    expect(surface.body.controllers[0].identity).toBe(state.aliceId);
    expect(surface.body.controllers[0].is_root).toBe(true);
    expect(surface.body.invitee).toBe(state.bobId);
    expect(surface.body.role).toBe("controller");
    expect(surface.body.controller_on_raw_root).toBe(false);
    expect(surface.body.warning).toBeNull();
    expect(typeof surface.body.acceptance_base64).toBe("string");
    expect(surface.body.acceptance_base64.length).toBeGreaterThan(0);
    acceptanceBase64 = surface.body.acceptance_base64;
  });

  await test.step("002 step 7: a controller admits the acceptance", async () => {
    writeFileBase64("mabel-alice", "/tmp/acceptance.file", acceptanceBase64);
    const admit = mabel("alice", [
      "membership",
      "admit",
      "--ledger",
      "mabel-demo-co",
      "--by",
      "alice",
      "/tmp/acceptance.file",
    ]);
    expect(admit.status, admit.stderr).toBe(0);
    expect(stdoutLines(admit)[0]).toBe(
      `admitted ${state.bobId} as controller at seq 2 of ${state.orgId}`,
    );
  });

  await test.step("002 step 8: the membership state reads back", async () => {
    const list = mabel("alice", ["membership", "list", "--ledger", "mabel-demo-co"]);
    expect(list.status, list.stderr).toBe(0);
    const lines = stdoutLines(list);
    expect(lines[0]).toBe(`${state.orgId}: 2 principals, 0 open invitations up to seq 2`);
    expect(lines.some((line) => new RegExp(`^controller ${state.aliceId} \\([a-z2-7]{52}\\) root$`).test(line.trim()))).toBe(
      true,
    );
    expect(lines.some((line) => new RegExp(`^controller ${state.bobId} \\([a-z2-7]{52}\\)$`).test(line.trim()))).toBe(
      true,
    );
    expect(
      lines.some((line) =>
        new RegExp(`^invitation .* offers controller to ${state.bobId}, accepted$`).test(
          line.trim(),
        ),
      ),
    ).toBe(true);

    const document = json(mabel("alice", ["membership", "list", "--ledger", "mabel-demo-co", "--json"]));
    expect(document.root).toBe("identity");
    const identities = document.principals.map((principal: any) => principal.identity);
    expect(identities).toEqual([...identities].sort());
    expect(document.invitations[0].status).toBe("accepted");
  });

  return state;
}

export interface ForkState {
  witnessId: string;
  witnessTwoId: string;
  witnessTwoTicket: string;
  aliceId: string;
  carolId: string;
  daveId: string;
  keptEvent: string;
  conflictingEvent: string;
}

/**
 * Story 004 steps 1 to 7: two witnesses, one home on two machines, one branch
 * to each witness and the second branch offered to witness one. Story 005
 * opens with it and tears down what it leaves running.
 */
export async function story004Steps1to7(): Promise<ForkState> {
  const state: ForkState = {
    witnessId: "",
    witnessTwoId: "",
    witnessTwoTicket: "",
    aliceId: "",
    carolId: "",
    daveId: "",
    keptEvent: "",
    conflictingEvent: "",
  };

  await test.step("004 step 1: the topology from nothing", async () => {
    resetTopology();
    state.witnessId = readWitnessId();
    expect(state.witnessId).toMatch(BASE32_ID);
  });

  await test.step("004 step 2: a second witness on the same bridge", async () => {
    startWitnessTwo();
    const { id, ticket } = await witnessTwoIdentity();
    state.witnessTwoId = id;
    state.witnessTwoTicket = ticket;
    expect(state.witnessTwoId).toMatch(BASE32_ID);
  });

  await test.step("004 step 3: one identity, two subjects, both witnesses", async () => {
    state.aliceId = createIdentityCli("alice", "alice");
    state.carolId = createIdentityCli("alice", "carol");
    state.daveId = createIdentityCli("alice", "dave");
    for (const endpoint of [state.witnessId, state.witnessTwoId]) {
      const added = mabel("alice", ["witness", "add", "--identity", "alice", "--endpoint", endpoint]);
      expect(added.status, added.stderr).toBe(0);
    }
    const pushed = dcSh(
      "alice",
      'mabel sync push --identity alice --peer "$(cat /shared/witness.ticket)" --peer "$(cat /shared/witness-two.ticket)"',
    );
    expect(pushed.status, pushed.stderr).toBe(0);
  });

  await test.step("004 step 4: alice's home on a second machine", async () => {
    startAliceTwo();
  });

  await test.step("004 step 5: both machines append at the same sequence", async () => {
    const kept = json(
      mabel("alice", [
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        state.carolId,
        "--no-sync",
        "--json",
      ]),
    );
    const conflicting = json(
      mustRun("docker", [
        "exec",
        "mabel-alice-two",
        "mabel",
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        state.daveId,
        "--no-sync",
        "--json",
      ]),
    );
    expect(kept.attestation_seq).toBe(3);
    expect(conflicting.attestation_seq).toBe(3);
    state.keptEvent = kept.attestation_event;
    state.conflictingEvent = conflicting.attestation_event;
    expect(state.keptEvent).not.toBe(state.conflictingEvent);
  });

  await test.step("004 step 6: one branch to each witness", async () => {
    const first = dcSh(
      "alice",
      `mabel sync push --identity alice --to ${state.witnessId} --peer "$(cat /shared/witness.ticket)"`,
    );
    expect(first.status, first.stderr).toBe(0);
    const second = mustRun("docker", [
      "exec",
      "mabel-alice-two",
      "sh",
      "-c",
      `mabel sync push --identity alice --to ${state.witnessTwoId} --peer "$(cat /shared/witness-two.ticket)"`,
    ]);
    expect(second.stdout).toContain("stored 1");
  });

  await test.step("004 step 7: the second branch reaches witness one", async () => {
    const pushed = mustRunAllowing(
      ["exec", "mabel-alice-two", "sh", "-c",
        `mabel sync push --identity alice --to ${state.witnessId} --peer "$(cat /shared/witness.ticket)" --json`],
      30,
    );
    const document = json(pushed);
    expect(document.ok).toBe(false);
    expect(document.code).toBe(30);
    expect(document.details.reason).toBe("all_witnesses_failed");
    expect(document.details.results[0].status).toBe("rejected");
    expect(document.details.results[0].reject_code).toBe("FORK");
    expect(document.details.results[0].at_seq).toBe(3);
  });

  return state;
}

/** `docker <args>` that must exit with one expected non-zero code. */
export function mustRunAllowing(args: string[], expectedStatus: number) {
  const result = mustRunOrStatus(args, expectedStatus);
  return result;
}

function mustRunOrStatus(args: string[], expectedStatus: number) {
  const { docker } = require("./docker") as typeof import("./docker");
  const result = docker(args);
  expect(result.status, `${result.command}\n${result.stdout}\n${result.stderr}`).toBe(
    expectedStatus,
  );
  return result;
}

export function createIdentityCli(service: string, alias: string, extra: string[] = []): string {
  const result = mabel(service, ["identity", "create", "--alias", alias, "--kind", "person", ...extra, "--json"]);
  expect(result.status, result.stderr).toBe(0);
  return json(result).identity.identity_id;
}

/** Story 004 step 2's `docker run` for witness two. */
export function startWitnessTwo(): void {
  mustRun("docker", [
    "run",
    "-d",
    "--name",
    "mabel-witness-two",
    "--network",
    "mabel_mabel",
    "--volume",
    "mabel_witness-ticket:/shared",
    "--env",
    "MABEL_ROLE=witness",
    "--env",
    "MABEL_RELAY=disabled",
    "--env",
    "MABEL_HTTP_BIND=0.0.0.0:9083",
    "--env",
    "MABEL_IROH_PORT=9073",
    "--env",
    "MABEL_PUBLISH_TICKET=/shared/witness-two",
    "--publish",
    "9083:9083",
    "--publish",
    "9073:9073/udp",
    "mabel:dev",
    "witness",
    "run",
    "--http",
    "0.0.0.0:9083",
    "--iroh-port",
    "9073",
  ]);
}

/** The two waits story 004 step 2 spells out, then the id and the ticket. */
export async function witnessTwoIdentity(): Promise<{ id: string; ticket: string }> {
  const { dcExec, until, waitForNode, WITNESS_TWO_URL } = require("./docker") as typeof import("./docker");
  await until("/shared/witness-two.ticket", () =>
    dcExec("alice", ["test", "-f", "/shared/witness-two.ticket"]).status === 0,
  );
  await waitForNode(WITNESS_TWO_URL);
  return {
    id: mustRun("docker", ["compose", "-f", composeFile(), "exec", "-T", "alice", "cat", "/shared/witness-two.id"]).stdout.trim(),
    ticket: mustRun("docker", ["compose", "-f", composeFile(), "exec", "-T", "alice", "cat", "/shared/witness-two.ticket"]).stdout.trim(),
  };
}

function composeFile(): string {
  const { COMPOSE_FILE } = require("./docker") as typeof import("./docker");
  return COMPOSE_FILE;
}

/**
 * Stories 004 step 4 and 006 step 3: alice's home copied to a second machine,
 * without node.json and node.key, then served on 9084.
 */
export function startAliceTwo(): void {
  mustRun("docker", ["volume", "create", "mabel-alice-second"]);
  mustRun("docker", [
    "run",
    "--rm",
    "--user",
    "0",
    "--volumes-from",
    "mabel-alice",
    "--volume",
    "mabel-alice-second:/copy",
    "--entrypoint",
    "sh",
    "mabel:dev",
    "-c",
    "cp -a /data/. /copy/ && rm -f /copy/node.json /copy/node.key",
  ]);
  mustRun("docker", [
    "run",
    "-d",
    "--name",
    "mabel-alice-two",
    "--network",
    "mabel_mabel",
    "--volume",
    "mabel-alice-second:/data",
    "--volume",
    "mabel_witness-ticket:/shared:ro",
    "--env",
    "MABEL_ROLE=wallet",
    "--env",
    "MABEL_RELAY=disabled",
    "--env",
    "MABEL_HTTP_BIND=0.0.0.0:9084",
    "--env",
    "MABEL_IROH_PORT=9074",
    "--publish",
    "9084:9084",
    "mabel:dev",
    "wallet",
    "serve",
    "--http",
    "0.0.0.0:9084",
    "--iroh-port",
    "9074",
  ]);
}

export { removeExtras, addTrust, identifier };

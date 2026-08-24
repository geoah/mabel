import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, seedIdentities } from "@/mocks/fixtures";

import { openAction, renderApp } from "./render";

/**
 * The membership flow ticket 019 specified, driven end to end against the mock
 * routes: Alice invites Acme onto her raw-rooted ledger, Acme's wallet reads
 * the bundle and signs an acceptance, Alice admits it, and Alice removes the
 * principal again.
 */

const acmeKey = seedIdentities.find((identity) => identity.identity_id === ACME)!.principals[0]
  .active_key;

/** What `mabel identity descriptor` writes: the identity and the key it signs under. */
const DESCRIPTOR = btoa(JSON.stringify({ identity: ACME, active_key: acmeKey }));

function fill(testId: string, value: string): void {
  fireEvent.change(screen.getByTestId(testId), { target: { value } });
}

function artifact(testId: string): string {
  return (screen.getByTestId(testId) as HTMLTextAreaElement).value;
}

async function open(route: string, ...actions: string[]) {
  const rendered = renderApp(route);
  await screen.findByTestId("identity-actions");
  for (const action of actions) {
    await openAction(rendered.user, action);
  }
  return rendered;
}

async function invite(): Promise<string> {
  const { user } = await open(`/identities/${ALICE}`, "action-invite");
  fill("invite-descriptor", DESCRIPTOR);
  await user.click(screen.getByTestId("invite-submit"));

  await screen.findByTestId("invite-result");
  expect(screen.getByTestId("invite-result-invitee")).toHaveTextContent(ACME);
  expect(screen.getByTestId("invite-result-role")).toHaveTextContent("controller");
  return artifact("invite-bundle");
}

async function accept(bundle: string): Promise<string> {
  const { user } = await open(`/identities/${ACME}`, "action-accept");
  fill("accept-bundle", bundle);
  await user.click(screen.getByTestId("accept-submit"));

  await screen.findByTestId("accept-result");
  // A controller role on a raw-rooted ledger means signing as that ledger's own
  // identity, and the acceptance stays hidden until that sentence is read.
  expect(screen.getByTestId("accept-warning")).toHaveTextContent(`signing as ${ALICE}`);
  expect(screen.queryByTestId("accept-acceptance")).not.toBeInTheDocument();

  await user.click(screen.getByTestId("accept-acknowledge"));
  return artifact("accept-acceptance");
}

describe("membership", () => {
  it("invites, accepts, admits and removes over the ticket 021 routes", async () => {
    const bundle = await invite();
    expect(bundle.length).toBeGreaterThan(0);
    // The invitation is on the ledger and open, before anybody accepts it.
    expect(screen.getByTestId(`invitation-status-${ACME}`)).toHaveTextContent("open");
    expect(screen.getByTestId("identity-detail-open-invitations")).toHaveTextContent(
      "1 invitation to help control this identity, still waiting for an answer",
    );
    cleanup();

    const acceptance = await accept(bundle);
    expect(acceptance.length).toBeGreaterThan(0);
    cleanup();

    const { user } = await open(`/identities/${ALICE}`, "action-admit", "action-remove");
    fill("admit-acceptance", acceptance);
    await user.click(screen.getByTestId("admit-submit"));

    await screen.findByTestId("admit-result");
    expect(screen.getByTestId("admit-result-invitee")).toHaveTextContent(ACME);
    await waitFor(() =>
      expect(screen.getByTestId(`principal-role-${ACME}`)).toHaveTextContent("controller"),
    );
    expect(screen.getByTestId(`invitation-status-${ACME}`)).toHaveTextContent("accepted");
    expect(screen.getByTestId("identity-detail-open-invitations")).toHaveTextContent("none");

    fireEvent.change(screen.getByTestId("remove-target"), { target: { value: ACME } });
    await user.click(screen.getByTestId("remove-submit"));

    await screen.findByTestId("remove-result");
    expect(screen.getByTestId("remove-result-principal")).toHaveTextContent("yes");
    await waitFor(() =>
      expect(screen.queryByTestId(`principal-row-${ACME}`)).not.toBeInTheDocument(),
    );
  });

  it("refuses an acceptance that was already admitted, as a replay", async () => {
    const bundle = await invite();
    cleanup();
    const acceptance = await accept(bundle);
    cleanup();

    const { user } = await open(`/identities/${ALICE}`, "action-admit");
    fill("admit-acceptance", acceptance);
    await user.click(screen.getByTestId("admit-submit"));
    await screen.findByTestId("admit-result");

    await user.click(screen.getByTestId("admit-submit"));

    const envelope = await screen.findByTestId("admit-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 50");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "acceptance_already_used",
    );
  });

  it("refuses a bundle addressed to another identity", async () => {
    const bundle = await invite();
    cleanup();

    // The bundle invites Acme; reading it in Alice's own wallet is not the
    // invitee, which the node answers before it signs anything.
    const { user } = await open(`/identities/${ALICE}`, "action-accept");
    fill("accept-bundle", bundle);
    await user.click(screen.getByTestId("accept-submit"));

    const envelope = await screen.findByTestId("accept-error");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("not_the_invitee");
    expect(screen.queryByTestId("accept-result")).not.toBeInTheDocument();
  });

  it("refuses a second open invitation for one invitee", async () => {
    await invite();

    fill("invite-descriptor", DESCRIPTOR);
    await screen.findByTestId("invite-submit");
    fireEvent.click(screen.getByTestId("invite-submit"));

    const envelope = await screen.findByTestId("invite-error");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "duplicate_invitation",
    );
  });
});

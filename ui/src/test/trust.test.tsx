import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

const DETAIL = `/identities/${ALICE}`;

async function openDetail() {
  const rendered = renderApp(DETAIL);
  await screen.findByTestId("identity-detail");
  await openAction(rendered.user, "action-trust");
  return rendered;
}

/** The event id of the last append, which the line carries as an identifier. */
async function appendedEvent(): Promise<string> {
  const line = await screen.findByTestId("trust-appended-event");
  return line.querySelector("[data-value]")?.getAttribute("data-value") ?? "";
}

describe("saying you trust someone", () => {
  it("posts issuer and subject and draws the subject as a card", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/trust")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));

    await appendedEvent();
    expect(bodies).toEqual([{ issuer: ALICE, subject: ACME }]);

    // Every name this identity trusts is a full identity card now, the same one
    // every other list of identities draws.
    const list = await screen.findByTestId("trust-list");
    const card = within(list).getByTestId(`identity-card-${ACME}`);
    expect(within(card).getByTestId(`identity-card-link-${ACME}`)).toHaveAttribute(
      "href",
      `/identities/${ACME}`,
    );
  });

  it("renders the code 20 policy envelope on a duplicate unrevoked attestation", async () => {
    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));
    await screen.findByTestId("trust-appended-event");

    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));

    const envelope = await screen.findByTestId("trust-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 20");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "duplicate_unrevoked_attestation",
    );
    expect(within(envelope).getByTestId("error-message")).toHaveTextContent("Policy error:");
  });

  it("keeps the subject in the box when the append is refused", async () => {
    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));
    await screen.findByTestId("trust-appended-event");
    expect(screen.getByTestId("trust-add-subject")).toHaveValue("");

    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));

    await screen.findByTestId("trust-error");
    // Retrying is the same action run again, not the same id typed again.
    expect(screen.getByTestId("trust-add-subject")).toHaveValue(ACME);
  });

  it("renders the code 10 schema envelope when the subject is the issuer", async () => {
    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ALICE);
    await user.click(screen.getByTestId("trust-add-submit"));

    const envelope = await screen.findByTestId("trust-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 10");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "subject_equals_ledger",
    );
    expect(within(envelope).getByTestId("error-message")).toHaveTextContent("Schema error:");
  });
});

describe("taking trust back", () => {
  it("finds the standing entry for the id it is given and revokes that one", async () => {
    const revoked: string[] = [];
    server.events.on("request:start", ({ request }) => {
      const path = new URL(request.url).pathname;
      if (request.method === "POST" && path.endsWith("/revoke")) {
        revoked.push(path);
      }
    });

    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));
    const eventId = await appendedEvent();
    await screen.findByTestId(`identity-card-${ACME}`);

    await openAction(user, "action-revoke");
    await user.type(screen.getByTestId("trust-revoke-subject"), ACME);
    await user.click(screen.getByTestId("trust-revoke-submit"));

    // The form was given an identity id and revoked the entry that said it.
    await waitFor(() => expect(revoked).toEqual([`/api/trust/${eventId}/revoke`]));
    // Trust taken back is not on the screen at all any more.
    await waitFor(() =>
      expect(screen.queryByTestId(`identity-card-${ACME}`)).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("trust-revoke-subject")).toHaveValue("");
  });

  it("refuses an id this identity does not trust, without asking the node", async () => {
    const posted: string[] = [];
    server.events.on("request:start", ({ request }) => {
      if (request.method === "POST") {
        posted.push(new URL(request.url).pathname);
      }
    });

    const { user } = renderApp(DETAIL);
    await screen.findByTestId("identity-detail");
    await openAction(user, "action-revoke");

    await user.type(screen.getByTestId("trust-revoke-subject"), ACME);
    await user.click(screen.getByTestId("trust-revoke-submit"));

    expect(screen.getByTestId("trust-revoke-none")).toHaveTextContent(
      "This identity does not trust that id right now, so there is nothing to take back.",
    );
    expect(posted).toEqual([]);
    // The id stays in the box: it is the same action, run again.
    expect(screen.getByTestId("trust-revoke-subject")).toHaveValue(ACME);
  });

  it("draws no take-it-back button beside a name in the list", async () => {
    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));
    const eventId = await appendedEvent();

    const list = await screen.findByTestId("trust-list");
    expect(within(list).queryByTestId(`trust-revoke-${eventId}`)).not.toBeInTheDocument();
    expect(screen.queryByTestId(`trust-row-${eventId}`)).not.toBeInTheDocument();
    expect(screen.queryByTestId(`trust-state-${eventId}`)).not.toBeInTheDocument();
  });
});

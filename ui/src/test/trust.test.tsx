import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

const DETAIL = `/wallet/identities/${ALICE}`;

async function openDetail() {
  const rendered = renderApp(DETAIL);
  await screen.findByTestId("identity-detail");
  return rendered;
}

describe("trust add and revoke", () => {
  it("posts issuer and subject and lists the new attestation", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/trust")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));

    const appended = await screen.findByTestId("trust-appended-event");
    const eventId = appended.textContent ?? "";
    expect(bodies).toEqual([{ issuer: ALICE, subject: ACME }]);

    const row = await screen.findByTestId(`trust-row-${eventId}`);
    expect(within(row).getByTestId(`trust-state-${eventId}`)).toHaveTextContent("unrevoked");
  });

  it("revokes the attestation it just appended", async () => {
    const { user } = await openDetail();
    await user.type(screen.getByTestId("trust-add-subject"), ACME);
    await user.click(screen.getByTestId("trust-add-submit"));
    const eventId = (await screen.findByTestId("trust-appended-event")).textContent ?? "";

    await user.click(await screen.findByTestId(`trust-revoke-${eventId}`));

    await waitFor(() =>
      expect(screen.getByTestId(`trust-state-${eventId}`)).toHaveTextContent(
        /revoked at seq \d+/,
      ),
    );
    expect(screen.getByTestId(`trust-revoke-${eventId}`)).toBeDisabled();
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

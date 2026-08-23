import { screen, within } from "@testing-library/react";
import { http } from "msw";
import { describe, expect, it } from "vitest";

import { ApiError } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { ACME, ALICE, cliErrorCases, errors } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp, renderComponent } from "./render";

describe("error envelope", () => {
  // contracts/cli/errors.json holds one case per exit code and layer prefix, and
  // the same envelope crosses HTTP.
  it.each(cliErrorCases.map((entry) => [entry.case, entry] as const))(
    "renders the %s case",
    (_name, entry) => {
      renderComponent(
        <ErrorEnvelopeView error={new ApiError(entry.document, 400)} />,
      );
      const envelope = screen.getByTestId("error-envelope");
      expect(within(envelope).getByTestId("error-code")).toHaveTextContent(
        `code ${entry.exit_code}`,
      );
      expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
        entry.document.details.reason,
      );
      expect(within(envelope).getByTestId("error-message")).toHaveTextContent(
        entry.document.message,
      );
      for (const key of Object.keys(entry.document.details)) {
        if (key !== "reason") {
          expect(within(envelope).getByTestId(`error-detail-${key}`)).toBeInTheDocument();
        }
      }
    },
  );

  it("renders the code 30 envelope when a push reaches no witness", async () => {
    // acme carries an empty witness set in the fixtures.
    const { user } = renderApp(`/wallet/identities/${ACME}`);
    await screen.findByTestId("identity-detail");

    await user.click(screen.getByTestId("sync-push-submit"));

    const envelope = await screen.findByTestId("sync-push-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 30");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "all_witnesses_failed",
    );
    expect(within(envelope).getByTestId("error-status")).toHaveTextContent("status 502");
  });

  it("renders the code 50 envelope when a witness reports a later head", async () => {
    server.use(
      http.post("/api/identities/:identityId/witnesses", () =>
        Response.json(errors.staleHead.body, { status: errors.staleHead.status }),
      ),
    );

    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");
    await user.type(screen.getByTestId("witness-add-endpoint"), "a".repeat(52));
    await user.click(screen.getByTestId("witness-add-submit"));

    const envelope = await screen.findByTestId("witness-add-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 50");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("stale_head");
    expect(within(envelope).getByTestId("error-detail-observed_head_seq")).toHaveTextContent("4");
  });

  it("renders the code 60 envelope when a key file has insecure permissions", async () => {
    server.use(
      http.get("/api/identities", () =>
        Response.json(errors.insecureKeyPermissions.body, {
          status: errors.insecureKeyPermissions.status,
        }),
      ),
    );

    renderApp("/wallet");

    const envelope = await screen.findByTestId("identity-list-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 60");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "insecure_key_permissions",
    );
    expect(within(envelope).getByTestId("error-detail-mode")).toHaveTextContent("0644");
  });
});

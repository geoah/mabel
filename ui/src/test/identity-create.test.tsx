import { screen, waitFor, within } from "@testing-library/react";
import { http } from "msw";
import { describe, expect, it } from "vitest";

import { errors } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

describe("identity create", () => {
  it("posts alias and declared_kind and lists the new identity", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/identities")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-list");

    await user.type(screen.getByTestId("identity-create-alias"), "carol");
    await user.selectOptions(screen.getByTestId("identity-create-declared-kind"), "organization");
    await user.click(screen.getByTestId("identity-create-submit"));

    const created = await screen.findByTestId("identity-create-result-identity-id");
    const identityId = created.textContent ?? "";
    expect(identityId).toHaveLength(52);
    expect(bodies).toEqual([{ alias: "carol", declared_kind: "organization" }]);

    const row = await screen.findByTestId(`identity-row-${identityId}`);
    expect(within(row).getByText("carol")).toBeInTheDocument();
    expect(screen.getByTestId(`identity-declared-kind-${identityId}`)).toHaveTextContent(
      "organization",
    );
  });

  it("sends founder only when one is given, for an identity-rooted ledger", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/identities")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-list");
    const founder = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";

    await user.type(screen.getByTestId("identity-create-alias"), "acme two");
    await user.selectOptions(screen.getByTestId("identity-create-declared-kind"), "organization");
    await user.type(screen.getByTestId("identity-create-founder"), founder);
    await user.click(screen.getByTestId("identity-create-submit"));

    await screen.findByTestId("identity-create-result-identity-id");
    expect(bodies).toEqual([
      { alias: "acme two", declared_kind: "organization", founder },
    ]);
  });

  it("renders the code 2 envelope when the node rejects a missing alias", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-list");

    await user.click(screen.getByTestId("identity-create-submit"));

    const envelope = await screen.findByTestId("identity-create-error");
    expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 2");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("missing_field");
    expect(within(envelope).getByTestId("error-message")).toHaveTextContent(
      errors.missingField.body.message,
    );
    expect(within(envelope).getByTestId("error-detail-field")).toHaveTextContent("alias");
  });

  it("renders the code 70 envelope when the node cannot mint the declared kind", async () => {
    server.use(
      http.post("/api/identities", () =>
        Response.json(errors.unsupportedDeclaredKind.body, {
          status: errors.unsupportedDeclaredKind.status,
        }),
      ),
    );

    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-list");
    await user.type(screen.getByTestId("identity-create-alias"), "robot");
    await user.selectOptions(screen.getByTestId("identity-create-declared-kind"), "agent");
    await user.click(screen.getByTestId("identity-create-submit"));

    const envelope = await screen.findByTestId("identity-create-error");
    await waitFor(() =>
      expect(within(envelope).getByTestId("error-code")).toHaveTextContent("code 70"),
    );
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent(
      "unsupported_declared_kind",
    );
    expect(within(envelope).getByTestId("error-status")).toHaveTextContent("status 501");
  });
});

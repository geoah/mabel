import { screen, waitFor, within } from "@testing-library/react";
import { http } from "msw";
import { describe, expect, it } from "vitest";

import { errors } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

/** The create form is folded away on the wallet page; every test opens it. */
async function openCreateForm() {
  const rendered = renderApp("/wallet");
  await screen.findByTestId("identity-cards");
  await rendered.user.click(screen.getByTestId("identity-create-summary"));
  return rendered;
}

describe("identity create", () => {
  it("posts alias and declared_kind and lists the new identity", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/identities")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = await openCreateForm();

    await user.type(screen.getByTestId("identity-create-alias"), "carol");
    await user.selectOptions(screen.getByTestId("identity-create-declared-kind"), "organization");
    await user.click(screen.getByTestId("identity-create-submit"));

    const created = await screen.findByTestId("identity-create-result-identity-id");
    const identityId = created.textContent ?? "";
    expect(identityId).toHaveLength(52);
    expect(bodies).toEqual([{ alias: "carol", declared_kind: "organization" }]);

    const card = await screen.findByTestId(`identity-card-${identityId}`);
    expect(within(card).getByText("carol")).toBeInTheDocument();
    expect(
      within(card).getByTestId(`identity-card-declared-kind-${identityId}`),
    ).toHaveTextContent("organization");
  });

  it("sends founder only when one is given, for an identity-rooted ledger", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/identities")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = await openCreateForm();
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

  it("names the alias as the private nickname, and the other two as public", async () => {
    await openCreateForm();

    expect(screen.getByLabelText("Private nickname (only this device sees it)")).toBe(
      screen.getByTestId("identity-create-alias"),
    );
    expect(screen.getByLabelText("Public name (optional)")).toBe(
      screen.getByTestId("identity-create-display-name"),
    );
    expect(screen.getByLabelText("Public email (optional)")).toBe(
      screen.getByTestId("identity-create-email"),
    );
    // The private nickname comes first, then the two public fields, then the
    // kind and the founder (proposal 005).
    const order = [
      "identity-create-alias",
      "identity-create-display-name",
      "identity-create-email",
      "identity-create-declared-kind",
      "identity-create-founder",
    ].map((testId) => screen.getByTestId(testId));
    for (let index = 1; index < order.length; index += 1) {
      expect(
        order[index - 1].compareDocumentPosition(order[index]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
  });

  it("publishes a public name and email at creation, and shows them on the new card", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/identities")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = await openCreateForm();

    await user.type(screen.getByTestId("identity-create-alias"), "dana");
    await user.type(screen.getByTestId("identity-create-display-name"), "Dana Dane");
    await user.type(screen.getByTestId("identity-create-email"), "dana@dana.example");
    await user.click(screen.getByTestId("identity-create-submit"));

    const created = await screen.findByTestId("identity-create-result-identity-id");
    const identityId = created.textContent ?? "";
    expect(bodies).toEqual([
      {
        alias: "dana",
        declared_kind: "person",
        display_name: "Dana Dane",
        email: "dana@dana.example",
      },
    ]);
    expect(screen.getByTestId("identity-create-result-email")).toHaveTextContent(
      "dana@dana.example",
    );

    // The public fields become one entry on the new record, right after the one
    // that created it, so the card names them straight away.
    const card = await screen.findByTestId(`identity-card-${identityId}`);
    expect(within(card).getByTestId(`identity-card-name-${identityId}-name`)).toHaveTextContent(
      "Dana Dane",
    );
    expect(within(card).getByTestId(`identity-card-email-${identityId}`)).toHaveTextContent(
      "dana@dana.example",
    );
    expect(card).not.toHaveTextContent("at position");
  });

  it("sends neither public field when both boxes are left empty", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/api/identities")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = await openCreateForm();

    await user.type(screen.getByTestId("identity-create-alias"), "quiet");
    await user.click(screen.getByTestId("identity-create-submit"));

    const identityId = (await screen.findByTestId("identity-create-result-identity-id")).textContent;
    expect(bodies).toEqual([{ alias: "quiet", declared_kind: "person" }]);
    expect(screen.queryByTestId("identity-create-result-email")).not.toBeInTheDocument();
    expect(screen.queryByTestId(`identity-card-email-${identityId}`)).not.toBeInTheDocument();
  });

  it("refuses an email with no at sign, and mints nothing", async () => {
    const { user } = await openCreateForm();

    await user.type(screen.getByTestId("identity-create-alias"), "typo");
    await user.type(screen.getByTestId("identity-create-email"), "dana.example");
    await user.click(screen.getByTestId("identity-create-submit"));

    const envelope = await screen.findByTestId("identity-create-error");
    expect(within(envelope).getByTestId("error-reason")).toHaveTextContent("invalid_email");
    expect(screen.queryByTestId("identity-create-result")).not.toBeInTheDocument();
  });

  it("renders the code 2 envelope when the node rejects a missing alias", async () => {
    const { user } = await openCreateForm();

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

    const { user } = await openCreateForm();
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

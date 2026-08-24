import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ALICE } from "@/mocks/fixtures";

import { openAction, renderApp } from "./render";

/**
 * A form that is one box and one button is one row: the box grows into the space
 * that is left and the button sits beside it. It wraps only when the box cannot
 * keep its own minimum width, which the narrowest phone column does not force.
 */
function assertOneRow(formTestId: string, inputTestId: string, submitTestId: string): void {
  const form = screen.getByTestId(formTestId);
  const input = screen.getByTestId(inputTestId);
  const submit = screen.getByTestId(submitTestId);

  expect(form.className).toContain("flex");
  expect(form.className).toContain("items-end");
  // The button is the form's own child, beside the field, not under it.
  expect(submit.parentElement).toBe(form);
  const field = input.parentElement!;
  expect(field.parentElement).toBe(form);
  expect(field.className).toContain("flex-1");
  expect(field.className).toContain("min-w-36");
}

describe("one box and one button", () => {
  it("puts the search box and its button on one row", async () => {
    renderApp("/wallet");
    await screen.findByTestId("identity-cards");

    assertOneRow("wallet-search-form", "wallet-search-input", "wallet-search-submit");
  });

  it.each([
    ["action-trust", "trust-add-form", "trust-add-subject", "trust-add-submit"],
    ["action-revoke", "trust-revoke-form", "trust-revoke-subject", "trust-revoke-submit"],
    ["action-witnesses", "witness-add-form", "witness-add-endpoint", "witness-add-submit"],
    ["action-push", "sync-push-form", "sync-push-to", "sync-push-submit"],
    ["action-handle", "handle-form", "handle-input", "handle-submit"],
  ])("puts %s on one row", async (action, form, input, submit) => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, action);

    assertOneRow(form, input, submit);
  });
});

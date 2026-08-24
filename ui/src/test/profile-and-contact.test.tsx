import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

describe("profile replacement", () => {
  it("shows the before-and-after diff and waits for a confirmation", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-profile");

    await user.clear(screen.getByTestId("profile-display-name"));
    await user.type(screen.getByTestId("profile-display-name"), "Alice A.");
    await user.click(screen.getByTestId("profile-replace-submit"));

    expect(screen.getByTestId("profile-diff-display-name-before")).toHaveTextContent(
      "Alice Ashworth",
    );
    expect(screen.getByTestId("profile-diff-display-name-after")).toHaveTextContent("Alice A.");
    // The handle has its own action, so this form neither shows nor changes it.
    expect(screen.queryByTestId("profile-diff-hostname")).not.toBeInTheDocument();
    expect(screen.queryByTestId("profile-hostname")).not.toBeInTheDocument();
    // Nothing is signed before the confirmation.
    expect(screen.getByTestId("profile-current-display-name")).toHaveTextContent(
      "Alice Ashworth",
    );

    await user.click(screen.getByTestId("profile-replace-confirm"));

    await waitFor(() =>
      expect(screen.getByTestId("profile-current-display-name")).toHaveTextContent("Alice A."),
    );
    expect(screen.getByTestId("profile-replace-result")).toHaveTextContent("Saved at position 9.");
    expect(screen.getByTestId("identity-detail-resolved-name")).toHaveTextContent("Alice A.");
  });

  it("keeps the handle this identity publishes while it replaces the name", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/profile")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-profile");

    await user.clear(screen.getByTestId("profile-display-name"));
    await user.type(screen.getByTestId("profile-display-name"), "Alice A.");
    await user.click(screen.getByTestId("profile-replace-submit"));
    await user.click(screen.getByTestId("profile-replace-confirm"));

    // The route replaces the whole profile, so the handle travels with it.
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toEqual({
      display_name: "Alice A.",
      hostname: "alice.example",
      email: "alice@alice.example",
    });
  });

  it("drops the diff and changes nothing when the confirmation is cancelled", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-profile");

    await user.clear(screen.getByTestId("profile-display-name"));
    await user.type(screen.getByTestId("profile-display-name"), "Alice A.");
    await user.click(screen.getByTestId("profile-replace-submit"));
    await user.click(screen.getByTestId("profile-replace-cancel"));

    expect(screen.queryByTestId("profile-diff")).not.toBeInTheDocument();
    expect(screen.getByTestId("profile-current-display-name")).toHaveTextContent(
      "Alice Ashworth",
    );
  });

  it("refuses a replacement that would change nothing", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-profile");

    await user.click(screen.getByTestId("profile-replace-submit"));
    await user.click(screen.getByTestId("profile-replace-confirm"));

    expect(await screen.findByTestId("profile-error")).toBeInTheDocument();
    expect(screen.getByTestId("error-reason")).toHaveTextContent("no_op_profile_update");
    expect(screen.getByTestId("error-status")).toHaveTextContent("status 409");
  });
});

describe("contact", () => {
  it("writes the nickname and the note together, from one button", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-contact");

    expect(screen.getByTestId("identity-detail-contact")).toHaveTextContent("none");
    // Both fields carry a short label with the sentence behind an info icon.
    expect(screen.getByTestId("contact-nickname-info")).toHaveAttribute(
      "aria-label",
      "Your local nickname for this identity. Only this device sees it.",
    );
    expect(screen.getByTestId("contact-note-info")).toHaveAttribute(
      "aria-label",
      "A private note about this identity. Only this device sees it.",
    );

    await user.type(screen.getByTestId("contact-nickname"), "alice at the co-op");
    await user.type(screen.getByTestId("contact-note"), "keys live on the blue laptop");
    await user.click(screen.getByTestId("contact-save"));

    expect(await screen.findByTestId("contact-result")).toHaveTextContent("Saved ");
    // The nickname row is the name the card falls back to, the note is its own
    // row directly under it.
    await waitFor(() =>
      expect(screen.getByTestId("identity-detail-alias")).toHaveTextContent("alice at the co-op"),
    );
    expect(screen.getByTestId("identity-detail-contact")).toHaveTextContent(
      "keys live on the blue laptop",
    );
  });

  it("reports a nickname past its byte cap", async () => {
    const { user } = renderApp(`/identities/${ACME}`);
    await openAction(user, "action-contact");

    await user.type(screen.getByTestId("contact-nickname"), "n".repeat(65));
    await user.click(screen.getByTestId("contact-save"));

    expect(await screen.findByTestId("contact-error")).toBeInTheDocument();
    expect(screen.getByTestId("error-reason")).toHaveTextContent("contact_field_too_long");
  });
});

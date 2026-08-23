import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { HOSTNAME_CONSENT_KEY } from "@/lib/preferences";
import { ACME, ALICE } from "@/mocks/fixtures";

import { renderApp } from "./render";

describe("profile replacement", () => {
  it("shows the before-and-after diff and waits for a confirmation", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("profile-panel");

    await user.clear(screen.getByTestId("profile-display-name"));
    await user.type(screen.getByTestId("profile-display-name"), "Alice A.");
    await user.click(screen.getByTestId("profile-replace-submit"));

    expect(screen.getByTestId("profile-diff-display-name-before")).toHaveTextContent(
      "Alice Ashworth",
    );
    expect(screen.getByTestId("profile-diff-display-name-after")).toHaveTextContent("Alice A.");
    expect(screen.getByTestId("profile-diff-hostname-after")).toHaveTextContent("alice.example");
    // Nothing is signed before the confirmation.
    expect(screen.getByTestId("profile-current-display-name")).toHaveTextContent(
      "Alice Ashworth",
    );

    await user.click(screen.getByTestId("profile-replace-confirm"));

    await waitFor(() =>
      expect(screen.getByTestId("profile-current-display-name")).toHaveTextContent("Alice A."),
    );
    expect(screen.getByTestId("profile-replace-result")).toHaveTextContent("replaced at seq 9");
    expect(screen.getByTestId("identity-detail-resolved-name")).toHaveTextContent("Alice A.");
  });

  it("drops the diff and changes nothing when the confirmation is cancelled", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("profile-panel");

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
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("profile-panel");

    await user.click(screen.getByTestId("profile-replace-submit"));
    await user.click(screen.getByTestId("profile-replace-confirm"));

    expect(await screen.findByTestId("profile-error")).toBeInTheDocument();
    expect(screen.getByTestId("error-reason")).toHaveTextContent("no_op_profile_update");
    expect(screen.getByTestId("error-status")).toHaveTextContent("status 409");
  });

  it("states what a hostname publishes, once, before the first one", async () => {
    const { user } = renderApp(`/wallet/identities/${ACME}`);
    await screen.findByTestId("profile-panel");

    await user.type(screen.getByTestId("profile-hostname"), "acme.example");
    await user.click(screen.getByTestId("profile-replace-submit"));

    expect(screen.getByTestId("profile-hostname-consent")).toHaveTextContent(
      "stays readable forever",
    );
    expect(screen.getByTestId("profile-replace-confirm")).toHaveTextContent("Publish and replace");
    expect(globalThis.localStorage.getItem(HOSTNAME_CONSENT_KEY)).toBeNull();

    await user.click(screen.getByTestId("profile-replace-confirm"));

    await waitFor(() =>
      expect(globalThis.localStorage.getItem(HOSTNAME_CONSENT_KEY)).toBe("1"),
    );
    // A claim this node has not checked reads unverified, never a plain check.
    await waitFor(() =>
      expect(screen.getByTestId("verification-mark")).toHaveAttribute(
        "data-verification",
        "unverified",
      ),
    );
  });

  it("asks for the consent only once per node home", async () => {
    globalThis.localStorage.setItem(HOSTNAME_CONSENT_KEY, "1");
    const { user } = renderApp(`/wallet/identities/${ACME}`);
    await screen.findByTestId("profile-panel");

    await user.type(screen.getByTestId("profile-hostname"), "acme.example");
    await user.click(screen.getByTestId("profile-replace-submit"));

    expect(screen.queryByTestId("profile-hostname-consent")).not.toBeInTheDocument();
    expect(screen.getByTestId("profile-replace-confirm")).toHaveTextContent("Confirm");
  });
});

describe("verification", () => {
  it("forces a check and reports the fresh verdict", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("verification-panel");

    expect(screen.getByTestId("verification-mark")).toHaveAttribute(
      "data-verification",
      "verified",
    );

    await user.click(screen.getByTestId("verification-check"));

    await waitFor(() =>
      expect(screen.getByTestId("verification-mark")).toHaveTextContent("alice.example"),
    );
    expect(screen.getByTestId("verification-note")).toHaveTextContent("advisory");
  });

  it("refuses a check on an identity that claims no hostname", async () => {
    const { user } = renderApp(`/wallet/identities/${ACME}`);
    await screen.findByTestId("verification-panel");

    expect(screen.getByTestId("verification-status")).toHaveTextContent("unclaimed");

    await user.click(screen.getByTestId("verification-check"));

    expect(await screen.findByTestId("verification-error")).toBeInTheDocument();
    expect(screen.getByTestId("error-reason")).toHaveTextContent("no_hostname_claimed");
  });
});

describe("contact", () => {
  it("round-trips the private note through the contact route", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("contact-panel");

    expect(screen.getByTestId("identity-detail-contact")).toHaveTextContent("none");

    await user.type(screen.getByTestId("contact-nickname"), "alice at the co-op");
    await user.type(screen.getByTestId("contact-note"), "keys live on the blue laptop");
    await user.click(screen.getByTestId("contact-save"));

    expect(await screen.findByTestId("contact-result")).toHaveTextContent("saved at");
    await waitFor(() =>
      expect(screen.getByTestId("identity-detail-contact")).toHaveTextContent(
        "alice at the co-op: keys live on the blue laptop",
      ),
    );
  });

  it("reports a nickname past its byte cap", async () => {
    const { user } = renderApp(`/wallet/identities/${ALICE}`);
    await screen.findByTestId("contact-panel");

    await user.type(screen.getByTestId("contact-nickname"), "n".repeat(65));
    await user.click(screen.getByTestId("contact-save"));

    expect(await screen.findByTestId("contact-error")).toBeInTheDocument();
    expect(screen.getByTestId("error-reason")).toHaveTextContent("contact_field_too_long");
  });
});

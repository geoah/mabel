import { screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { HOSTNAME_CONSENT_KEY } from "@/lib/preferences";
import { ACME, ALICE, seedIdentities } from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

/**
 * The handle is the name a person types instead of a 52-character id. Setting
 * one replaces the profile, and it only works once DNS names this identity back,
 * so the action shows the line to add and checks it.
 */
describe("the handle action", () => {
  it("shows the TXT line for the handle this identity publishes", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-handle");

    expect(screen.getByTestId("handle-current")).toHaveTextContent("alice.example");
    expect(screen.getByTestId("handle-panel")).toHaveTextContent(
      "Add this line to the DNS records of your handle",
    );
    const record = screen.getByTestId("handle-panel").querySelector("[data-value]");
    expect(record?.getAttribute("data-value")).toBe(
      `_mabel.alice.example. IN TXT "mabel=${ALICE}"`,
    );
  });

  it("names the handle in the box, so the line follows what you are about to set", async () => {
    const { user } = renderApp(`/identities/${ACME}`);
    await openAction(user, "action-handle");

    expect(screen.getByTestId("handle-current")).toHaveTextContent("none");
    expect(screen.getByTestId("handle-panel")).toHaveTextContent(
      "Set a handle to see the line your DNS records need.",
    );

    await user.type(screen.getByTestId("handle-input"), "acme.example");

    const record = screen.getByTestId("handle-panel").querySelector("[data-value]");
    expect(record?.getAttribute("data-value")).toBe(
      `_mabel.acme.example. IN TXT "mabel=${ACME}"`,
    );
  });

  it("states what publishing a handle makes public, once, before the first one", async () => {
    const bodies: unknown[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/profile")) {
        bodies.push(await request.clone().json());
      }
    });

    const { user } = renderApp(`/identities/${ACME}`);
    await openAction(user, "action-handle");

    await user.type(screen.getByTestId("handle-input"), "acme.example");
    await user.click(screen.getByTestId("handle-submit"));

    expect(screen.getByTestId("handle-consent")).toHaveTextContent("stays readable forever");
    expect(globalThis.localStorage.getItem(HOSTNAME_CONSENT_KEY)).toBeNull();
    expect(bodies).toEqual([]);

    await user.click(screen.getByTestId("handle-consent-confirm"));

    await waitFor(() => expect(globalThis.localStorage.getItem(HOSTNAME_CONSENT_KEY)).toBe("1"));
    // The public name and the email of the identity travel with the handle.
    expect(bodies).toEqual([
      { display_name: "Acme Corporation", hostname: "acme.example", email: null },
    ]);
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
    const { user } = renderApp(`/identities/${ACME}`);
    await openAction(user, "action-handle");

    await user.type(screen.getByTestId("handle-input"), "acme.example");
    await user.click(screen.getByTestId("handle-submit"));

    expect(screen.queryByTestId("handle-consent")).not.toBeInTheDocument();
    expect(await screen.findByTestId("handle-result")).toHaveTextContent("Saved at position");
  });

  it("checks the handle now and reports the fresh verdict", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-handle");

    expect(screen.getByTestId("verification-mark")).toHaveAttribute(
      "data-verification",
      "verified",
    );

    await user.click(screen.getByTestId("verification-check"));

    await waitFor(() =>
      expect(screen.getByTestId("verification-mark")).toHaveTextContent("alice.example"),
    );
  });

  // The node keeps a failed re-check beside the last decisive verdict, and a
  // verdict whose latest check failed is not a clean mark.
  it("says the last check failed, with the time of it, over a verified handle", async () => {
    const verifiedAt = 1_700_000_790_000;
    const failedAt = verifiedAt + 90_000;
    server.use(
      http.get(`/api/identities/${ALICE}`, () =>
        HttpResponse.json({
          ok: true,
          identity: {
            ...alice,
            verification: {
              hostname: "alice.example",
              status: "verified",
              checked_at_ms: failedAt,
              last_verified_at_ms: verifiedAt,
              stale: false,
              detail: null,
              unreachable: { checked_at_ms: failedAt, detail: "no answer from the resolver" },
            },
          },
        }),
      ),
    );

    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-handle");

    const mark = screen.getByTestId("verification-mark");
    expect(mark).toHaveAttribute("data-verification", "recheck-failed");
    expect(mark).toHaveTextContent("last check failed");
    expect(screen.getByTestId("verification-unreachable")).toHaveTextContent("last check failed");
    expect(screen.getByTestId("verification-unreachable-detail")).toHaveTextContent(
      "no answer from the resolver",
    );
    // Both times are on the page: when it last matched, and when the latest
    // check failed.
    expect(screen.getByTestId("verification-last-verified-at-ms")).toBeInTheDocument();
    expect(screen.getByTestId("verification-unreachable-checked-at-ms")).toBeInTheDocument();
  });

  it("refuses a check on an identity that claims no handle", async () => {
    const { user } = renderApp(`/identities/${ACME}`);
    await openAction(user, "action-handle");

    expect(screen.getByTestId("verification-status")).toHaveTextContent(
      "this identity claims no handle",
    );

    await user.click(screen.getByTestId("verification-check"));

    expect(await screen.findByTestId("verification-error")).toBeInTheDocument();
    expect(screen.getByTestId("error-reason")).toHaveTextContent("no_hostname_claimed");
  });

  it("says handle everywhere it used to say website", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.getByTestId("identity-detail-hostname-row")).toHaveTextContent("handle");
    expect(screen.getByTestId("action-handle-summary")).toHaveTextContent(
      "Set the handle people can look you up by",
    );
    await openAction(user, "action-handle");
    const words = document.body.textContent ?? "";
    expect(words).not.toMatch(/website/i);
  });
});

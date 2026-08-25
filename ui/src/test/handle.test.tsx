import { screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { HOSTNAME_CONSENT_KEY } from "@/lib/preferences";
import { ACME, ALICE, HINTED_MACHINE, seedIdentities, WITNESS_MACHINE } from "@/mocks/fixtures";
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

  // The identity a fixture ships advertises no endpoint, so the second line has
  // nothing to name and the screen says so with one line and the singular
  // wording.
  it("shows the handle line alone when this identity advertises no endpoint", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-handle");

    expect(screen.getByTestId("handle-panel")).toHaveTextContent(
      "Add this line to the DNS records of your handle, then check it here.",
    );
    expect(screen.getByTestId("handle-txt-record")).toBeInTheDocument();
    expect(screen.queryByTestId("handle-txt-endpoints-record")).not.toBeInTheDocument();
  });

  // The second line of proposal 006 section 6, on the screen that asks for the
  // first one. The ids inside a record value stay bare: what goes in a zone file
  // is the DNS grammar, and `mabel-endpoints=` is defined over bare ids.
  it("shows the endpoints line beside the handle line once this identity advertises endpoints", async () => {
    server.use(
      http.get(`/api/identities/${ALICE}`, () =>
        HttpResponse.json({
          ok: true,
          identity: { ...alice, endpoints: [WITNESS_MACHINE, HINTED_MACHINE] },
        }),
      ),
    );

    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-handle");

    const endpointsLine = await screen.findByTestId("handle-txt-endpoints-record");
    expect(screen.getByTestId("handle-panel")).toHaveTextContent(
      "Add these two lines to the DNS records of your handle, then check them here.",
    );

    // Both lines, in the order a zone file wants them, each one whole.
    expect(
      screen.getByTestId("handle-txt-record").querySelector("[data-value]"),
    ).toHaveAttribute("data-value", `_mabel.alice.example. IN TXT "mabel=${ALICE}"`);
    expect(endpointsLine.querySelector("[data-value]")).toHaveAttribute(
      "data-value",
      `_mabel.alice.example. IN TXT "mabel-endpoints=${WITNESS_MACHINE},${HINTED_MACHINE}"`,
    );
    // Comma joined, no space, and no prefix on an id inside the record value.
    expect(endpointsLine.textContent).toContain(`${WITNESS_MACHINE},${HINTED_MACHINE}`);
    expect(endpointsLine.textContent).not.toContain("mabel://");

    // A sentence under each line saying what that line does, and a copy control
    // on each: a person adding these to a zone copies them one at a time.
    expect(screen.getByTestId("handle-panel")).toHaveTextContent(
      "This line says the handle belongs to this identity.",
    );
    expect(screen.getByTestId("handle-panel")).toHaveTextContent(
      "This line names the endpoints that answer for it, so someone who has the handle can reach it.",
    );
    expect(screen.getByLabelText("Copy the handle line")).toBeInTheDocument();
    expect(screen.getByLabelText("Copy the endpoints line")).toBeInTheDocument();
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
    // A claim this node has not checked reads unchecked, never a plain check
    // and never the word for a lookup that found nothing (issue 042).
    await waitFor(() =>
      expect(screen.getByTestId("verification-mark")).toHaveAttribute(
        "data-verification",
        "unchecked",
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

  // A handle nobody looked up and a handle whose DNS names nobody are two
  // different things to tell a reader, and the node spells them with two
  // statuses (issue 042).
  it("says a handle has not been checked yet, and offers the check", async () => {
    server.use(
      http.get(`/api/identities/${ALICE}`, () =>
        HttpResponse.json({
          ok: true,
          identity: {
            ...alice,
            verification: {
              hostname: "alice.example",
              status: "unchecked",
              checked_at_ms: null,
              last_verified_at_ms: null,
              stale: false,
              detail: "alice.example has not been checked on this node",
              unreachable: null,
            },
          },
        }),
      ),
    );

    const { user } = renderApp(`/identities/${ALICE}`);
    await openAction(user, "action-handle");

    const mark = screen.getByTestId("verification-mark");
    expect(mark).toHaveAttribute("data-verification", "unchecked");
    expect(mark).toHaveTextContent("alice.example");
    expect(mark).toHaveAttribute(
      "title",
      "unchecked: alice.example has not been checked from this wallet yet",
    );
    expect(screen.getByTestId("verification-unchecked")).toHaveTextContent(
      "This handle has not been checked from this wallet yet.",
    );
    // The one control that runs a check is right there, and nothing ran one on
    // its own to get here.
    expect(screen.getByTestId("verification-check")).toBeEnabled();

    await user.click(screen.getByTestId("verification-check"));

    await waitFor(() =>
      expect(screen.getByTestId("verification-mark")).toHaveAttribute(
        "data-verification",
        "verified",
      ),
    );
    expect(screen.queryByTestId("verification-unchecked")).not.toBeInTheDocument();
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

import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ENDPOINTS_CONSENT_KEY } from "@/lib/preferences";
import {
  ALICE,
  HINTED_MACHINE,
  UNSTORED_LEDGER,
  WITNESS_MACHINE,
  seedIdentities,
} from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;

describe("sharing an identity", () => {
  it("shows the link as text, as a square and as a file, and says what it gives away", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-share");

    // Alice's record names no machine yet, so the link is the Mabel ID alone.
    const link = `mabel://${ALICE}`;
    const panel = screen.getByTestId("share-panel");
    // The string itself, whole, with a control that copies it.
    expect(panel).toHaveTextContent(link);
    expect(within(panel).getByLabelText("Copy the link")).toBeInTheDocument();
    // The same string as a square, so a phone across the table can read it.
    expect(screen.getByTestId("share-qr")).toHaveAttribute("data-value", link);
    // And as a file holding one line.
    const download = screen.getByTestId("share-download");
    expect(download).toHaveAttribute("download", `${ALICE.slice(0, 8)}.mabel`);
    expect(download.getAttribute("href")).toBe(
      `data:text/plain;charset=utf-8,${encodeURIComponent(`${link}\n`)}`,
    );

    const disclosure = screen.getByTestId("share-disclosure");
    expect(disclosure).toHaveTextContent("Mabel ID");
    expect(disclosure).toHaveTextContent("the machines that answer for this identity");
    expect(disclosure).toHaveTextContent("this home's network address");
    expect(screen.getByTestId("share-machine-count")).toHaveTextContent(
      "No machine answers for this identity yet",
    );
  });

  it("carries the machines the record names once it names one", async () => {
    const machine = "d".repeat(52);
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-endpoints");
    await user.type(screen.getByTestId("endpoints-input"), machine);
    await user.click(screen.getByTestId("endpoints-submit"));
    await user.click(screen.getByTestId("endpoints-consent-confirm"));
    await screen.findByTestId("endpoints-head-seq");

    await openAction(user, "action-share");

    const link = `mabel://${ALICE}?endpoints=${machine}`;
    await waitFor(() =>
      expect(screen.getByTestId("share-qr")).toHaveAttribute("data-value", link),
    );
    expect(screen.getByTestId("share-machine-count")).toHaveTextContent("names 1 machine");
  });
});

describe("publishing the machines that answer for an identity", () => {
  it("asks for consent once, states the three facts, then publishes", async () => {
    const machine = "b".repeat(52);
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-endpoints");

    await user.type(screen.getByTestId("endpoints-input"), machine);
    await user.click(screen.getByTestId("endpoints-submit"));

    const consent = screen.getByTestId("endpoints-consent");
    expect(consent).toHaveTextContent("stays readable forever");
    expect(consent).toHaveTextContent("can dial that machine directly");
    expect(consent).toHaveTextContent("list the identities it signs for");
    expect(globalThis.localStorage.getItem(ENDPOINTS_CONSENT_KEY)).toBeNull();

    await user.click(screen.getByTestId("endpoints-consent-confirm"));

    await waitFor(() =>
      expect(globalThis.localStorage.getItem(ENDPOINTS_CONSENT_KEY)).toBe("1"),
    );
    expect(screen.queryByTestId("endpoints-consent")).not.toBeInTheDocument();
    expect(await screen.findByTestId("endpoints-head-seq")).toHaveTextContent(
      `Saved at position ${alice.head_seq + 1}.`,
    );
    // The machines row on the card above says where the new claim came from.
    expect(
      await screen.findByTestId(`identity-detail-machine-${machine}-note`),
    ).toHaveTextContent("This machine is listed on this identity's own record.");
  });

  it("publishes nothing when the consent is declined", async () => {
    const posted: string[] = [];
    server.events.on("request:start", ({ request }) => {
      if (request.method === "POST") {
        posted.push(new URL(request.url).pathname);
      }
    });
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-endpoints");

    await user.type(screen.getByTestId("endpoints-input"), "c".repeat(52));
    await user.click(screen.getByTestId("endpoints-submit"));
    await user.click(screen.getByTestId("endpoints-consent-cancel"));

    expect(screen.queryByTestId("endpoints-consent")).not.toBeInTheDocument();
    expect(globalThis.localStorage.getItem(ENDPOINTS_CONSENT_KEY)).toBeNull();
    expect(posted).toEqual([]);
  });

  it("says so rather than sending a machine the record already names", async () => {
    const machine = "e".repeat(52);
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-endpoints");
    await user.type(screen.getByTestId("endpoints-input"), machine);
    await user.click(screen.getByTestId("endpoints-submit"));
    await user.click(screen.getByTestId("endpoints-consent-confirm"));
    await screen.findByTestId("endpoints-head-seq");

    await user.type(screen.getByTestId("endpoints-input"), machine);
    await user.click(screen.getByTestId("endpoints-submit"));

    expect(screen.getByTestId("endpoints-duplicate")).toHaveTextContent(
      "already on this identity's record",
    );
    expect(screen.queryByTestId("endpoints-consent")).not.toBeInTheDocument();
  });
});

describe("a link pasted into the search box", () => {
  it("is asked about the node, then opens the identity with the machines it named", async () => {
    const asked: string[] = [];
    server.events.on("request:start", ({ request }) => {
      const url = new URL(request.url);
      if (url.pathname === "/api/resolve") {
        asked.push(url.searchParams.get("input") ?? "");
      }
    });
    const link = `mabel://${UNSTORED_LEDGER}?endpoints=${WITNESS_MACHINE},${HINTED_MACHINE}`;
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-input");

    expect(screen.getByTestId("wallet-search-form")).toHaveTextContent(
      "Mabel ID, handle or link",
    );

    await user.type(screen.getByTestId("wallet-search-input"), link);
    await user.click(screen.getByTestId("wallet-search-submit"));

    // The browser parsed nothing: the node was asked what the string is.
    expect(asked).toEqual([link]);
    // The page states what using the link does before anything is fetched.
    expect(await screen.findByTestId("identity-fetch-link-note")).toHaveTextContent(
      "tells those machines this home's network address",
    );
  });

  it("dials the machines the link named, in order, when the fetch runs", async () => {
    const sources: (string | null)[] = [];
    server.events.on("request:start", async ({ request }) => {
      if (request.method === "POST" && request.url.endsWith("/fetch")) {
        sources.push(((await request.clone().json()) as { from: string | null }).from);
      }
    });
    const link = `mabel://${UNSTORED_LEDGER}?endpoints=${WITNESS_MACHINE}`;
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-input");

    await user.type(screen.getByTestId("wallet-search-input"), link);
    await user.click(screen.getByTestId("wallet-search-submit"));
    await screen.findByTestId("identity-fetch-button");
    await user.click(screen.getByTestId("identity-fetch-button"));

    await screen.findByTestId("ledger-events");
    expect(sources).toEqual([WITNESS_MACHINE]);
  });

  it("names no machine when a bare Mabel ID is pasted", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("wallet-search-input");

    await user.type(screen.getByTestId("wallet-search-input"), UNSTORED_LEDGER);
    await user.click(screen.getByTestId("wallet-search-submit"));

    await screen.findByTestId("identity-fetch-button");
    expect(screen.queryByTestId("identity-fetch-link-note")).not.toBeInTheDocument();
  });
});

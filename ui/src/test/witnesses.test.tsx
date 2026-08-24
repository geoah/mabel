import { screen, within } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import {
  ACME,
  ALICE,
  BOB,
  HINTED_MACHINE,
  REACHABLE_WITNESS,
  UNREACHABLE_WITNESS,
  WITNESS_MACHINE,
} from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { openAction, renderApp } from "./render";

describe("the witness list", () => {
  it("draws each witness as the identity card every other screen draws", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    const card = screen.getByTestId(`identity-card-${REACHABLE_WITNESS}`);
    // The name its profile publishes, and its Mabel ID under it: a witness is
    // an identity, so it is named like one.
    expect(card).toHaveTextContent("the co-op witness");
    expect(card).toHaveTextContent(REACHABLE_WITNESS);
    expect(
      within(card).getByTestId(`witness-default-${REACHABLE_WITNESS}`),
    ).toHaveTextContent("this node uses it by default");
    // Its card opens its own page, which is the identity page.
    expect(screen.getByTestId(`identity-card-link-${REACHABLE_WITNESS}`)).toHaveAttribute(
      "href",
      `/identities/${REACHABLE_WITNESS}`,
    );
  });

  it("draws no node-default marker on a witness only a ledger names", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    expect(screen.getByTestId(`identity-card-${UNREACHABLE_WITNESS}`)).toBeInTheDocument();
    expect(
      screen.queryByTestId(`witness-default-${UNREACHABLE_WITNESS}`),
    ).not.toBeInTheDocument();
  });

  it("says the list is empty rather than drawing nothing", async () => {
    server.use(http.get("/api/witnesses", () => HttpResponse.json({ ok: true, witnesses: [] })));
    renderApp("/witnesses");

    expect(await screen.findByTestId("witness-list-empty")).toHaveTextContent(
      "Your wallet knows of no witness yet.",
    );
  });

  it("names no protocol vocabulary anywhere on the list", async () => {
    renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    const page = screen.getByTestId("witness-list").textContent ?? "";
    expect(page).not.toMatch(/binding|verified|hinted|endpoint/i);
    expect(page).not.toMatch(/·/);
  });
});

describe("a witness's own page", () => {
  it("lists what it holds, as identity cards, under its own heading", async () => {
    renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("witness-holdings");

    await screen.findByTestId("identity-cards");
    const card = screen.getByTestId(`identity-card-${ALICE}`);
    expect(within(card).getByTestId(`identity-card-declared-kind-${ALICE}`)).toHaveTextContent(
      "person",
    );
    // What a witness holds is how much of the record it has, not a position.
    expect(within(card).getByTestId(`identity-card-entries-${ALICE}`)).toHaveTextContent(
      "4 entries",
    );
    expect(screen.getByTestId(`identity-card-link-${ALICE}`)).toHaveAttribute(
      "href",
      `/identities/${ALICE}`,
    );
  });

  it("carries the facts its card used to, as rows", async () => {
    renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("witness-holdings");

    expect(screen.getByTestId("witness-chosen-by")).toHaveTextContent("1 of your identities");
    expect(screen.getByTestId("witness-node-default")).toHaveTextContent("yes");
  });

  it("names the ledger with conflicting entries the witness kept", async () => {
    renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-cards");

    expect(screen.getByTestId(`identity-card-fork-count-${ALICE}`)).toHaveTextContent(
      "1 conflict",
    );
    expect(screen.queryByTestId(`identity-card-fork-count-${ACME}`)).not.toBeInTheDocument();
  });

  // The route pages, and this section has no page control: it reads the pages.
  it("reads past the first page of what a witness holds", async () => {
    const second = "d".repeat(52);
    server.use(
      http.get("/api/witnesses/:identityId/holdings", ({ params, request }) => {
        const offset = Number(new URL(request.url).searchParams.get("offset") ?? "0");
        const ledger = (ledgerId: string) => ({
          ledger_id: ledgerId,
          declared_kind: "person",
          head_seq: 0,
          head_event: "e".repeat(52),
          event_count: 1,
          fork_count: 0,
        });
        return HttpResponse.json({
          ok: true,
          identity_id: String(params.identityId),
          endpoint_id: WITNESS_MACHINE,
          offset,
          limit: 1,
          more: offset === 0,
          ledgers: [ledger(offset === 0 ? ALICE : second)],
        });
      }),
    );

    renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-cards");

    expect(await screen.findByTestId(`identity-card-${second}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.queryByTestId("witness-holdings-capped")).not.toBeInTheDocument();
  });

  it("narrows what a witness holds to yours and to the ones you trust", async () => {
    const { user } = renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-cards");

    // Everything it holds, which is what the page opens on.
    expect(screen.getByTestId("witness-holdings-all")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`identity-card-${BOB}`)).toBeInTheDocument();

    await user.click(screen.getByTestId("witness-holdings-ours"));

    // The ledgers this wallet controls, and nothing else.
    expect(screen.getByTestId(`identity-card-${ALICE}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`identity-card-${BOB}`)).not.toBeInTheDocument();

    await user.click(screen.getByTestId("witness-holdings-trusted"));

    // The people one of your identities has vouched for, and nothing you own.
    expect(screen.getByTestId(`identity-card-${BOB}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`identity-card-${ALICE}`)).not.toBeInTheDocument();
  });

  it("states a witness no machine answers for as a fact about the connection", async () => {
    renderApp(`/identities/${UNREACHABLE_WITNESS}`);

    const panel = await screen.findByTestId("witness-unreachable");
    expect(panel).toHaveTextContent("could not reach this witness");
    expect(panel).toHaveTextContent("not about the records it keeps");
    expect(screen.getByTestId("witness-unreachable-message")).toHaveTextContent("Network error:");
    expect(screen.queryByTestId("identity-cards")).not.toBeInTheDocument();
  });

  it("draws rows and no actions for a witness this home cannot sign for", async () => {
    renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-detail");

    // The machines row is there; the buttons that would change the record are
    // not, because this identity is not in the signing set.
    expect(screen.getByTestId(`identity-detail-machine-${WITNESS_MACHINE}`)).toBeInTheDocument();
    expect(screen.queryByTestId("identity-actions")).not.toBeInTheDocument();
    expect(screen.queryByTestId("action-share")).not.toBeInTheDocument();
    expect(screen.queryByTestId("action-endpoints")).not.toBeInTheDocument();
  });

  it("draws no holdings section on an identity that is no witness of ours", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-detail");

    expect(screen.queryByTestId("witness-holdings")).not.toBeInTheDocument();
  });

  it("fetches nothing into this home while browsing a witness", async () => {
    const methods: string[] = [];
    server.events.on("request:start", ({ request }) => methods.push(request.method));

    const { user } = renderApp("/witnesses");
    await screen.findByTestId("witness-cards");

    await user.click(screen.getByTestId(`identity-card-link-${REACHABLE_WITNESS}`));
    await screen.findByTestId("identity-cards");

    expect(methods.length).toBeGreaterThan(0);
    expect(methods.filter((method) => method !== "GET")).toEqual([]);
  });
});

describe("naming a witness", () => {
  it("says plainly that a machine id is not an identity id", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-witnesses");

    await user.type(screen.getByTestId("witness-add-identity"), WITNESS_MACHINE);
    await user.click(screen.getByTestId("witness-add-submit"));

    expect(await screen.findByTestId("witness-add-refused")).toHaveTextContent(
      "That is the id of a machine, not of an identity.",
    );
  });

  it("says plainly that no machine answers for the identity that was named", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-witnesses");

    await user.type(screen.getByTestId("witness-add-identity"), "q".repeat(52));
    await user.click(screen.getByTestId("witness-add-submit"));

    expect(await screen.findByTestId("witness-add-refused")).toHaveTextContent(
      "found no machine that answers for that identity",
    );
  });

  it("names the witnesses it already has as identities, linked to their pages", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");
    await openAction(user, "action-witnesses");

    const row = screen.getByTestId(`witness-row-${REACHABLE_WITNESS}`);
    expect(row).toHaveTextContent("the co-op witness");
    expect(
      within(row).getByTestId(`witness-row-${REACHABLE_WITNESS}-link`),
    ).toHaveAttribute("href", `/identities/${REACHABLE_WITNESS}`);
  });
});

describe("the machines that answer for an identity", () => {
  it("says of each machine where the claim it answers came from", async () => {
    renderApp(`/identities/${REACHABLE_WITNESS}`);
    await screen.findByTestId("identity-detail");

    // The machine the witness's own record lists.
    const own = await screen.findByTestId(`identity-detail-machine-${WITNESS_MACHINE}`);
    expect(own).toHaveTextContent(WITNESS_MACHINE);
    expect(
      screen.getByTestId(`identity-detail-machine-${WITNESS_MACHINE}-note`),
    ).toHaveTextContent("This machine is listed on this identity's own record.");

    // The machine this home only knows from somewhere else.
    const hinted = screen.getByTestId(`identity-detail-machine-${HINTED_MACHINE}`);
    expect(hinted).toHaveTextContent(HINTED_MACHINE);
    expect(
      screen.getByTestId(`identity-detail-machine-${HINTED_MACHINE}-note`),
    ).toHaveTextContent("No record we have confirms that this machine answers for it.");

    // The row is labelled, and the id carries no separators and no status.
    expect(screen.getByTestId(`identity-detail-machine-${WITNESS_MACHINE}-row`)).toHaveTextContent(
      "machine",
    );
    const page = screen.getByTestId("identity-detail").textContent ?? "";
    expect(page).not.toMatch(/binding|hinted|verified|endpoint/i);
    expect(page).not.toMatch(/·/);
  });

  it("draws no machine row for an identity nothing answers for", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("identity-detail");

    expect(screen.queryByTestId(/^identity-detail-machine-/)).not.toBeInTheDocument();
  });
});

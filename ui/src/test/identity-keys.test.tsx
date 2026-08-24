import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACME, ALICE, identityKeys } from "@/mocks/fixtures";

import { renderApp } from "./render";

/**
 * Decision 017: creating an identity offers the person their two secret keys to
 * save, in plain words about what each one is and what losing them means. The
 * same panel is an action on every identity page the wallet can sign for.
 */
describe("save your keys", () => {
  it("offers both secrets, a download and a copy control after a create", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");
    await user.click(screen.getByTestId("identity-create-summary"));

    await user.type(screen.getByTestId("identity-create-alias"), "dana");
    await user.click(screen.getByTestId("identity-create-submit"));

    const result = await screen.findByTestId("identity-create-result");
    expect(result).toHaveTextContent("Save your keys");

    const panel = await screen.findByTestId("identity-keys");
    await waitFor(() =>
      expect(within(panel).getByTestId("identity-keys-active")).toHaveValue(
        identityKeys.active_secret_key,
      ),
    );
    expect(within(panel).getByTestId("identity-keys-reserve")).toHaveValue(
      identityKeys.reserve_secret_key,
    );
    // One plain sentence each, saying what the key is for.
    expect(panel).toHaveTextContent("The key that signs today");
    expect(panel).toHaveTextContent("The key you will need if you ever replace it");
    expect(within(panel).getByTestId("identity-keys-active-copy")).toBeInTheDocument();
    expect(within(panel).getByTestId("identity-keys-reserve-copy")).toBeInTheDocument();
  });

  it("downloads a file naming the identity and both keys", async () => {
    const { user } = renderApp("/wallet");
    await screen.findByTestId("identity-cards");
    await user.click(screen.getByTestId("identity-create-summary"));

    await user.type(screen.getByTestId("identity-create-alias"), "dana");
    await user.click(screen.getByTestId("identity-create-submit"));

    const created = await screen.findByTestId("identity-create-result-identity-id");
    const identityId = created.querySelector("[data-value]")?.getAttribute("data-value") ?? "";
    const link = await screen.findByTestId("identity-keys-download");

    expect(link).toHaveAttribute("download", `mabel-keys-${identityId.slice(0, 8)}.txt`);
    const file = decodeURIComponent(
      (link.getAttribute("href") ?? "").replace("data:text/plain;charset=utf-8,", ""),
    );
    expect(file).toContain(identityId);
    expect(file).toContain(identityKeys.active_secret_key);
    expect(file).toContain(identityKeys.reserve_secret_key);
    expect(file).toContain("Losing both");
  });

  it("warns what the keys are worth and that the wallet keeps its own copy", async () => {
    renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");

    const warning = await screen.findByTestId("identity-keys-warning");
    expect(warning).toHaveTextContent("Anyone who has these two keys controls this identity");
    expect(warning).toHaveTextContent("losing both loses it");
    expect(warning).toHaveTextContent("This wallet keeps its own copy on this computer");
  });

  it("collapses the action on an owned identity page and opens on a click", async () => {
    const { user } = renderApp(`/identities/${ALICE}`);
    await screen.findByTestId("identity-actions");

    expect(screen.getByTestId("action-keys")).not.toHaveAttribute("open");

    await user.click(screen.getByTestId("action-keys-summary"));

    expect(screen.getByTestId("action-keys")).toHaveAttribute("open");
    expect(screen.getByTestId("identity-keys-active")).toBeVisible();
  });

  it("says in words that an identity-rooted ledger holds no keys to hand back", async () => {
    renderApp(`/identities/${ACME}`);
    await screen.findByTestId("identity-actions");

    expect(await screen.findByTestId("identity-keys-none")).toHaveTextContent(
      "This identity holds no key of its own. Its controllers sign for it",
    );
    // A 409 the screen can explain is not an error envelope.
    expect(screen.queryByTestId("identity-keys-error")).not.toBeInTheDocument();
    expect(screen.queryByTestId("identity-keys-active")).not.toBeInTheDocument();
  });
});

import { screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Identifier } from "@/components/Identifier";
import { COPY_FAILED, copyText } from "@/lib/clipboard";
import { ALICE } from "@/mocks/fixtures";
import { KeysPanel } from "@/routes/wallet/KeysPanel";

import { renderComponent } from "./render";

/**
 * navigator.clipboard exists only in a secure context, and the node serves this
 * page over plain http as often as not. userEvent installs a clipboard stub, so
 * a test of the other path makes that stub refuse.
 */
function clipboardRefuses(): void {
  vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("not allowed"));
}

/** The pre-clipboard-API copy, which jsdom does not implement by itself. */
function withExecCommand(result: boolean): ReturnType<typeof vi.fn> {
  const execCommand = vi.fn(() => result);
  Object.defineProperty(document, "execCommand", {
    value: execCommand,
    configurable: true,
    writable: true,
  });
  return execCommand;
}

function withoutExecCommand(): void {
  Reflect.deleteProperty(document, "execCommand");
}

afterEach(() => {
  vi.restoreAllMocks();
  withoutExecCommand();
});

describe("copyText", () => {
  it("uses the clipboard API where the browser offers one", async () => {
    const { user } = renderComponent(<p>a page with a clipboard</p>);
    void user;
    const execCommand = withExecCommand(true);

    expect(await copyText("copy me")).toBe(true);
    expect(await navigator.clipboard.readText()).toBe("copy me");
    expect(execCommand).not.toHaveBeenCalled();
  });

  it("falls back to the legacy copy when the clipboard is unavailable", async () => {
    renderComponent(<p>a page on plain http</p>);
    clipboardRefuses();
    const execCommand = withExecCommand(true);

    expect(await copyText("copy me")).toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
    // The textarea the fallback selects is gone again.
    expect(document.querySelectorAll("textarea")).toHaveLength(0);
  });

  it("reports failure when neither way is available", async () => {
    renderComponent(<p>a page with nothing</p>);
    clipboardRefuses();
    withoutExecCommand();

    expect(await copyText("copy me")).toBe(false);
  });
});

describe("a copy that could not happen", () => {
  it("tells the reader to select the identifier instead", async () => {
    const { user } = renderComponent(
      <MemoryRouter>
        <Identifier value={ALICE} />
      </MemoryRouter>,
    );
    clipboardRefuses();
    withoutExecCommand();

    await user.click(screen.getByRole("button", { name: "copy" }));

    expect(await screen.findByTestId("copy-failed")).toHaveTextContent(COPY_FAILED);
    expect(screen.getByRole("button", { name: COPY_FAILED })).toHaveAttribute(
      "data-copy-failed",
      "true",
    );
  });

  it("says so beside a secret key, which is the one nobody can retype", async () => {
    const { user } = renderComponent(<KeysPanel identityId={ALICE} />);
    await screen.findByTestId("identity-keys-active");
    clipboardRefuses();
    withoutExecCommand();

    await user.click(screen.getByTestId("identity-keys-active-copy"));

    expect(await screen.findByTestId("identity-keys-active-copy-failed")).toHaveTextContent(
      COPY_FAILED,
    );
  });
});

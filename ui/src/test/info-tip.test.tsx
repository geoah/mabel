import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { InfoTip, NICKNAME_INFO } from "@/components/InfoTip";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";

import { renderComponent } from "./render";

/**
 * The info icon a short label carries instead of a long one. A phone has no
 * hover, so the sentence has to survive a tap: a tap fires pointerenter, focus
 * and click in that order, and any of the three toggling would shut it again.
 */
describe("the info tip", () => {
  it("shows the sentence on a tap, and keeps it while the trigger holds focus", async () => {
    const { user } = renderComponent(<InfoTip text={NICKNAME_INFO} testId="tip" />);

    const trigger = screen.getByTestId("tip");
    expect(trigger).toHaveAttribute("aria-label", NICKNAME_INFO);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("tip-text")).not.toBeInTheDocument();

    await user.click(trigger);

    const text = screen.getByTestId("tip-text");
    expect(text).toHaveTextContent(NICKNAME_INFO);
    expect(text).toHaveAttribute("role", "tooltip");
    // The sentence is wired to the trigger, so a screen reader reads the pair.
    expect(trigger).toHaveAttribute("aria-describedby", text.id);
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    await user.tab();

    expect(screen.queryByTestId("tip-text")).not.toBeInTheDocument();
  });

  it("hides the sentence again on Escape", async () => {
    const { user } = renderComponent(<InfoTip text="a note" testId="tip" />);

    await user.click(screen.getByTestId("tip"));
    expect(screen.getByTestId("tip-text")).toBeInTheDocument();

    await user.keyboard("{Escape}");

    expect(screen.queryByTestId("tip-text")).not.toBeInTheDocument();
  });

  it("hangs off a key-value label without lengthening it", async () => {
    const { user } = renderComponent(
      <KeyValueTable>
        <KeyValue label="Nickname" testId="alias" info={NICKNAME_INFO}>
          alice
        </KeyValue>
      </KeyValueTable>,
    );

    const row = screen.getByTestId("alias-row");
    expect(row.querySelector("dt")?.textContent).toBe("Nickname");

    await user.click(screen.getByTestId("alias-info"));

    expect(screen.getByTestId("alias-info-text")).toHaveTextContent(NICKNAME_INFO);
  });
});

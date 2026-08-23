import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { KeyValue, KeyValueTable } from "@/components/KeyValue";

import { renderComponent } from "./render";

describe("KeyValue", () => {
  it("keeps the key and the value on one line, in one row", () => {
    renderComponent(
      <KeyValueTable data-testid="table">
        <KeyValue label="head_seq" testId="head-seq">
          8
        </KeyValue>
        <KeyValue label="event_count" testId="event-count">
          9
        </KeyValue>
      </KeyValueTable>,
    );

    const row = screen.getByTestId("head-seq-row");
    const label = within(row).getByText("head_seq");
    const value = screen.getByTestId("head-seq");

    expect(label.tagName).toBe("DT");
    expect(value.tagName).toBe("DD");
    // The pair shares one row element and that row is a single flex line, which
    // is what decision 014 asks for: never stacked label over value.
    expect(label.parentElement).toBe(row);
    expect(value.parentElement).toBe(row);
    expect(row.className).toMatch(/\bflex\b/);
    expect(row.className).not.toMatch(/\bgrid\b/);
    expect(value).toHaveTextContent("8");
  });

  it("puts each pair in its own row of the table", () => {
    renderComponent(
      <KeyValueTable data-testid="table">
        <KeyValue label="head_seq" testId="head-seq">
          8
        </KeyValue>
        <KeyValue label="event_count" testId="event-count">
          9
        </KeyValue>
      </KeyValueTable>,
    );

    const table = screen.getByTestId("table");
    expect(table.tagName).toBe("DL");
    expect(screen.getByTestId("head-seq-row")).not.toBe(screen.getByTestId("event-count-row"));
    expect(table).toContainElement(screen.getByTestId("event-count-row"));
  });
});

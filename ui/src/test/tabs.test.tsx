import { screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

import { renderComponent } from "./render";

/**
 * The tabs one section puts over a list. Everything asserted here is what a
 * reader on a keyboard or a screen reader gets, because that is the part a
 * hand-vendored component loses first.
 */

/** Three tabs, uncontrolled, which is the shape a caller writes by hand. */
function ThreeTabs() {
  return (
    <Tabs defaultValue="all">
      <TabsList data-testid="row">
        <TabsTrigger value="all" data-testid="tab-all">
          All
        </TabsTrigger>
        <TabsTrigger value="trusted" data-testid="tab-trusted">
          Trusted
        </TabsTrigger>
        <TabsTrigger value="yours" data-testid="tab-yours">
          Yours
        </TabsTrigger>
      </TabsList>
      <TabsContent value="all" data-testid="panel-all">
        every record
      </TabsContent>
      <TabsContent value="trusted" data-testid="panel-trusted">
        the trusted records
      </TabsContent>
      <TabsContent value="yours" data-testid="panel-yours">
        your records
      </TabsContent>
    </Tabs>
  );
}

describe("the tabs", () => {
  it("names the row, the tabs and the panel the way a screen reader reads them", () => {
    renderComponent(<ThreeTabs />);

    expect(screen.getByTestId("row")).toHaveAttribute("role", "tablist");
    const all = screen.getByTestId("tab-all");
    const panel = screen.getByTestId("panel-all");

    expect(all).toHaveAttribute("role", "tab");
    expect(all).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("tab-trusted")).toHaveAttribute("aria-selected", "false");
    expect(panel).toHaveAttribute("role", "tabpanel");
    // The tab points at the panel it shows, and the panel is named by the tab.
    expect(all.getAttribute("aria-controls")).toBe(panel.getAttribute("id"));
    expect(panel.getAttribute("aria-labelledby")).toBe(all.getAttribute("id"));
  });

  it("leaves only the chosen tab in the tab order", async () => {
    const { user } = renderComponent(<ThreeTabs />);

    expect(screen.getByTestId("tab-all")).toHaveAttribute("tabindex", "0");
    expect(screen.getByTestId("tab-trusted")).toHaveAttribute("tabindex", "-1");
    expect(screen.getByTestId("tab-yours")).toHaveAttribute("tabindex", "-1");

    await user.click(screen.getByTestId("tab-yours"));

    expect(screen.getByTestId("tab-yours")).toHaveAttribute("tabindex", "0");
    expect(screen.getByTestId("tab-all")).toHaveAttribute("tabindex", "-1");
  });

  it("holds only the chosen panel in the document", async () => {
    const { user } = renderComponent(<ThreeTabs />);

    expect(screen.getByTestId("panel-all")).toHaveTextContent("every record");
    expect(screen.queryByTestId("panel-trusted")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("tab-trusted"));

    expect(screen.getByTestId("panel-trusted")).toHaveTextContent("the trusted records");
    expect(screen.queryByTestId("panel-all")).not.toBeInTheDocument();
  });

  it("moves and chooses on the arrow keys, and wraps at both ends", async () => {
    const { user } = renderComponent(<ThreeTabs />);
    screen.getByTestId("tab-all").focus();

    await user.keyboard("{ArrowRight}");

    // Activation follows focus, so the panel is already the new one.
    expect(screen.getByTestId("tab-trusted")).toHaveFocus();
    expect(screen.getByTestId("tab-trusted")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("panel-trusted")).toBeInTheDocument();

    await user.keyboard("{ArrowLeft}{ArrowLeft}");

    expect(screen.getByTestId("tab-yours")).toHaveFocus();
    expect(screen.getByTestId("panel-yours")).toBeInTheDocument();

    await user.keyboard("{ArrowRight}");

    expect(screen.getByTestId("tab-all")).toHaveFocus();
  });

  it("goes to the first tab on Home and the last on End", async () => {
    const { user } = renderComponent(<ThreeTabs />);
    screen.getByTestId("tab-all").focus();

    await user.keyboard("{End}");

    expect(screen.getByTestId("tab-yours")).toHaveFocus();
    expect(screen.getByTestId("tab-yours")).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{Home}");

    expect(screen.getByTestId("tab-all")).toHaveFocus();
    expect(screen.getByTestId("tab-all")).toHaveAttribute("aria-selected", "true");
  });

  it("skips a disabled tab on the way past it", async () => {
    const { user } = renderComponent(
      <Tabs defaultValue="all">
        <TabsList>
          <TabsTrigger value="all" data-testid="tab-all">
            All
          </TabsTrigger>
          <TabsTrigger value="trusted" data-testid="tab-trusted" disabled>
            Trusted
          </TabsTrigger>
          <TabsTrigger value="yours" data-testid="tab-yours">
            Yours
          </TabsTrigger>
        </TabsList>
        <TabsContent value="all" data-testid="panel-all">
          every record
        </TabsContent>
        <TabsContent value="yours" data-testid="panel-yours">
          your records
        </TabsContent>
      </Tabs>,
    );
    screen.getByTestId("tab-all").focus();

    await user.keyboard("{ArrowRight}");

    expect(screen.getByTestId("tab-yours")).toHaveFocus();
    expect(screen.getByTestId("tab-yours")).toHaveAttribute("aria-selected", "true");
  });

  it("tells a controlled caller the value and draws what the caller says", async () => {
    function Controlled() {
      const [value, setValue] = useState("all");
      return (
        <>
          <p data-testid="chosen">{value}</p>
          <Tabs value={value} onValueChange={setValue}>
            <TabsList>
              <TabsTrigger value="all" data-testid="tab-all">
                All
              </TabsTrigger>
              <TabsTrigger value="trusted" data-testid="tab-trusted">
                Trusted
              </TabsTrigger>
            </TabsList>
            <TabsContent value={value} data-testid="panel">
              {value}
            </TabsContent>
          </Tabs>
        </>
      );
    }
    const { user } = renderComponent(<Controlled />);

    await user.click(screen.getByTestId("tab-trusted"));

    expect(screen.getByTestId("chosen")).toHaveTextContent("trusted");
    expect(screen.getByTestId("panel")).toHaveTextContent("trusted");
    expect(screen.getByTestId("tab-trusted")).toHaveAttribute("aria-selected", "true");
  });
});

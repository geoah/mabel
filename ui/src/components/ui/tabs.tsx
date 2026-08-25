import { createContext, useContext, useId, useState } from "react";
import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * The shadcn tabs, vendored without their Radix dependency: this app switches
 * one list for another and never animates the swap, which is most of what the
 * primitive was doing for it. The parts, their names, their `data-slot` and
 * `data-state` attributes and the aria wiring are the shadcn ones, so a screen
 * written against the upstream docs works here.
 *
 * The look is the underlined row from the shadcn base tabs: no box around the
 * row, only a rule under it and a heavier rule under the chosen tab, because a
 * section already draws no border and nothing in this app draws a border inside
 * a border.
 *
 * The panel that is not chosen is unmounted, which is what Radix does without
 * `forceMount`, so two panels never put the same test id in the document twice.
 */
interface TabsState {
  value: string;
  select: (value: string) => void;
  /** The id of the tab for a value, which its panel is labelled by. */
  tabId: (value: string) => string;
  /** The id of the panel for a value, which its tab controls. */
  panelId: (value: string) => string;
}

const TabsContext = createContext<TabsState | null>(null);

function useTabs(part: string): TabsState {
  const state = useContext(TabsContext);
  if (state === null) {
    throw new Error(`${part} must be rendered inside Tabs`);
  }
  return state;
}

export function Tabs({
  value,
  defaultValue = "",
  onValueChange,
  className,
  children,
  ...props
}: Omit<React.ComponentProps<"div">, "onChange"> & {
  /** Controlled state. Left out, the tabs keep their own. */
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
}) {
  const [held, setHeld] = useState(defaultValue);
  const chosen = value ?? held;
  const baseId = useId();

  return (
    <div data-slot="tabs" className={cn("flex flex-col gap-3", className)} {...props}>
      <TabsContext.Provider
        value={{
          value: chosen,
          select: (next) => {
            if (value === undefined) {
              setHeld(next);
            }
            onValueChange?.(next);
          },
          tabId: (forValue) => `${baseId}-tab-${forValue}`,
          panelId: (forValue) => `${baseId}-panel-${forValue}`,
        }}
      >
        {children}
      </TabsContext.Provider>
    </div>
  );
}

export function TabsList({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      role="tablist"
      data-slot="tabs-list"
      className={cn(
        "flex w-full items-center justify-start gap-4 overflow-x-auto border-b",
        className,
      )}
      {...props}
    />
  );
}

/**
 * Moves along the row on the arrow keys, and to its ends on Home and End.
 * Activation follows focus, which is what tabs laid out in a row do everywhere,
 * so arrowing onto a tab shows its panel without a second keypress. The order
 * is read off the row itself rather than kept in state, so a tab added or
 * hidden by a caller needs no bookkeeping here.
 */
function moveAlongRow(
  event: React.KeyboardEvent<HTMLButtonElement>,
  select: (value: string) => void,
): void {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) {
    return;
  }
  const row = event.currentTarget.closest('[role="tablist"]');
  if (row === null) {
    return;
  }
  const tabs = Array.from(row.querySelectorAll<HTMLButtonElement>('[role="tab"]:not(:disabled)'));
  const here = tabs.indexOf(event.currentTarget);
  if (here === -1) {
    return;
  }
  // The row wraps at both ends, as the shadcn and Radix tabs do.
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.length - 1
        : event.key === "ArrowLeft"
          ? (here - 1 + tabs.length) % tabs.length
          : (here + 1) % tabs.length;
  const target = tabs[next];
  if (target === undefined) {
    return;
  }
  // The page scrolls on Home, End and the arrows otherwise, which throws the
  // row off the screen the moment someone uses it.
  event.preventDefault();
  target.focus();
  select(target.dataset.value ?? "");
}

/**
 * One tab. Only the chosen one is in the tab order: the arrow keys reach the
 * rest, so a reader tabbing through the page steps over the row in one press
 * and lands in the panel.
 */
export function TabsTrigger({
  value,
  className,
  onClick,
  onKeyDown,
  ...props
}: React.ComponentProps<"button"> & { value: string }) {
  const tabs = useTabs("TabsTrigger");
  const active = tabs.value === value;

  return (
    <button
      type="button"
      role="tab"
      id={tabs.tabId(value)}
      data-slot="tabs-trigger"
      data-state={active ? "active" : "inactive"}
      data-value={value}
      aria-selected={active}
      aria-controls={tabs.panelId(value)}
      tabIndex={active ? 0 : -1}
      onClick={(event) => {
        onClick?.(event);
        tabs.select(value);
      }}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        moveAlongRow(event, tabs.select);
      }}
      className={cn(
        // Touch first, like every other control here: 40px of row on a phone,
        // 36px from md up.
        "-mb-px inline-flex h-10 items-center justify-center gap-2 whitespace-nowrap border-b-2 border-transparent px-1 text-sm font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 md:h-9",
        "data-[state=active]:border-foreground data-[state=active]:text-foreground",
        className,
      )}
      {...props}
    />
  );
}

/**
 * The panel one tab shows. It takes no tab stop of its own: everything this app
 * puts in a panel is a list of links, which the tab order already reaches.
 */
export function TabsContent({
  value,
  className,
  ...props
}: React.ComponentProps<"div"> & { value: string }) {
  const tabs = useTabs("TabsContent");
  if (tabs.value !== value) {
    return null;
  }
  return (
    <div
      id={tabs.panelId(value)}
      role="tabpanel"
      data-slot="tabs-content"
      data-state="active"
      aria-labelledby={tabs.tabId(value)}
      className={className}
      {...props}
    />
  );
}

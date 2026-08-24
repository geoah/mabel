import { createContext, useContext, useId, useState } from "react";
import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * The shadcn collapsible, vendored without its Radix dependency: this app opens
 * and closes blocks and never animates the height, which is all the primitive
 * was doing for it. The parts, their names, their `data-slot` and `data-state`
 * attributes and the trigger's aria wiring are the shadcn ones, so a block
 * written against the upstream docs works here.
 *
 * Closed content is unmounted, which is what Radix does without `forceMount`.
 */
interface CollapsibleState {
  open: boolean;
  toggle: () => void;
  contentId: string;
}

const CollapsibleContext = createContext<CollapsibleState | null>(null);

function useCollapsible(part: string): CollapsibleState {
  const state = useContext(CollapsibleContext);
  if (state === null) {
    throw new Error(`${part} must be rendered inside a Collapsible`);
  }
  return state;
}

export function Collapsible({
  open,
  defaultOpen = false,
  onOpenChange,
  className,
  children,
  ...props
}: React.ComponentProps<"div"> & {
  /** Controlled state. Left out, the collapsible keeps its own. */
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const [held, setHeld] = useState(defaultOpen);
  const shown = open ?? held;
  const contentId = useId();

  return (
    <div
      data-slot="collapsible"
      data-state={shown ? "open" : "closed"}
      className={className}
      {...props}
    >
      <CollapsibleContext.Provider
        value={{
          open: shown,
          contentId,
          toggle: () => {
            if (open === undefined) {
              setHeld(!shown);
            }
            onOpenChange?.(!shown);
          },
        }}
      >
        {children}
      </CollapsibleContext.Provider>
    </div>
  );
}

/**
 * The control that opens the block. It carries `aria-expanded` and points at
 * the content it opens, and it runs the caller's own `onClick` first, so a
 * trigger inside a clickable card can stop the click reaching the card.
 */
export function CollapsibleTrigger({
  className,
  onClick,
  children,
  ...props
}: React.ComponentProps<"button">) {
  const { open, toggle, contentId } = useCollapsible("CollapsibleTrigger");

  return (
    <button
      type="button"
      data-slot="collapsible-trigger"
      data-state={open ? "open" : "closed"}
      aria-expanded={open}
      aria-controls={contentId}
      onClick={(event) => {
        onClick?.(event);
        toggle();
      }}
      className={cn(
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        className,
      )}
      {...props}
    >
      {children}
    </button>
  );
}

export function CollapsibleContent({ className, ...props }: React.ComponentProps<"div">) {
  const { open, contentId } = useCollapsible("CollapsibleContent");
  if (!open) {
    return null;
  }
  return (
    <div
      id={contentId}
      data-slot="collapsible-content"
      data-state="open"
      className={className}
      {...props}
    />
  );
}

/**
 * The one expand affordance this app draws, anywhere anything expands: a chevron
 * pointing down while the block is closed, which is where the content appears,
 * and up while it is open, which is where pressing it sends the content back. It
 * reads the collapsible it sits in, so no caller passes a state.
 */
export function CollapsibleChevron({ className }: { className?: string }) {
  const { open } = useCollapsible("CollapsibleChevron");
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden="true"
      data-slot="collapsible-chevron"
      data-state={open ? "open" : "closed"}
      className={cn(
        "size-3.5 shrink-0 text-muted-foreground transition-transform",
        open && "rotate-180",
        className,
      )}
      fill="currentColor"
    >
      <path d="M3.72 5.72a.75.75 0 0 1 1.06 0L8 8.94l3.22-3.22a.75.75 0 1 1 1.06 1.06l-3.75 3.75a.75.75 0 0 1-1.06 0L3.72 6.78a.75.75 0 0 1 0-1.06Z" />
    </svg>
  );
}

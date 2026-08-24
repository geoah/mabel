import { createContext, useContext, useId, useState } from "react";
import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * The shadcn tooltip, vendored without its Radix dependency, the way the
 * collapsible was: this app shows one short sentence beside one label and needs
 * no portal and no collision detection. The part names, the `data-slot` and
 * `data-state` attributes and the trigger's aria wiring are the upstream ones,
 * so a block written against the shadcn docs works here.
 *
 * A phone has no hover, so the trigger opens on a tap as well as on a pointer
 * and on focus, and closes on the next tap, on Escape and on blur. Closed
 * content is not rendered.
 */
interface TooltipState {
  open: boolean;
  setOpen: (open: boolean) => void;
  contentId: string;
}

const TooltipContext = createContext<TooltipState | null>(null);

function useTooltip(part: string): TooltipState {
  const state = useContext(TooltipContext);
  if (state === null) {
    throw new Error(`${part} must be rendered inside a Tooltip`);
  }
  return state;
}

/**
 * Upstream this is where the shared delay lives. Nothing here delays, so the
 * provider is the passthrough that keeps a shadcn snippet working unchanged.
 */
export function TooltipProvider({ children }: { children: React.ReactNode }) {
  return <>{children}</>;
}

export function Tooltip({ className, children, ...props }: React.ComponentProps<"span">) {
  const [open, setOpen] = useState(false);
  const contentId = useId();

  return (
    <span
      data-slot="tooltip"
      data-state={open ? "open" : "closed"}
      className={cn("relative inline-flex", className)}
      {...props}
    >
      <TooltipContext.Provider value={{ open, setOpen, contentId }}>
        {children}
      </TooltipContext.Provider>
    </span>
  );
}

export function TooltipTrigger({
  className,
  onClick,
  children,
  ...props
}: React.ComponentProps<"button">) {
  const { open, setOpen, contentId } = useTooltip("TooltipTrigger");

  return (
    <button
      type="button"
      data-slot="tooltip-trigger"
      data-state={open ? "open" : "closed"}
      aria-expanded={open}
      aria-describedby={open ? contentId : undefined}
      // Every one of these opens and none of them toggles: a tap fires enter,
      // focus and click in that order, and a toggle on any of the three would
      // shut the sentence the tap just opened. Leaving, blurring and Escape are
      // what close it.
      onClick={(event) => {
        // A trigger inside a clickable card opens the sentence and nothing else.
        event.stopPropagation();
        onClick?.(event);
        setOpen(true);
      }}
      onPointerEnter={() => setOpen(true)}
      onPointerLeave={(event) => {
        if (event.pointerType === "mouse") {
          setOpen(false);
        }
      }}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          setOpen(false);
        }
      }}
      className={cn(
        "inline-flex items-center justify-center rounded-sm text-muted-foreground",
        "hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        className,
      )}
      {...props}
    >
      {children}
    </button>
  );
}

export function TooltipContent({ className, ...props }: React.ComponentProps<"span">) {
  const { open, contentId } = useTooltip("TooltipContent");
  if (!open) {
    return null;
  }
  return (
    <span
      id={contentId}
      role="tooltip"
      data-slot="tooltip-content"
      data-state="open"
      className={cn(
        "absolute top-full left-0 z-50 mt-1 w-max max-w-56 rounded-md border bg-popover",
        "px-2 py-1 text-xs text-popover-foreground shadow-md",
        className,
      )}
      {...props}
    />
  );
}

import { type ReactNode, useState } from "react";

import { cn } from "@/lib/utils";

/**
 * One task in the actions section: a name saying what the reader gets to do, a
 * one-line description of how it goes, and the form it opens into. The
 * description is on the closed row, so the section reads as a list of what this
 * wallet can do without opening anything.
 *
 * Everything is closed by default. `defaultOpen` exists for a panel that is the
 * only thing on its screen, and no action on the identity page uses it.
 */
export function Action({
  title,
  description,
  testId,
  defaultOpen = false,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <details
      data-testid={testId}
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
      className="rounded-md border"
    >
      <summary
        data-testid={`${testId}-summary`}
        className={cn(
          "flex min-h-11 cursor-pointer list-none flex-col justify-center gap-0.5 px-3 py-2",
          "marker:content-none hover:bg-accent focus-visible:outline-none",
          "focus-visible:ring-1 focus-visible:ring-ring",
        )}
      >
        <span className="text-sm font-medium">
          <span aria-hidden="true" className="mr-1 inline-block w-3 text-muted-foreground">
            {open ? "−" : "+"}
          </span>
          {title}
        </span>
        <span className="ml-4 text-xs text-muted-foreground">{description}</span>
      </summary>
      <div className="space-y-3 border-t px-3 py-3">{children}</div>
    </details>
  );
}

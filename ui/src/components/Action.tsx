import { type ReactNode, useState } from "react";

import { cn } from "@/lib/utils";

/**
 * One operation in the actions section (decision 014): a name, a one-line
 * description of what it does, and the form it opens into. The description is
 * on the closed row, so the section reads as a list of what this wallet can do
 * without opening anything.
 *
 * The three operations a story drives every time (attest, add a witness, push)
 * open by default; the rest stay shut so the page stays a page.
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

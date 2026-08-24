import type { ReactNode } from "react";

import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";

/**
 * One task in the actions section: a name saying what the reader gets to do, a
 * one-line description of how it goes, and the form it opens into. The
 * description is on the closed row, so the section reads as a list of what this
 * wallet can do without opening anything.
 *
 * It is the shared collapsible, with the one chevron every expanding block in
 * this app draws, and it has no border of its own: the rows of one group are
 * divided by a rule, and the only borders inside an opened row belong to its
 * form and to the cards it lists.
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
  return (
    <Collapsible data-testid={testId} defaultOpen={defaultOpen}>
      <CollapsibleTrigger
        data-testid={`${testId}-summary`}
        className="flex w-full min-h-11 items-start gap-2 rounded-md px-1 py-2 text-left hover:bg-accent"
      >
        <CollapsibleChevron className="mt-1" />
        <span className="flex min-w-0 flex-col gap-1">
          <span className="text-sm font-medium">{title}</span>
          <span className="text-xs text-muted-foreground">{description}</span>
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-3 pb-3 pl-6">{children}</CollapsibleContent>
    </Collapsible>
  );
}

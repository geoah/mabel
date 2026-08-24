import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * One section of a page: a heading, an optional one-line description, and its
 * content. A section draws no border and no background, ever. Only the leaf
 * content inside it carries one, which is an identity card, a witness card, a
 * form input or a notice, so no screen draws a border inside a border.
 *
 * The heading is an h2 at 18px, larger than the 16px name on a card and smaller
 * than the 24px name a detail page carries as its h1.
 */
export function Section({
  title,
  description,
  testId,
  descriptionTestId,
  action,
  className,
  children,
}: {
  title: string;
  /** The one line under the heading. A node, so a name can sit in the sentence. */
  description?: ReactNode;
  testId?: string;
  descriptionTestId?: string;
  /** A control belonging to the heading, drawn at the end of its row. */
  action?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section data-testid={testId} className={cn("space-y-3", className)}>
      <div className="space-y-1">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-lg leading-tight font-semibold tracking-tight">{title}</h2>
          {action}
        </div>
        {description !== undefined && (
          <p data-testid={descriptionTestId} className="text-sm text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {children}
    </section>
  );
}

/**
 * The stack every page is: 32px between sections. Nothing in it is a card, and
 * no rule divides it: the headings and the space do that work.
 */
export function PageSections({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return <div className={cn("space-y-8", className)}>{children}</div>;
}

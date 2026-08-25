import type * as React from "react";

import { cn } from "@/lib/utils";

import { buttonVariants } from "./button";

/**
 * The one icon control this app draws: a 32px square, ghost with a border, so a
 * copy button and an expand button are the same size wherever they sit and both
 * read as buttons. The class is exported because a collapsible trigger renders
 * its own button element and wears this instead of nesting one.
 */
export const ICON_BUTTON = cn(
  buttonVariants({ variant: "ghost", size: "icon-sm" }),
  "shrink-0 border border-input bg-background text-muted-foreground hover:text-foreground",
);

/**
 * The control that copies the value beside it: smaller than an icon button and
 * without its border, because it is a handle on the id next to it rather than a
 * control of its own. The id is the thing a reader came to look at.
 */
export const COPY_BUTTON = cn(
  buttonVariants({ variant: "ghost", size: "icon-sm" }),
  "size-5 shrink-0 border-0 bg-transparent text-muted-foreground hover:text-foreground",
);

export function IconButton({ className, ...props }: React.ComponentProps<"button">) {
  return <button type="button" className={cn(ICON_BUTTON, className)} {...props} />;
}

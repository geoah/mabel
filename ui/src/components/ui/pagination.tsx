import type * as React from "react";

import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * The shadcn pagination, vendored with one adaptation: a page control renders a
 * `button` unless it is given an `href`. Upstream every control is an anchor,
 * because upstream a page is a URL; here the ledger page is component state, and
 * an anchor with no destination is not a link. The part names, the class strings
 * and the `data-slot` attributes are the upstream ones.
 */
export function Pagination({ className, ...props }: React.ComponentProps<"nav">) {
  return (
    <nav
      role="navigation"
      aria-label="pagination"
      data-slot="pagination"
      className={cn("flex w-full", className)}
      {...props}
    />
  );
}

export function PaginationContent({ className, ...props }: React.ComponentProps<"ul">) {
  return (
    <ul
      data-slot="pagination-content"
      className={cn("flex flex-row flex-wrap items-center gap-1", className)}
      {...props}
    />
  );
}

export function PaginationItem({ ...props }: React.ComponentProps<"li">) {
  return <li data-slot="pagination-item" {...props} />;
}

type PaginationLinkProps = React.ComponentProps<"button"> & {
  isActive?: boolean;
  size?: "default" | "sm" | "icon";
  /** Renders an anchor, for a pagination whose pages are URLs. */
  href?: string;
};

export function PaginationLink({
  className,
  isActive,
  size = "icon",
  href,
  ...props
}: PaginationLinkProps) {
  const shared = {
    "aria-current": isActive ? ("page" as const) : undefined,
    "data-slot": "pagination-link",
    "data-active": isActive,
    className: cn(buttonVariants({ variant: isActive ? "outline" : "ghost", size }), className),
  };
  if (href !== undefined) {
    // A button's `type` means nothing on an anchor, so it does not travel.
    const { type, ...rest } = props;
    void type;
    return <a href={href} {...shared} {...(rest as React.ComponentProps<"a">)} />;
  }
  return <button type="button" {...shared} {...props} />;
}

export function PaginationPrevious({
  className,
  ...props
}: React.ComponentProps<typeof PaginationLink>) {
  return (
    <PaginationLink
      aria-label="Go to previous page"
      size="sm"
      className={cn("gap-1 px-2.5", className)}
      {...props}
    >
      <ChevronLeftIcon />
      <span>Previous</span>
    </PaginationLink>
  );
}

export function PaginationNext({
  className,
  ...props
}: React.ComponentProps<typeof PaginationLink>) {
  return (
    <PaginationLink
      aria-label="Go to next page"
      size="sm"
      className={cn("gap-1 px-2.5", className)}
      {...props}
    >
      <span>Next</span>
      <ChevronRightIcon />
    </PaginationLink>
  );
}

export function PaginationEllipsis({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      aria-hidden
      data-slot="pagination-ellipsis"
      className={cn("flex size-9 items-center justify-center text-muted-foreground", className)}
      {...props}
    >
      <span>…</span>
      <span className="sr-only">More pages</span>
    </span>
  );
}

function ChevronLeftIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3.5" fill="currentColor">
      <path d="M10.28 3.72a.75.75 0 0 1 0 1.06L7.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L5.47 8.53a.75.75 0 0 1 0-1.06l3.75-3.75a.75.75 0 0 1 1.06 0Z" />
    </svg>
  );
}

function ChevronRightIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3.5" fill="currentColor">
      <path d="M5.72 3.72a.75.75 0 0 1 1.06 0l3.75 3.75a.75.75 0 0 1 0 1.06l-3.75 3.75a.75.75 0 1 1-1.06-1.06L8.94 8 5.72 4.78a.75.75 0 0 1 0-1.06Z" />
    </svg>
  );
}

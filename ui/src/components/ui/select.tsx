import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * A native select styled like the shadcn control. The radix version is dropped
 * on purpose: a native select is one element for Playwright and testing-library
 * to drive, with no portal or pointer-event emulation.
 */
export function Select({ className, ...props }: React.ComponentProps<"select">) {
  return (
    <select
      className={cn(
        "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

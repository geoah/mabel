import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * The compact key-value table decision 014 asks for: key and value sit on one
 * line at every width, table-like, never stacked label over value. The label
 * column is fixed and narrow, and the value column takes the rest and wraps
 * inside itself, which is what keeps a 52-character id on the same row as its
 * key on a phone.
 */
export function KeyValueTable({ className, ...props }: React.ComponentProps<"dl">) {
  return <dl className={cn("divide-y", className)} {...props} />;
}

interface KeyValueProps {
  label: string;
  /** Lands on the value, which is what a suite reads; the row adds -row. */
  testId: string;
  children: React.ReactNode;
}

export function KeyValue({ label, testId, children }: KeyValueProps) {
  return (
    <div
      data-testid={`${testId}-row`}
      className="flex items-baseline gap-3 py-1 first:pt-0 last:pb-0"
    >
      <dt className="w-28 shrink-0 text-xs text-muted-foreground sm:w-40">{label}</dt>
      <dd data-testid={testId} className="min-w-0 flex-1 text-sm">
        {children}
      </dd>
    </div>
  );
}

import type * as React from "react";

import { InfoTip } from "@/components/InfoTip";
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
  /** The sentence a short label will not carry, shown by an info icon beside it. */
  info?: string;
  children: React.ReactNode;
}

export function KeyValue({ label, testId, info, children }: KeyValueProps) {
  return (
    <div
      data-testid={`${testId}-row`}
      className="flex items-baseline gap-3 py-1 first:pt-0 last:pb-0"
    >
      <dt className="flex w-28 shrink-0 items-center gap-1 text-xs text-muted-foreground sm:w-40">
        {label}
        {info !== undefined && <InfoTip text={info} testId={`${testId}-info`} />}
      </dt>
      <dd data-testid={testId} className="min-w-0 flex-1 text-sm">
        {children}
      </dd>
    </div>
  );
}

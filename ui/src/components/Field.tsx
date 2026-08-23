import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * Label and value sit side by side from 640px up. Below that the pair stacks,
 * because a 10rem label column leaves too little room for a 52-character id.
 */
export function FieldGrid({ className, ...props }: React.ComponentProps<"dl">) {
  return (
    <dl
      className={cn(
        "grid grid-cols-[minmax(0,1fr)] gap-x-4 gap-y-2 sm:grid-cols-[10rem_minmax(0,1fr)] sm:gap-y-1",
        className,
      )}
      {...props}
    />
  );
}

interface FieldProps {
  label: string;
  testId: string;
  mono?: boolean;
  children: React.ReactNode;
}

/** One labelled value. testId lands on the value, which is what a suite reads. */
export function Field({ label, testId, mono = false, children }: FieldProps) {
  return (
    <>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd
        data-testid={testId}
        className={cn("min-w-0 text-sm", mono && "break-all font-mono text-xs")}
      >
        {children}
      </dd>
    </>
  );
}

/** Renders null as an em-free placeholder so a null field is still visible. */
export function Nullable({ value }: { value: string | number | null | undefined }) {
  return <>{value === null || value === undefined ? "null" : String(value)}</>;
}

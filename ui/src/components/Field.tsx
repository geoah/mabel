import type * as React from "react";

import { cn } from "@/lib/utils";

export function FieldGrid({ className, ...props }: React.ComponentProps<"dl">) {
  return <dl className={cn("grid grid-cols-[10rem_1fr] gap-x-4 gap-y-1", className)} {...props} />;
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
        className={cn("text-sm", mono && "break-all font-mono text-xs")}
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

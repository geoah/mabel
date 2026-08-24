import type * as React from "react";

import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

/**
 * A form that is one box and one button, on one row: the box takes the width
 * that is left and the button sits beside it. The row wraps only when the box
 * cannot keep its own minimum, which a 360px phone column does not force.
 */
export function InlineForm({ className, ...props }: React.ComponentProps<"form">) {
  return <form className={cn("flex flex-wrap items-end gap-2", className)} {...props} />;
}

/** The box of an inline form, with its label above it. */
export function InlineField({
  label,
  htmlFor,
  className,
  children,
}: {
  label: string;
  htmlFor: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn("min-w-36 flex-1 space-y-1", className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
    </div>
  );
}

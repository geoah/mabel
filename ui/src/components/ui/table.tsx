import { createContext, useContext, type ComponentProps } from "react";

import { cn } from "@/lib/utils";

/**
 * Below the named breakpoint every row is drawn as a card: the head row is
 * dropped and each cell prints its own label from data-label. The DOM stays one
 * table at every width, so the testids and the ARIA roles a suite reads never
 * depend on the viewport. Both class sets are spelled out because Tailwind
 * reads literal strings out of the source.
 */
export type TableStack = "md" | "lg" | "xl" | "none";

const STACK: Record<TableStack, Record<"table" | "header" | "body" | "row" | "cell", string>> = {
  md: {
    table: "max-md:block",
    header: "max-md:hidden",
    body: "max-md:block",
    row: "max-md:mb-2 max-md:block max-md:rounded-md max-md:border max-md:last:mb-0",
    cell:
      "max-md:grid max-md:grid-cols-[7rem_minmax(0,1fr)] max-md:items-start max-md:gap-x-2 " +
      "max-md:justify-items-start " +
      "max-md:border-t max-md:px-2 max-md:py-1.5 max-md:first:border-t-0 max-md:before:text-xs " +
      "max-md:before:text-muted-foreground max-md:before:content-[attr(data-label)]",
  },
  lg: {
    table: "max-lg:block",
    header: "max-lg:hidden",
    body: "max-lg:block",
    row: "max-lg:mb-2 max-lg:block max-lg:rounded-md max-lg:border max-lg:last:mb-0",
    cell:
      "max-lg:grid max-lg:grid-cols-[7rem_minmax(0,1fr)] max-lg:items-start max-lg:gap-x-2 " +
      "max-lg:justify-items-start " +
      "max-lg:border-t max-lg:px-2 max-lg:py-1.5 max-lg:first:border-t-0 max-lg:before:text-xs " +
      "max-lg:before:text-muted-foreground max-lg:before:content-[attr(data-label)]",
  },
  xl: {
    table: "max-xl:block",
    header: "max-xl:hidden",
    body: "max-xl:block",
    row: "max-xl:mb-2 max-xl:block max-xl:rounded-md max-xl:border max-xl:last:mb-0",
    cell:
      "max-xl:grid max-xl:grid-cols-[7rem_minmax(0,1fr)] max-xl:items-start max-xl:gap-x-2 " +
      "max-xl:justify-items-start " +
      "max-xl:border-t max-xl:px-2 max-xl:py-1.5 max-xl:first:border-t-0 max-xl:before:text-xs " +
      "max-xl:before:text-muted-foreground max-xl:before:content-[attr(data-label)]",
  },
  none: { table: "", header: "", body: "", row: "", cell: "" },
};

const StackContext = createContext<TableStack>("none");

export function Table({
  className,
  stack = "md",
  ...props
}: ComponentProps<"table"> & { stack?: TableStack }) {
  return (
    <StackContext.Provider value={stack}>
      <div className="table-scroll w-full max-w-full overflow-x-auto">
        <table
          role="table"
          className={cn("w-full caption-bottom text-sm", STACK[stack].table, className)}
          {...props}
        />
      </div>
    </StackContext.Provider>
  );
}

export function TableHeader({ className, ...props }: ComponentProps<"thead">) {
  const stack = useContext(StackContext);
  return (
    <thead
      role="rowgroup"
      className={cn("[&_tr]:border-b", STACK[stack].header, className)}
      {...props}
    />
  );
}

export function TableBody({ className, ...props }: ComponentProps<"tbody">) {
  const stack = useContext(StackContext);
  return (
    <tbody
      role="rowgroup"
      className={cn("[&_tr:last-child]:border-0", STACK[stack].body, className)}
      {...props}
    />
  );
}

export function TableRow({ className, ...props }: ComponentProps<"tr">) {
  const stack = useContext(StackContext);
  return (
    <tr
      role="row"
      className={cn(
        "border-b transition-colors hover:bg-muted/50",
        STACK[stack].row,
        className,
      )}
      {...props}
    />
  );
}

export function TableHead({ className, ...props }: ComponentProps<"th">) {
  return (
    <th
      role="columnheader"
      className={cn(
        "h-9 px-2 text-left align-middle text-xs font-medium whitespace-nowrap text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

/** label names the column when the row is stacked, where no head row is drawn. */
export function TableCell({
  className,
  label,
  ...props
}: ComponentProps<"td"> & { label?: string }) {
  const stack = useContext(StackContext);
  return (
    <td
      role="cell"
      data-label={label ?? ""}
      className={cn("p-2 align-middle", STACK[stack].cell, className)}
      {...props}
    />
  );
}

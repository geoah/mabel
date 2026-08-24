import { useMemo } from "react";

import { encode } from "uqr";

import { cn } from "@/lib/utils";

/**
 * The quiet border the spec asks for, in modules. A square printed hard against
 * its neighbour does not scan.
 */
const QUIET = 4;

/**
 * One string as a square a camera reads. The encoder is uqr, pinned exactly: it
 * has no dependencies of its own and answers a matrix, which is drawn here as
 * one path rather than a few thousand rects.
 *
 * The same string is always on the screen beside it, as text with a copy
 * control: the square is a convenience for a phone across a table, never the
 * only way to get the value out.
 */
export function QrSquare({
  value,
  testId,
  label,
  className,
}: {
  value: string;
  testId: string;
  /** What the square holds, for a reader who cannot see it. */
  label: string;
  className?: string;
}) {
  const { size, path } = useMemo(() => {
    const encoded = encode(value, { ecc: "M" });
    const parts: string[] = [];
    for (let row = 0; row < encoded.size; row += 1) {
      for (let column = 0; column < encoded.size; column += 1) {
        if (encoded.data[row][column]) {
          parts.push(`M${column + QUIET} ${row + QUIET}h1v1h-1z`);
        }
      }
    }
    return { size: encoded.size + QUIET * 2, path: parts.join("") };
  }, [value]);

  return (
    <svg
      data-testid={testId}
      data-value={value}
      role="img"
      aria-label={label}
      viewBox={`0 0 ${size} ${size}`}
      // A square is read by a camera, so it is drawn in black on white at every
      // theme: inverting it stops some readers.
      className={cn("h-40 w-40 rounded-md border bg-white p-1", className)}
      shapeRendering="crispEdges"
    >
      <path d={path} fill="#000000" />
    </svg>
  );
}

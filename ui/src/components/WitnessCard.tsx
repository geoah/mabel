import type { ReactNode } from "react";

import { Identifier } from "@/components/Identifier";
import { Card } from "@/components/ui/card";

/**
 * One witness as a card, in the same shape as an identity card and with the same
 * single border: the word witness and any badge on the first line, the Iroh ID
 * under it. The id is never truncated here, because it is the only name a
 * witness has, and it is the card's anchor: nothing on a witness expands, so the
 * card draws no control at all.
 *
 * Every screen that draws a witness draws this, so the list of witnesses and the
 * witnesses one identity chose cannot drift apart.
 */
export function WitnessCard({
  endpointId,
  testIdPrefix,
  badge,
  children,
}: {
  endpointId: string;
  /** `<prefix>-<endpoint id>` names the card, `<prefix>-link-<endpoint id>` its anchor. */
  testIdPrefix: string;
  /** Drawn in the top right corner, for a fact about this witness. */
  badge?: ReactNode;
  /** The lines under the id: where this wallet knows the witness from. */
  children?: ReactNode;
}) {
  const to = `/witnesses/${endpointId}`;

  return (
    <Card
      data-testid={`${testIdPrefix}-${endpointId}`}
      className="relative cursor-pointer p-3 transition-colors focus-within:ring-1 focus-within:ring-ring hover:border-foreground/30 hover:bg-accent/40 sm:p-4"
    >
      {/* The first line: the kind, and the badge at the end of it. The id comes
          under it, across the whole card: 52 characters and a copy button do not
          share a phone's width with a badge. */}
      <div className="flex items-center justify-between gap-3">
        <h3
          data-testid={`${testIdPrefix}-kind-line-${endpointId}`}
          className="min-w-0 text-base leading-tight font-medium"
        >
          witness
        </h3>
        {badge !== undefined && <div className="shrink-0">{badge}</div>}
      </div>
      <Identifier
        value={endpointId}
        full
        to={to}
        stretch
        linkTestId={`${testIdPrefix}-link-${endpointId}`}
        copyLabel="Copy Iroh ID"
        className="mt-2"
      />
      {children !== undefined && <div className="relative z-10 mt-2">{children}</div>}
    </Card>
  );
}

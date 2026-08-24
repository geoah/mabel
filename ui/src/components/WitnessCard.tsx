import type { MouseEvent, ReactNode } from "react";
import { useNavigate } from "react-router";

import { Identifier } from "@/components/Identifier";
import { Card } from "@/components/ui/card";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemHeader,
  ItemTitle,
} from "@/components/ui/item";

/**
 * One witness as a card, in the same shape as an identity card: the kind on the
 * first small line, the id under it, and a badge in the top right corner. The
 * id is never truncated here, because it is the only name a witness has.
 *
 * Every screen that draws a witness draws this, so the list of witnesses and the
 * witnesses one identity chose cannot drift apart. The whole card is clickable
 * and the id is the real anchor, exactly as on an identity card.
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
  const navigate = useNavigate();
  const to = `/witnesses/${endpointId}`;

  function open(event: MouseEvent<HTMLDivElement>) {
    if ((event.target as HTMLElement).closest("a,button")) {
      return;
    }
    void navigate(to);
  }

  return (
    <Card
      data-testid={`${testIdPrefix}-${endpointId}`}
      onClick={open}
      className="cursor-pointer p-3 transition-colors hover:border-foreground/30 hover:bg-accent sm:p-4"
    >
      <Item size="flush" className="items-start">
        {/* The first line of the card: the kind, and the badge in the corner. */}
        <ItemHeader>
          <ItemDescription data-testid={`${testIdPrefix}-kind-line-${endpointId}`}>
            witness
          </ItemDescription>
          {badge !== undefined && <ItemActions className="ml-auto">{badge}</ItemActions>}
        </ItemHeader>
        <ItemContent>
          <ItemTitle className="flex-wrap">
            <Identifier
              value={endpointId}
              full
              to={to}
              linkTestId={`${testIdPrefix}-link-${endpointId}`}
            />
          </ItemTitle>
          {children}
        </ItemContent>
      </Item>
    </Card>
  );
}

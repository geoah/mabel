import type { DeclaredKind } from "@/api/types";
import { Badge } from "@/components/ui/badge";

/**
 * Proposal 002 section 3: the kind an identity declares is advisory. It gates no
 * payload validity, no authorization and no verification outcome, so every
 * surface says "declared kind" rather than repeating a disclaimer beside it
 * (proposal 005, which removed the advisory sentence outright).
 *
 * A card draws it as a badge in the quiet tone: it labels what the identity says
 * it is. The pills, which say something about your own trust, keep the corner
 * and the colours.
 */
export function DeclaredKindValue({ kind, testId }: { kind: DeclaredKind; testId: string }) {
  return (
    <Badge variant="secondary" data-testid={testId} data-declared-kind={kind}>
      {kind}
    </Badge>
  );
}

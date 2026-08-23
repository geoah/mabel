import { Badge } from "@/components/ui/badge";
import type { DeclaredKind } from "@/api/types";

/**
 * Proposal 002 section 3: the kind an identity declares is advisory. It gates no
 * payload validity, no authorization and no verification outcome, so every
 * surface says "declared kind" and repeats why.
 */
export const DECLARED_KIND_NOTE =
  "declared kind is advisory: it gates no authorization, no payload validity and no verification outcome";

export function DeclaredKindValue({
  kind,
  testId,
}: {
  kind: DeclaredKind;
  testId: string;
}) {
  return (
    <Badge variant="secondary" data-testid={testId}>
      {kind}
    </Badge>
  );
}

export function DeclaredKindNote({ testId }: { testId: string }) {
  return (
    <p data-testid={testId} className="text-xs text-muted-foreground">
      {DECLARED_KIND_NOTE}
    </p>
  );
}

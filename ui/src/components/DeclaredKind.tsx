import type { DeclaredKind } from "@/api/types";

/**
 * Proposal 002 section 3: the kind an identity declares is advisory. It gates no
 * payload validity, no authorization and no verification outcome, so every
 * surface says "declared kind" rather than repeating a disclaimer beside it
 * (proposal 005, which removed the advisory sentence outright).
 *
 * It is the label line of a card, not a badge: the badges a card draws are the
 * pills, which say something about your own trust.
 */
export function DeclaredKindValue({ kind, testId }: { kind: DeclaredKind; testId: string }) {
  return <span data-testid={testId}>{kind}</span>;
}

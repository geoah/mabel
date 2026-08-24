import type { ReactNode } from "react";
import { Link } from "react-router";

import type { DeclaredKind, ResolvedIdentity as ResolvedIdentityDocument } from "@/api/types";
import { DeclaredKindValue } from "@/components/DeclaredKind";
import { ResolvedIdentity, ResolvedIdentityScope } from "@/components/ResolvedIdentity";
import { Card } from "@/components/ui/card";

/**
 * One entry of the identity card list (proposal 004). The same list renders the
 * wallet's own identities, what a witness holds and what the witness node holds,
 * so nothing in it may depend on the wallet's private state.
 */
export interface IdentityCardEntry {
  /** The resolved name, id and verdict; the id alone when nothing named it. */
  identity: ResolvedIdentityDocument;
  declaredKind: DeclaredKind | null;
  headSeq: number | null;
  /** Where the whole card navigates, always an identity page. */
  to: string;
  /** True when the identity document reports its verified result as aged. */
  stale?: boolean;
  /** Badges drawn on the second line, after the kind and the head seq. */
  markers?: ReactNode;
}

/**
 * One identity as a card: the resolved name with its verification mark, the id,
 * the declared kind and the head sequence. The whole card is one link, which is
 * why the id renders plain: an expand button inside an anchor is not valid HTML,
 * and the identity page carries the copyable id.
 */
export function IdentityCard({ entry }: { entry: IdentityCardEntry }) {
  const id = entry.identity.identity_id;
  return (
    <Card
      data-testid={`identity-card-${id}`}
      className="overflow-hidden transition-colors hover:border-foreground/30 hover:bg-accent"
    >
      <Link
        to={entry.to}
        data-testid={`identity-card-link-${id}`}
        className="flex min-h-16 flex-col justify-center gap-1.5 p-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring sm:p-4"
      >
        <ResolvedIdentity
          identity={entry.identity}
          stale={entry.stale}
          plain
          testId={`identity-card-name-${id}`}
        />
        <span className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          {entry.declaredKind !== null && (
            <DeclaredKindValue
              kind={entry.declaredKind}
              testId={`identity-card-declared-kind-${id}`}
            />
          )}
          {entry.headSeq !== null && (
            <span data-testid={`identity-card-head-seq-${id}`}>at position {entry.headSeq}</span>
          )}
          {entry.markers}
        </span>
      </Link>
    </Card>
  );
}

/**
 * A list of identities. The scope is the whole list, so two entries resolving
 * to one name both drop the id truncation and stay tellable apart.
 */
export function IdentityCardList({
  entries,
  testId,
  empty,
  emptyTestId = `${testId}-empty`,
}: {
  entries: IdentityCardEntry[];
  testId: string;
  /** What the list says when it holds nothing. */
  empty: string;
  emptyTestId?: string;
}) {
  if (entries.length === 0) {
    return (
      <p data-testid={emptyTestId} className="text-sm">
        {empty}
      </p>
    );
  }
  return (
    <ResolvedIdentityScope identities={entries.map((entry) => entry.identity)}>
      <ul data-testid={testId} className="grid gap-2 sm:grid-cols-2">
        {entries.map((entry) => (
          <li key={entry.identity.identity_id} className="min-w-0">
            <IdentityCard entry={entry} />
          </li>
        ))}
      </ul>
    </ResolvedIdentityScope>
  );
}

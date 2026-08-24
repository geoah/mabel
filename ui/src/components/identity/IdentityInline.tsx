import type { ResolvedIdentity as ResolvedIdentityDocument } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { cn } from "@/lib/utils";

import { resolveName, useSharedName, VerificationMark } from "./names";
import { IdentityPillBadge, type Pill, usePill } from "./pill";

interface IdentityInlineProps {
  identity: ResolvedIdentityDocument;
  /** True when the identity document reports its verified result as aged. */
  stale?: boolean;
  /** Lands on the wrapper; the name, pill, id and verdict add their own suffixes. */
  testId?: string;
  /** Routes the id, for the ids that address a screen. */
  to?: string;
  /** Overrides `<testId>-link` on the routed id, for a list a suite navigates by. */
  linkTestId?: string;
  /** Overrides the pill the screen's own facts would give this id. */
  pill?: Pill | null;
  className?: string;
}

/**
 * One identity on one line: the name it publishes or the one you gave it, the
 * verdict on the handle it claims, its pill, and its id with a button that
 * copies it. Every sentence, list row, path hop and tight row that names an
 * identity renders this, so no screen invents its own spelling of a name and an
 * id (proposal 005).
 */
export function IdentityInline({
  identity,
  stale = false,
  testId,
  to,
  linkTestId,
  pill,
  className,
}: IdentityInlineProps) {
  const { name, source } = resolveName(identity);
  // Two entries of one list resolving to the same name both drop the
  // truncation, because the id is the only thing telling them apart.
  const shared = useSharedName(name);
  const fromScope = usePill(identity.identity_id);
  const shown = pill === undefined ? fromScope : pill;

  return (
    <span
      data-testid={testId}
      data-identity-id={identity.identity_id}
      data-name-source={source}
      data-shared-name={String(shared)}
      className={cn("inline-flex flex-wrap items-baseline gap-x-2 gap-y-0.5", className)}
    >
      {name !== null && (
        <span data-testid={testId && `${testId}-name`} className="text-sm">
          {name}
        </span>
      )}
      <VerificationMark
        status={identity.verification_status}
        hostname={identity.hostname}
        stale={stale}
        testId={testId && `${testId}-verification`}
      />
      {shown && <IdentityPillBadge pill={shown} testId={testId && `${testId}-pill`} />}
      <Identifier
        value={identity.identity_id}
        full={shared}
        to={to}
        linkTestId={linkTestId ?? (testId && `${testId}-link`)}
        className="text-muted-foreground"
      />
    </span>
  );
}

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
  /**
   * `stacked` puts the name on its own line, larger, with the id under it: what
   * a card heading needs. `inline` is one line, which is every other use.
   */
  layout?: "inline" | "stacked";
  /** Draws the whole id, for an inline use that sits on a card and has the width. */
  full?: boolean;
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
  layout = "inline",
  full = false,
  className,
}: IdentityInlineProps) {
  const { name, source, nickname } = resolveName(identity);
  // Two entries of one list resolving to the same name both drop the
  // truncation, because the id is the only thing telling them apart.
  const shared = useSharedName(name);
  const fromScope = usePill(identity.identity_id);
  const shown = pill === undefined ? fromScope : pill;
  const stacked = layout === "stacked";

  const heading = (
    <>
      {name !== null && (
        <span
          data-testid={testId && `${testId}-name`}
          className={stacked ? "text-base leading-tight font-medium" : "text-sm"}
        >
          {name}
        </span>
      )}
      {/* The name you gave them, after the name they publish. */}
      {nickname !== null && (
        <span
          data-testid={testId && `${testId}-nickname`}
          className="text-sm text-muted-foreground"
        >
          ({nickname})
        </span>
      )}
      <VerificationMark
        status={identity.verification_status}
        hostname={identity.hostname}
        stale={stale}
        testId={testId && `${testId}-verification`}
      />
      {shown && <IdentityPillBadge pill={shown} testId={testId && `${testId}-pill`} />}
    </>
  );
  const id = (
    <Identifier
      value={identity.identity_id}
      // A card has the width for the whole id, and a Mabel ID is the only thing
      // that tells two identities apart: no card truncates one.
      full={shared || stacked || full}
      to={to}
      linkTestId={linkTestId ?? (testId && `${testId}-link`)}
      className="text-muted-foreground"
    />
  );

  return (
    <span
      data-testid={testId}
      data-identity-id={identity.identity_id}
      data-name-source={source}
      data-shared-name={String(shared)}
      className={cn(
        stacked
          ? "flex min-w-0 flex-col items-start gap-0.5"
          : "inline-flex flex-wrap items-baseline gap-x-2 gap-y-0.5",
        className,
      )}
    >
      {stacked ? (
        <span className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">{heading}</span>
      ) : (
        heading
      )}
      {id}
    </span>
  );
}

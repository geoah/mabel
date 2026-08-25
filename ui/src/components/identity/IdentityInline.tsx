import type { ReactNode } from "react";
import { Link } from "react-router";

import type { ResolvedIdentity as ResolvedIdentityDocument } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { cn } from "@/lib/utils";

import { resolveName, useSharedName, VerificationMark } from "./names";
import { IdentityPillBadge, type Pill, usePill } from "./pill";

/**
 * `inline` is one line, which is every sentence and tight row. `stacked` is a
 * card heading: the name at 16px on its own line with the id under it. `page` is
 * the same, with the name as the page's h1 at 24px.
 */
export type IdentityInlineLayout = "inline" | "stacked" | "page";

/** How much of the id stands in for a name when an identity has none. */
export const PLACEHOLDER_CHARS = 8;

interface IdentityInlineProps {
  identity: ResolvedIdentityDocument;
  /** True when the identity document reports its verified result as aged. */
  stale?: boolean;
  /** Lands on the wrapper; the name, pill, id and verdict add their own suffixes. */
  testId?: string;
  /** Routes the name, for the ids that address a screen. */
  to?: string;
  /** Overrides `<testId>-link` on the routed name, for a list a suite navigates by. */
  linkTestId?: string;
  /** Overrides the pill the screen's own facts would give this id. */
  pill?: Pill | null;
  layout?: IdentityInlineLayout;
  /** Drawn at the end of the name line: what the identity says it is. */
  trailing?: ReactNode;
  /** Drawn at the far end of the name line, on a card: the pills and the controls. */
  aside?: ReactNode;
  /** Draws the whole id, for an inline use that sits on a card and has the width. */
  full?: boolean;
  /**
   * Makes the name's link cover the nearest positioned ancestor, which is the
   * card it heads: one anchor, reachable by keyboard, and the whole card clicks.
   */
  stretch?: boolean;
  className?: string;
}

/**
 * One identity: the name it publishes or the one you gave it, the verdict on the
 * handle it claims, its pill, and its id with a button that copies it. Every
 * sentence, list row, path hop and card heading that names an identity renders
 * this, so no screen invents its own spelling of a name and an id (proposal
 * 005).
 *
 * The name is the link. A nameless identity has only its id to click, so the id
 * carries the link instead, and either way one anchor per identity is what a
 * keyboard and a screen reader reach.
 */
export function IdentityInline({
  identity,
  stale = false,
  testId,
  to,
  linkTestId,
  pill,
  layout = "inline",
  trailing,
  aside,
  full = false,
  stretch = false,
  className,
}: IdentityInlineProps) {
  const { name, source, nickname } = resolveName(identity);
  // An identity that publishes no name and that this device has never named
  // still needs something to be called on a card, so it is titled with the
  // first characters of its id. That title stands in for a name: it is not the
  // id being shown, which is why it carries no prefix and why the rule that
  // every shown id is whole does not reach it (decision 019). The whole id is
  // under it as on every other card.
  const title = name ?? `${identity.identity_id.slice(0, PLACEHOLDER_CHARS)}…`;
  // Two entries of one list resolving to the same name both drop the
  // truncation, because the id is the only thing telling them apart.
  const shared = useSharedName(name);
  const fromScope = usePill(identity.identity_id);
  const shown = pill === undefined ? fromScope : pill;
  const stacked = layout !== "inline";
  const linkId = linkTestId ?? (testId && `${testId}-link`);
  const NameTag = layout === "page" ? "h1" : layout === "stacked" ? "h3" : "span";

  const heading = (
    <>
      {/* The name and what the identity says it is stay together: on a phone the
          pair wraps as one, so the kind never ends up alone on a line. */}
      <NameTag
        data-testid={testId && `${testId}-name`}
        data-placeholder-name={String(name === null)}
        className={cn(
          layout === "page"
            ? "text-2xl leading-tight font-semibold tracking-tight"
            : layout === "stacked"
              ? "text-base leading-tight font-medium"
              : "text-sm",
          // A stand-in reads quieter than a name someone chose.
          name === null && "font-mono text-muted-foreground",
        )}
      >
        {to === undefined ? (
          title
        ) : (
          <Link
            to={to}
            data-testid={linkId}
            className={cn(
              "hover:underline focus-visible:outline-none",
              stretch && "after:absolute after:inset-0",
            )}
          >
            {title}
          </Link>
        )}
      </NameTag>
      {trailing}
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
      copyLabel="Copy Mabel ID"
      mabel
    />
  );

  if (!stacked) {
    return (
      <span
        data-testid={testId}
        data-identity-id={identity.identity_id}
        data-name-source={source}
        data-shared-name={String(shared)}
        className={cn("inline-flex flex-wrap items-baseline gap-x-2 gap-y-0.5", className)}
      >
        {heading}
        {id}
      </span>
    );
  }

  return (
    <div
      data-testid={testId}
      data-identity-id={identity.identity_id}
      data-name-source={source}
      data-shared-name={String(shared)}
      className={cn("flex min-w-0 flex-col gap-2", className)}
    >
      {/* The name, what it says it is, and the pills: one line, the pills at the
          end of it. The id comes under the line, across the whole card. */}
      {/* items-start, not center: on a phone the name and what it says it is
          wrap onto two lines, and the pills stay beside the name rather than
          drifting down to the middle of the block. */}
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">{heading}</div>
        {aside !== undefined && (
          <div className="flex shrink-0 items-center gap-1">{aside}</div>
        )}
      </div>
      {id}
    </div>
  );
}

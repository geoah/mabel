import { createContext, type ReactNode, useContext } from "react";

import type { Identity } from "@/api/types";
import { Badge } from "@/components/ui/badge";

/**
 * The one badge both identity components draw, in the three states proposal 005
 * fixes and no others:
 *
 * - `your identity`, when this wallet holds a key that may sign for it;
 * - `trusted`, when one of your own identities has said it trusts them and has
 *   not taken it back;
 * - `trusted (Nd)`, when the last crawl this home stored reaches them in N
 *   steps through other people, and N is more than one.
 *
 * Nothing else draws a badge, and no state costs a request: every screen builds
 * the facts below out of documents it already loaded. A screen that never
 * loaded them draws no pill at all, and the identity page carries the
 * authoritative one.
 */
export type PillKind = "own" | "trusted" | "degree";

export interface Pill {
  kind: PillKind;
  label: string;
  /** The number of steps a degree pill counts, null for the other two. */
  degrees: number | null;
}

/** What one screen knows about every id it draws. */
export interface PillFacts {
  /** The ids this wallet may sign for. */
  own: ReadonlySet<string>;
  /** The ids one of your identities holds an unrevoked attestation for. */
  trusted: ReadonlySet<string>;
  /** The shortest path length the stored crawl knows, per id. */
  degrees: ReadonlyMap<string, number>;
}

export const NO_PILL_FACTS: PillFacts = {
  own: new Set<string>(),
  trusted: new Set<string>(),
  degrees: new Map<string, number>(),
};

/** The first state that applies, in the order proposal 005 lists them. */
export function pillFor(identityId: string, facts: PillFacts): Pill | null {
  if (facts.own.has(identityId)) {
    return { kind: "own", label: "your identity", degrees: null };
  }
  if (facts.trusted.has(identityId)) {
    return { kind: "trusted", label: "trusted", degrees: null };
  }
  const degrees = facts.degrees.get(identityId);
  // One step is the green state, which the trust lists above already answered:
  // a degree pill only says how far away someone you have not vouched for is.
  if (degrees !== undefined && degrees > 1) {
    return { kind: "degree", label: `trusted (${degrees}d)`, degrees };
  }
  return null;
}

/** Every id one of these identities has said it trusts and not taken back. */
export function trustedSubjects(identities: Identity[]): Set<string> {
  const subjects = new Set<string>();
  for (const identity of identities) {
    for (const record of identity.trust) {
      if (!record.revoked) {
        subjects.add(record.subject);
      }
    }
  }
  return subjects;
}

const PillContext = createContext<PillFacts>(NO_PILL_FACTS);

/**
 * Hands one screen's facts to every identity drawn under it. The scope exists so
 * a hop three lists deep gets the same pill as the card at the top without the
 * screen threading a prop through four components.
 */
export function IdentityPillScope({
  facts,
  children,
}: {
  facts: PillFacts;
  children: ReactNode;
}) {
  return <PillContext.Provider value={facts}>{children}</PillContext.Provider>;
}

export function usePill(identityId: string): Pill | null {
  return pillFor(identityId, useContext(PillContext));
}

const TONES: Record<PillKind, string> = {
  // The quiet one on purpose: every card in your own wallet wears it, so a
  // solid black badge on each of them is the loudest thing on the screen.
  own: "border-border",
  trusted: "border-green-700/40 bg-green-50 text-green-900",
  degree: "border-amber-700/40 bg-amber-50 text-amber-900",
};

const TITLES: Record<PillKind, string> = {
  own: "This wallet holds a key that can sign for this identity.",
  trusted: "One of your identities has said it trusts them.",
  degree: "The last crawl your wallet stored reaches them through other people.",
};

export function IdentityPillBadge({ pill, testId }: { pill: Pill; testId?: string }) {
  return (
    <Badge
      data-testid={testId}
      data-pill={pill.kind}
      variant={pill.kind === "own" ? "secondary" : "outline"}
      title={TITLES[pill.kind]}
      className={TONES[pill.kind]}
    >
      {pill.label}
    </Badge>
  );
}

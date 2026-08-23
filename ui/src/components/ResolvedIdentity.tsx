import { createContext, type ReactNode, useContext, useMemo } from "react";

import type {
  Identity,
  ResolvedIdentity as ResolvedIdentityDocument,
  VerificationStatus,
} from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { cn } from "@/lib/utils";

/**
 * One component renders every identity that carries a name, so no screen can
 * forget the anti-spoofing rules of proposal 003 section 4: a name is plain
 * text and an id and a hostname are monospace, the id is always beside the
 * name, two entries resolving to the same name both show their full ids, and
 * nothing here sorts, matches or deduplicates on a name.
 */

/** Which source the shown name came from; "id" means there is no name. */
export type NameSource = "profile" | "alias" | "id";

export interface ResolvedName {
  /** null when neither a profile name nor an alias exists: the id is the label. */
  name: string | null;
  source: NameSource;
}

/**
 * Resolution order, fixed by proposal 003 section 4: the profile display name,
 * then the local alias or contact nickname, then the id itself.
 */
export function resolveName(identity: ResolvedIdentityDocument): ResolvedName {
  if (identity.display_name) {
    return { name: identity.display_name, source: "profile" };
  }
  if (identity.alias) {
    return { name: identity.alias, source: "alias" };
  }
  return { name: null, source: "id" };
}

/** The names two or more entries of one list share, which forces their full ids. */
export function duplicateNames(identities: ResolvedIdentityDocument[]): Set<string> {
  const firstHolder = new Map<string, string>();
  const duplicates = new Set<string>();
  for (const identity of identities) {
    const { name } = resolveName(identity);
    if (name === null) {
      continue;
    }
    const holder = firstHolder.get(name);
    if (holder === undefined) {
      firstHolder.set(name, identity.identity_id);
    } else if (holder !== identity.identity_id) {
      duplicates.add(name);
    }
  }
  return duplicates;
}

const DuplicateNameContext = createContext<ReadonlySet<string>>(new Set<string>());

/**
 * Wraps one list so its entries know which names it repeats. Without a scope an
 * identity renders alone and its id is truncated as usual.
 */
export function ResolvedIdentityScope({
  identities,
  children,
}: {
  identities: ResolvedIdentityDocument[];
  children: ReactNode;
}) {
  const duplicates = useMemo(() => duplicateNames(identities), [identities]);
  return (
    <DuplicateNameContext.Provider value={duplicates}>{children}</DuplicateNameContext.Provider>
  );
}

/** The same advisory sentence the declared kind carries (decision 015). */
export const VERIFICATION_NOTE =
  "hostname verification is advisory: it gates no authorization and no ledger validity";

export function VerificationNote({ testId }: { testId: string }) {
  return (
    <p data-testid={testId} className="text-xs text-muted-foreground">
      {VERIFICATION_NOTE}
    </p>
  );
}

/** verified with a stale marker is its own rendered state, never a plain check. */
export type VerificationState = VerificationStatus | "stale-verified";

interface Mark {
  glyph: string;
  tone: string;
  sentence: string;
}

const MARKS: Record<Exclude<VerificationState, "unclaimed">, Mark> = {
  verified: {
    glyph: "✓",
    tone: "text-foreground",
    sentence: "the TXT record at _mabel.HOSTNAME names this identity",
  },
  "stale-verified": {
    glyph: "✓",
    tone: "text-muted-foreground",
    sentence: "verified more than a day ago and not rechecked since",
  },
  mismatched: {
    glyph: "⚠",
    tone: "text-destructive",
    sentence: "the TXT record at _mabel.HOSTNAME names a different identity",
  },
  unverified: {
    glyph: "·",
    tone: "text-muted-foreground",
    sentence: "no mabel record at _mabel.HOSTNAME, or this node has not checked yet",
  },
  unreachable: {
    glyph: "?",
    tone: "text-muted-foreground",
    sentence: "the resolver could not answer for _mabel.HOSTNAME",
  },
};

export function verificationState(
  status: VerificationStatus,
  stale: boolean,
): VerificationState {
  return status === "verified" && stale ? "stale-verified" : status;
}

/**
 * The verdict glyph. It never appears alone: the hostname it is about travels
 * with it, in the monospace style hostnames and ids share, so a display name
 * can never pass for a verified host. An unclaimed hostname renders nothing.
 */
export function VerificationMark({
  status,
  hostname,
  stale = false,
  testId,
}: {
  status: VerificationStatus;
  hostname: string | null;
  stale?: boolean;
  testId?: string;
}) {
  if (status === "unclaimed" || hostname === null) {
    return null;
  }
  const state = verificationState(status, stale);
  const mark = MARKS[state as Exclude<VerificationState, "unclaimed">];
  const sentence = mark.sentence.replace("HOSTNAME", hostname);
  return (
    <span
      data-testid={testId}
      data-verification={state}
      title={`${state}: ${sentence}. ${VERIFICATION_NOTE}`}
      className={cn("inline-flex items-baseline gap-1 text-xs", mark.tone)}
    >
      <span aria-hidden="true">{mark.glyph}</span>
      <span className="sr-only">{state}</span>
      <span className="font-mono">{hostname}</span>
      {state === "stale-verified" && <span className="italic">stale</span>}
    </span>
  );
}

interface ResolvedIdentityProps {
  identity: ResolvedIdentityDocument;
  /** True when the identity document reports its verified result as aged. */
  stale?: boolean;
  /** Lands on the wrapper; the name, id and verdict add their own suffixes. */
  testId?: string;
  /** Routes the id, for the ids that address a screen. */
  to?: string;
  /** Drops the id's expand and copy buttons, for a name drawn inside a link. */
  plain?: boolean;
  className?: string;
}

/**
 * One foreign or local identity: its resolved name as plain text, its id beside
 * it in the identifier style, and its hostname verdict when it claims one.
 */
export function ResolvedIdentity({
  identity,
  stale = false,
  testId,
  to,
  plain = false,
  className,
}: ResolvedIdentityProps) {
  const duplicates = useContext(DuplicateNameContext);
  const { name, source } = resolveName(identity);
  // Two entries of one list resolving to the same name both drop the
  // truncation, because the id is the only thing telling them apart.
  const shared = name !== null && duplicates.has(name);

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
      <Identifier
        value={identity.identity_id}
        full={shared}
        plain={plain}
        to={to}
        linkTestId={testId && `${testId}-link`}
        className="text-muted-foreground"
      />
      <VerificationMark
        status={identity.verification_status}
        hostname={identity.hostname}
        stale={stale}
        testId={testId && `${testId}-verification`}
      />
    </span>
  );
}

/**
 * The ResolvedIdentity the node would return for an identity this home holds,
 * built from the identity document so the local screens and the crawled ones
 * render through the same component.
 */
export function resolvedFrom(identity: Identity): ResolvedIdentityDocument {
  const displayName = identity.profile?.display_name ?? null;
  const alias = identity.contact?.nickname ?? identity.alias;
  return {
    identity_id: identity.identity_id,
    display_name: displayName,
    alias,
    hostname: identity.profile?.hostname ?? null,
    verification_status: identity.verification.status,
    provenance: displayName ? "profile" : alias ? "alias" : "none",
  };
}

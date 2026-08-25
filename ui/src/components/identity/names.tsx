import { createContext, type ReactNode, useContext, useMemo } from "react";

import type {
  Identity,
  ResolvedIdentity as ResolvedIdentityDocument,
  VerificationStatus,
} from "@/api/types";
import { cn } from "@/lib/utils";

/**
 * The naming rules both identity components obey, so no screen can forget the
 * anti-spoofing rules of proposal 003 section 4: a name is plain text and an id
 * and a hostname are monospace, the id is always beside the name, two entries
 * resolving to the same name both show their full ids, and nothing here sorts,
 * matches or deduplicates on a name.
 */

/** Which source the shown name came from; "id" means there is no name. */
export type NameSource = "profile" | "alias" | "id";

export interface ResolvedName {
  /** null when neither a profile name nor an alias exists: the id is the label. */
  name: string | null;
  source: NameSource;
  /**
   * The nickname this device keeps, when it is not itself the shown name. It is
   * drawn in parentheses after the name, so a public name and the name you gave
   * them are both readable and tellable apart: Alice Ashworth (alice).
   */
  nickname: string | null;
}

/**
 * Resolution order, fixed by proposal 003 section 4: the profile display name,
 * then the local alias or contact nickname, then the id itself.
 */
export function resolveName(identity: ResolvedIdentityDocument): ResolvedName {
  if (identity.display_name) {
    return {
      name: identity.display_name,
      source: "profile",
      nickname: identity.alias || null,
    };
  }
  if (identity.alias) {
    return { name: identity.alias, source: "alias", nickname: null };
  }
  return { name: null, source: "id", nickname: null };
}

/**
 * The name a heading spells for one identity: `Alice Ashworth (alice)` when this
 * device also keeps a nickname, the name alone otherwise, and null when the id is
 * the only label there is.
 */
export function nameWithNickname(identity: ResolvedIdentityDocument): string | null {
  const { name, nickname } = resolveName(identity);
  if (name === null) {
    return null;
  }
  return nickname === null ? name : `${name} (${nickname})`;
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
export function IdentityListScope({
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

/** True when another entry of the same list resolves to this same name. */
export function useSharedName(name: string | null): boolean {
  const duplicates = useContext(DuplicateNameContext);
  return name !== null && duplicates.has(name);
}

/**
 * verified with a stale marker is its own rendered state, never a plain check,
 * and so is a decisive verdict whose latest re-check failed: the node keeps that
 * failure in verification.unreachable beside the older result.
 */
export type VerificationState = VerificationStatus | "stale-verified" | "recheck-failed";

interface Mark {
  glyph: string;
  tone: string;
  sentence: string;
}

const MARKS: Record<Exclude<VerificationState, "unclaimed">, Mark> = {
  verified: {
    glyph: "✓",
    tone: "text-foreground",
    sentence: "HOSTNAME names this identity in its DNS records",
  },
  "stale-verified": {
    glyph: "✓",
    tone: "text-muted-foreground",
    sentence: "HOSTNAME matched more than a day ago, and has not been checked since",
  },
  "recheck-failed": {
    glyph: "?",
    tone: "text-muted-foreground",
    sentence: "the last check of HOSTNAME failed, so this verdict is the one before it",
  },
  mismatched: {
    glyph: "⚠",
    tone: "text-destructive",
    sentence: "HOSTNAME names a different identity in its DNS records",
  },
  unverified: {
    glyph: "○",
    tone: "text-muted-foreground",
    sentence: "HOSTNAME names no identity in its DNS records",
  },
  unchecked: {
    glyph: "○",
    tone: "text-muted-foreground",
    sentence: "HOSTNAME has not been checked from this wallet yet",
  },
  unreachable: {
    glyph: "?",
    tone: "text-muted-foreground",
    sentence: "DNS gave no answer for HOSTNAME",
  },
};

export function verificationState(
  status: VerificationStatus,
  stale: boolean,
  /** True when the node kept a failed re-check beside this verdict. */
  recheckFailed = false,
): VerificationState {
  // A verdict whose latest check failed is not a clean mark, whichever way the
  // older check went: the reader is looking at the answer before the failure.
  if (recheckFailed && (status === "verified" || status === "mismatched")) {
    return "recheck-failed";
  }
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
  recheckFailed = false,
  testId,
}: {
  status: VerificationStatus;
  hostname: string | null;
  stale?: boolean;
  /** True when the node kept a failed re-check beside this verdict. */
  recheckFailed?: boolean;
  testId?: string;
}) {
  if (status === "unclaimed" || hostname === null) {
    return null;
  }
  const state = verificationState(status, stale, recheckFailed);
  const mark = MARKS[state as Exclude<VerificationState, "unclaimed">];
  return (
    <span
      data-testid={testId}
      data-verification={state}
      title={`${state}: ${mark.sentence.replace("HOSTNAME", hostname)}`}
      className={cn("inline-flex items-baseline gap-1 text-xs", mark.tone)}
    >
      <span aria-hidden="true">{mark.glyph}</span>
      <span className="sr-only">{state}</span>
      <span className="font-mono">{hostname}</span>
      {state === "stale-verified" && <span className="italic">may be out of date</span>}
      {state === "recheck-failed" && <span className="italic">last check failed</span>}
    </span>
  );
}

/** The document an id with no name at all renders as: the id is the label. */
export function bareIdentity(identityId: string): ResolvedIdentityDocument {
  return {
    identity_id: identityId,
    display_name: null,
    email: null,
    alias: null,
    hostname: null,
    verification_status: "unclaimed",
    provenance: "none",
  };
}

/**
 * The ResolvedIdentity the node would return for an identity this home holds,
 * built from the identity document so the local screens and the crawled ones
 * render through the same two components.
 */
export function resolvedFrom(identity: Identity): ResolvedIdentityDocument {
  const displayName = identity.profile?.display_name ?? null;
  const alias = identity.contact?.nickname ?? identity.alias;
  return {
    identity_id: identity.identity_id,
    display_name: displayName,
    email: identity.profile?.email ?? null,
    alias,
    hostname: identity.profile?.hostname ?? null,
    verification_status: identity.verification.status,
    provenance: displayName ? "profile" : alias ? "alias" : "none",
  };
}

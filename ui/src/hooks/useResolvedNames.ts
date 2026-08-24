import { useEffect, useState } from "react";

import { lookup } from "@/api/client";
import type { ResolvedIdentity } from "@/api/types";
import { bareIdentity } from "@/components/identity";

export { bareIdentity };

/**
 * How many foreign ids one screen resolves. A trust list is an address book
 * page, not a crawl: past this many the remaining rows render as their ids
 * rather than firing one request each.
 */
export const RESOLVE_LIMIT = 16;

/**
 * What one lookup told a screen about one id: the name to show, and how far away
 * the stored crawl says they are. The distance rides along because the pill
 * needs it and this request was already going out for the name (proposal 005:
 * no request exists for the sake of a pill).
 */
export interface ResolvedEntry {
  resolved: ResolvedIdentity;
  degrees: number | null;
}

export type ResolvedNames = Map<string, ResolvedEntry>;

/**
 * The names a screen shows for foreign ids. `GET /api/lookup` is the one route
 * that names an identity this home does not hold: it answers from the crawl
 * generation, falling back to the local alias or contact nickname, so one
 * request per id gives the same name the lookup screen shows. An id the crawl
 * never reached still answers 200, with no name, and renders as its id.
 */
export function useResolvedNames(identityIds: string[], from: string | null): ResolvedNames {
  const [names, setNames] = useState<ResolvedNames>(new Map());
  // The effect depends on the ids, not on the array identity a render mints.
  const wanted = [...new Set(identityIds)].sort().slice(0, RESOLVE_LIMIT);
  const key = `${from ?? ""}|${wanted.join(",")}`;

  useEffect(() => {
    let live = true;
    if (from === null || wanted.length === 0) {
      return;
    }
    void Promise.all(
      wanted.map(async (identityId): Promise<[string, ResolvedEntry]> => {
        try {
          const answer = await lookup(identityId, { from });
          return [identityId, { resolved: answer.identity, degrees: answer.degrees }];
        } catch {
          return [identityId, { resolved: bareIdentity(identityId), degrees: null }];
        }
      }),
    ).then((resolved) => {
      if (live) {
        setNames(new Map(resolved));
      }
    });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return names;
}

/** The resolved document for one id, or the bare one while it is unknown. */
export function named(names: ResolvedNames, identityId: string): ResolvedIdentity {
  return names.get(identityId)?.resolved ?? bareIdentity(identityId);
}

/** Every distance one screen's lookups reported, which is what the pills read. */
export function degreesOf(names: ResolvedNames): Map<string, number> {
  const degrees = new Map<string, number>();
  for (const [identityId, entry] of names) {
    if (entry.degrees !== null) {
      degrees.set(identityId, entry.degrees);
    }
  }
  return degrees;
}

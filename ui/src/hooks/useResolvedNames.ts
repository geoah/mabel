import { useEffect, useState } from "react";

import { lookup } from "@/api/client";
import type { ResolvedIdentity } from "@/api/types";

/**
 * How many foreign ids one screen resolves. A trust list is an address book
 * page, not a crawl: past this many the remaining rows render as their ids
 * rather than firing one request each.
 */
export const RESOLVE_LIMIT = 16;

/** The row an unresolved id renders as: the id is the label (section 4). */
export function bareIdentity(identityId: string): ResolvedIdentity {
  return {
    identity_id: identityId,
    display_name: null,
    alias: null,
    hostname: null,
    verification_status: "unclaimed",
    provenance: "none",
  };
}

/**
 * The names a screen shows for foreign ids. `GET /api/lookup` is the one route
 * that names an identity this home does not hold: it answers from the crawl
 * generation, falling back to the local alias or contact nickname, so one
 * request per id gives the same name the lookup screen shows. An id the crawl
 * never reached still answers 200, with no name, and renders as its id.
 */
export function useResolvedNames(
  identityIds: string[],
  from: string | null,
): Map<string, ResolvedIdentity> {
  const [names, setNames] = useState<Map<string, ResolvedIdentity>>(new Map());
  // The effect depends on the ids, not on the array identity a render mints.
  const wanted = [...new Set(identityIds)].sort().slice(0, RESOLVE_LIMIT);
  const key = `${from ?? ""}|${wanted.join(",")}`;

  useEffect(() => {
    let live = true;
    if (from === null || wanted.length === 0) {
      return;
    }
    void Promise.all(
      wanted.map(async (identityId) => {
        try {
          return (await lookup(identityId, { from })).identity;
        } catch {
          return bareIdentity(identityId);
        }
      }),
    ).then((resolved) => {
      if (live) {
        setNames(new Map(resolved.map((entry) => [entry.identity_id, entry])));
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
export function named(
  names: Map<string, ResolvedIdentity>,
  identityId: string,
): ResolvedIdentity {
  return names.get(identityId) ?? bareIdentity(identityId);
}

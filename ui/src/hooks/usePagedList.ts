import { useCallback, useEffect, useState } from "react";

import type { ApiError } from "@/api/client";

import { asApiError } from "./useResource";

/** One page of an offset-paged list route, as the hook below reads it. */
export interface Page<T> {
  items: T[];
  /** The route's own `more`: true when the node holds entries past this page. */
  more: boolean;
}

export interface PagedList<T> {
  items: T[];
  error: ApiError | null;
  loading: boolean;
  /** True once a page has arrived, so a screen knows it has a real answer. */
  loaded: boolean;
  /** True when the cap stopped the reading while the node still had more. */
  capped: boolean;
  reload: () => void;
}

interface Settled<T> {
  key: string;
  depsKey: string;
  items: T[];
  loaded: boolean;
  capped: boolean;
  error: ApiError | null;
}

/**
 * Reads every page of an offset-paged list route, one page after another, and
 * stops at a hard cap. These screens draw a list a reader scans, not a table
 * they page through, so honouring `more` here means reading the rest rather than
 * offering a next button; `capped` is true when the cap cut the answer short,
 * and the caller says so on screen instead of pretending the list is whole.
 *
 * A page that fails keeps the pages already read and reports the error beside
 * them, the way useResource does.
 */
export function usePagedList<T>(
  load: (offset: number, limit: number) => Promise<Page<T>>,
  deps: unknown[],
  { pageSize = 256, cap = 1024 }: { pageSize?: number; cap?: number } = {},
): PagedList<T> {
  const [attempt, setAttempt] = useState(0);
  const [settled, setSettled] = useState<Settled<T>>({
    key: "",
    depsKey: "",
    items: [],
    loaded: false,
    capped: false,
    error: null,
  });
  const depsKey = deps.map(String).join("|");
  const key = [attempt, depsKey].join("|");

  const reload = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    let live = true;
    async function readAll(): Promise<{ items: T[]; capped: boolean; error: ApiError | null }> {
      const collected: T[] = [];
      let offset = 0;
      for (;;) {
        let page: Page<T>;
        try {
          page = await load(offset, Math.min(pageSize, cap - collected.length));
        } catch (thrown) {
          // The pages already read are a real, partial answer: they stay, with
          // the failure reported beside them.
          return { items: collected, capped: false, error: asApiError(thrown) };
        }
        collected.push(...page.items);
        offset += page.items.length;
        // A page that answers `more` and no items would loop forever, so the
        // reading stops there too: the cap is the honest report either way.
        if (!page.more || page.items.length === 0 || collected.length >= cap) {
          return { items: collected, capped: page.more && collected.length >= cap, error: null };
        }
      }
    }
    void readAll().then(({ items, capped, error }) => {
      if (!live) {
        return;
      }
      if (error === null) {
        setSettled({ key, depsKey, items, loaded: true, capped, error: null });
        return;
      }
      setSettled((previous) => {
        const kept = previous.depsKey === depsKey;
        const partial = items.length > 0;
        return {
          key,
          depsKey,
          items: partial ? items : kept ? previous.items : [],
          loaded: partial || (kept && previous.loaded),
          capped: partial ? capped : kept && previous.capped,
          error,
        };
      });
    });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return {
    items: settled.items,
    error: settled.error,
    loading: settled.key !== key,
    // Kept across a reload, so a list does not blink out of the page while the
    // next reading is in flight.
    loaded: settled.loaded,
    capped: settled.capped,
    reload,
  };
}

import { useCallback, useSyncExternalStore } from "react";

// Browser-side preferences, all of them under the mabel namespace in
// localStorage. localStorage is per origin and the wallet is served by its own
// node, so "remembered" here means remembered per node home.

/** Proposal 003 section 4 names this key; developer mode is off unless it is "1". */
export const DEVELOPER_MODE_KEY = "mabel.developer_mode";
/** Consent, taken once before the first hostname publication. */
export const HOSTNAME_CONSENT_KEY = "mabel.consent.hostname_publication";
/** Consent, taken once before the first graph sync. */
export const GRAPH_CONSENT_KEY = "mabel.consent.graph_sync";

const listeners = new Set<() => void>();

function announce(): void {
  for (const listener of listeners) {
    listener();
  }
}

/** A storage that throws (private mode, disabled cookies) reads as unset. */
export function readPreference(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writePreference(key: string, value: string | null): void {
  try {
    if (value === null) {
      globalThis.localStorage?.removeItem(key);
    } else {
      globalThis.localStorage?.setItem(key, value);
    }
  } catch {
    // Nothing is remembered, and the session still works.
  }
  announce();
}

/**
 * One subscription for every preference. A write in this tab announces itself,
 * and the storage event carries a write from another tab.
 */
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  globalThis.addEventListener?.("storage", listener);
  return () => {
    listeners.delete(listener);
    globalThis.removeEventListener?.("storage", listener);
  };
}

/** The current value of one key, re-read whenever any preference changes. */
export function usePreference(key: string): [string | null, (value: string | null) => void] {
  const value = useSyncExternalStore(
    subscribe,
    () => readPreference(key),
    () => null,
  );
  const set = useCallback((next: string | null) => writePreference(key, next), [key]);
  return [value, set];
}

/** Default off (decision 014): the primary view is the product, not the panels. */
export function useDeveloperMode(): [boolean, (on: boolean) => void] {
  const [value, set] = usePreference(DEVELOPER_MODE_KEY);
  const setOn = useCallback((on: boolean) => set(on ? "1" : "0"), [set]);
  return [value === "1", setOn];
}

/**
 * A consent taken once and remembered. The panel asking for it states what
 * becomes public or observable, and nothing acts before the answer.
 */
export function useConsent(key: string): [boolean, () => void] {
  const [value, set] = usePreference(key);
  const give = useCallback(() => set("1"), [set]);
  return [value === "1", give];
}

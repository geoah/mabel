import { useCallback, useEffect, useState } from "react";

import { ApiError } from "@/api/client";

export interface Resource<T> {
  data: T | null;
  error: ApiError | null;
  loading: boolean;
  reload: () => void;
}

interface Settled<T> {
  key: string;
  data: T | null;
  error: ApiError | null;
}

/**
 * Runs one GET and keeps its document, its error envelope and a reload handle.
 * The last document stays on screen while the next request is in flight, so a
 * reload after an append does not unmount the panels around it.
 */
export function useResource<T>(load: () => Promise<T>, deps: unknown[]): Resource<T> {
  const [attempt, setAttempt] = useState(0);
  const [settled, setSettled] = useState<Settled<T>>({ key: "", data: null, error: null });
  const key = [attempt, ...deps].map(String).join("|");

  const reload = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    let live = true;
    load()
      .then((document) => {
        if (live) {
          setSettled({ key, data: document, error: null });
        }
      })
      .catch((thrown: unknown) => {
        if (live) {
          setSettled({ key, data: null, error: asApiError(thrown) });
        }
      });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return { data: settled.data, error: settled.error, loading: settled.key !== key, reload };
}

/** Every failure the client throws is an ApiError; anything else is a bug here. */
export function asApiError(thrown: unknown): ApiError {
  if (thrown instanceof ApiError) {
    return thrown;
  }
  return new ApiError(
    {
      ok: false,
      code: 2,
      message: thrown instanceof Error ? thrown.message : String(thrown),
      details: { reason: "client_error" },
    },
    0,
  );
}

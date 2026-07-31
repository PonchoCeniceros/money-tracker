import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

// IPC calls here are local (no network), so refetching everything after
// every mutation is cheap and needs no cache/invalidation library — a
// global revision counter bumped after each write is enough to make every
// `useApi` consumer refetch.
let revision = 0;
const listeners = new Set<() => void>();

export function bumpRevision() {
  revision++;
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getRevision() {
  return revision;
}

export function useRevision() {
  return useSyncExternalStore(subscribe, getRevision);
}

export interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  reload: () => void;
}

/** Fetches `fetcher()` on mount, whenever the global revision bumps (any
 * mutation anywhere), or when `reload()` is called manually. */
export function useApi<T>(fetcher: () => Promise<T>): AsyncState<T> {
  const rev = useRevision();
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [manualTick, setManualTick] = useState(0);

  const reload = useCallback(() => setManualTick((t) => t + 1), []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    fetcher()
      .then((d) => {
        if (!cancelled) setData(d);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // fetcher is intentionally excluded — callers pass a fresh closure each
    // render; re-running only on revision/manual-reload is the point.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rev, manualTick]);

  return { data, loading, error, reload };
}

/** Bumps the revision whenever the window regains focus, so balances
 * refresh if the CLI wrote something while the GUI was in the background. */
export function useRefetchOnFocus() {
  useEffect(() => {
    const handler = () => bumpRevision();
    window.addEventListener("focus", handler);
    return () => window.removeEventListener("focus", handler);
  }, []);
}

import { useEffect, useRef } from "react";
import { syncApi } from "../api/sync";
import { bumpRevision } from "./useApi";

const POLL_MS = 30_000;

/** Runs one `sync_poll` every `POLL_MS`. Only bumps the global revision when
 * the mirror cursor actually advanced — a no-op poll (nothing changed
 * remotely) must not trigger a pointless refetch of every `useApi` consumer.
 * Sync is best-effort on purpose: failures (remote not configured, offline)
 * are swallowed here; the Settings panel surfaces the last warning. */
export function useSync() {
  const lastCursor = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      if (cancelled) return;
      try {
        const result = await syncApi.poll();
        if (!cancelled && result.cursor !== lastCursor.current) {
          lastCursor.current = result.cursor;
          bumpRevision();
        }
      } catch {
        // Not in remote mode, or offline — nothing local to refresh.
      }
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);
}
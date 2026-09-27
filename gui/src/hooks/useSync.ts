import { useEffect, useRef } from "react";
import { syncApi } from "../api/sync";
import { bumpRevision } from "./useApi";

const POLL_MS = 30_000;

/** Polls `ledger_status` every `POLL_MS` and bumps the global revision only when
 * the ledger's revision actually moved (a write from any device, including a
 * delete). A no-op poll must not refetch every `useApi` consumer. Failures
 * (offline, session expired) are swallowed here; the Connect gate and the
 * per-view error banners surface them. */
export function useSync() {
  const lastRevision = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      if (cancelled) return;
      try {
        const { revision } = await syncApi.ledgerStatus();
        if (!cancelled && lastRevision.current !== null && revision !== lastRevision.current) {
          bumpRevision();
        }
        lastRevision.current = revision;
      } catch {
        // Not connected right now — nothing to refresh.
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

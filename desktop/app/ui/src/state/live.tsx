// What every screen shares: the pending reviews, the connection state, and a
// tick that advances on every server event so screens can refetch.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, subscribe } from "../api/client";
import type { Notice, Review } from "../api/types";

type Live = {
  connected: boolean;
  pending: Review[];
  pendingCount: number;
  repositories: string[];
  /** advances on every server event; depend on it to refetch */
  tick: number;
  lastNotice: Notice | null;
  refresh: () => Promise<void>;
};

const LiveContext = createContext<Live | null>(null);

export function LiveProvider({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(false);
  const [pending, setPending] = useState<Review[]>([]);
  const [tick, setTick] = useState(0);
  const [lastNotice, setLastNotice] = useState<Notice | null>(null);

  const refresh = useCallback(async () => {
    try {
      setPending(await api.listReviews({ status: "pending" }));
    } catch {
      // the connection indicator reports the outage; the next event retries
    }
  }, []);

  useEffect(() => {
    refresh();
    return subscribe({
      onOpen: () => {
        setConnected(true);
        refresh();
      },
      onError: () => setConnected(false),
      onNotice: (notice) => {
        setLastNotice(notice);
        setTick((t) => t + 1);
        if (notice.review_id) refresh();
      },
    });
  }, [refresh]);

  const repositories = useMemo(
    () => [...new Set(pending.map((r) => r.origin.repo).filter((r): r is string => !!r))].sort(),
    [pending],
  );

  useEffect(() => {
    const badge = pending.length > 0 ? `(${pending.length}) ` : "";
    document.title = `${badge}wicket`;
  }, [pending.length]);

  const value = useMemo<Live>(
    () => ({ connected, pending, pendingCount: pending.length, repositories, tick, lastNotice, refresh }),
    [connected, pending, repositories, tick, lastNotice, refresh],
  );
  return <LiveContext.Provider value={value}>{children}</LiveContext.Provider>;
}

export function useLive(): Live {
  const live = useContext(LiveContext);
  if (!live) throw new Error("useLive outside LiveProvider");
  return live;
}

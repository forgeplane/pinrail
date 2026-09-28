// What every screen shares: the pending reviews, the connection state, and a
// tick that advances on every server event so screens can refetch.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, subscribe } from "../api/client";
import type { Notice, Plugin, Review } from "../api/types";

type Live = {
  connected: boolean;
  pending: Review[];
  pendingCount: number;
  /** the projects (`origin.repo`) of what is pending, sorted */
  projects: string[];
  /** how many pending reviews name no project */
  unassigned: number;
  /** advances on every server event; depend on it to refetch */
  tick: number;
  lastNotice: Notice | null;
  /** the registered plugins, by name */
  plugins: Map<string, Plugin>;
  /** the icon a plugin declares, if any */
  pluginIcon: (name: string) => string | null;
  refresh: () => Promise<void>;
};

const LiveContext = createContext<Live | null>(null);

export function LiveProvider({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(false);
  const [pending, setPending] = useState<Review[]>([]);
  const [tick, setTick] = useState(0);
  const [lastNotice, setLastNotice] = useState<Notice | null>(null);
  const [plugins, setPlugins] = useState<Map<string, Plugin>>(new Map());

  const loadPlugins = useCallback(async () => {
    try {
      const { plugins } = await api.plugins();
      setPlugins(new Map(plugins.map((p) => [p.name, p])));
    } catch {
      // the list stays as it was; the next reload event retries
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      // every pending review, up to the server's maximum: the sidebar, the
      // project counts and the inbox's pages are all drawn from this list
      setPending((await api.listReviews({ status: "pending", limit: "500" })).reviews);
    } catch {
      // the connection indicator reports the outage; the next event retries
    }
  }, []);

  useEffect(() => {
    refresh();
    loadPlugins();
    return subscribe({
      onOpen: () => {
        setConnected(true);
        refresh();
        loadPlugins();
      },
      onError: () => setConnected(false),
      onNotice: (notice) => {
        setLastNotice(notice);
        setTick((t) => t + 1);
        if (notice.review_id) refresh();
        if (notice.kind === "plugins_reloaded") loadPlugins();
      },
    });
  }, [refresh, loadPlugins]);

  const projects = useMemo(
    () => [...new Set(pending.map((r) => r.origin.repo).filter((r): r is string => !!r))].sort(),
    [pending],
  );
  const unassigned = useMemo(() => pending.filter((r) => !r.origin.repo).length, [pending]);

  useEffect(() => {
    const badge = pending.length > 0 ? `(${pending.length}) ` : "";
    document.title = `${badge}Pinrail`;
  }, [pending.length]);

  const pluginIcon = useCallback((name: string) => plugins.get(name)?.icon ?? null, [plugins]);
  const value = useMemo<Live>(
    () => ({
      connected,
      pending,
      pendingCount: pending.length,
      projects,
      unassigned,
      tick,
      lastNotice,
      plugins,
      pluginIcon,
      refresh,
    }),
    [connected, pending, projects, unassigned, tick, lastNotice, plugins, pluginIcon, refresh],
  );
  return <LiveContext.Provider value={value}>{children}</LiveContext.Provider>;
}

export function useLive(): Live {
  const live = useContext(LiveContext);
  if (!live) throw new Error("useLive outside LiveProvider");
  return live;
}

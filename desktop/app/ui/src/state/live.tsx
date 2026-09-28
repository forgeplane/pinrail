// What every screen shares: the pending reviews, kept up to date from the
// server's events, the connection state, the last event for screens that
// follow one review, and the installed plugins.

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
  /** advances when a review ends or old history is deleted: what the
   *  history lists has changed */
  historyVersion: number;
  lastNotice: Notice | null;
  /** the registered plugins, by name */
  plugins: Map<string, Plugin>;
  /** the icon a plugin declares, if any */
  pluginIcon: (name: string) => string | null;
  refresh: () => Promise<void>;
};

type Plugins = Pick<Live, "plugins" | "pluginIcon">;

const LiveContext = createContext<Live | null>(null);
// the plugins alone, which change far less often than the rest: a badge
// that only shows an icon does not redraw on every event
const PluginsContext = createContext<Plugins | null>(null);

/** Events after which a review is no longer pending. */
const ENDINGS = new Set(["decided", "withdrawn", "discarded", "expired"]);

/**
 * The pending list after an event about one review. The event carries the
 * review as it now stands, so the list changes without asking the server:
 * a review that ends leaves it, a new one joins it in its place (newest
 * first, as the server lists them), and any other change replaces it.
 */
export function applyNotice(pending: Review[], notice: Notice): Review[] {
  const id = notice.review_id;
  if (!id) return pending;
  const listed = pending.some((r) => r.id === id);
  const review = notice.review;
  if (ENDINGS.has(notice.kind) || (review && review.status !== "pending")) {
    return listed ? pending.filter((r) => r.id !== id) : pending;
  }
  if (!review) return pending;
  if (listed) return pending.map((r) => (r.id === id ? review : r));
  if (notice.kind !== "created") return pending;
  return [review, ...pending].sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}

export function LiveProvider({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(false);
  const [pending, setPending] = useState<Review[]>([]);
  const [historyVersion, setHistoryVersion] = useState(0);
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
        if (notice.review_id) setPending((pending) => applyNotice(pending, notice));
        if (ENDINGS.has(notice.kind) || notice.kind === "history_swept") setHistoryVersion((v) => v + 1);
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
      historyVersion,
      lastNotice,
      plugins,
      pluginIcon,
      refresh,
    }),
    [connected, pending, projects, unassigned, historyVersion, lastNotice, plugins, pluginIcon, refresh],
  );
  const pluginsValue = useMemo<Plugins>(() => ({ plugins, pluginIcon }), [plugins, pluginIcon]);
  return (
    <LiveContext.Provider value={value}>
      <PluginsContext.Provider value={pluginsValue}>{children}</PluginsContext.Provider>
    </LiveContext.Provider>
  );
}

export function useLive(): Live {
  const live = useContext(LiveContext);
  if (!live) throw new Error("useLive outside LiveProvider");
  return live;
}

/** The installed plugins alone, for what needs nothing else from the server. */
export function usePlugins(): Plugins {
  const plugins = useContext(PluginsContext);
  if (!plugins) throw new Error("usePlugins outside LiveProvider");
  return plugins;
}

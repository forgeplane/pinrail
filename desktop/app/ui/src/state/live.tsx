// What every screen shares: the pending reviews, kept up to date from the
// server's events, the connection state, the last event for screens that
// follow one review, and the installed plugins.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, subscribe } from "../api/client";
import type { Notice, Plugin, ProjectRef, Review } from "../api/types";
import { ENDINGS, applyNotice } from "./pending";

/** The most pending reviews the app loads; the server has the rest. */
const PENDING_LOADED = 500;

type Live = {
  connected: boolean;
  pending: Review[];
  /** every pending review, including those beyond the ones loaded */
  pendingCount: number;
  /** pending reviews the server has beyond the newest ones loaded */
  pendingUnloaded: number;
  /** the projects (`origin.repo`) of what is pending, sorted */
  projects: string[];
  /** how many pending reviews name no project */
  unassigned: number;
  /** advances when a review ends or old history is deleted: what the
   *  history lists has changed */
  historyVersion: number;
  /** advances when a project changes: what names or groups by projects
   *  must read them again */
  projectsVersion: number;
  /** the person's projects, by name, for the filters */
  namedProjects: ProjectRef[];
  lastNotice: Notice | null;
  /** the registered plugins, by name */
  plugins: Map<string, Plugin>;
  /** the icon a plugin declares, if any */
  pluginIcon: (name: string) => string | null;
};

type Plugins = Pick<Live, "plugins" | "pluginIcon">;

const LiveContext = createContext<Live | null>(null);
// the plugins alone, which change far less often than the rest: a badge
// that only shows an icon does not redraw on every event
const PluginsContext = createContext<Plugins | null>(null);

export function LiveProvider({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(false);
  const [pending, setPending] = useState<Review[]>([]);
  const [unloaded, setUnloaded] = useState(0);
  const [historyVersion, setHistoryVersion] = useState(0);
  const [projectsVersion, setProjectsVersion] = useState(0);
  const [namedProjects, setNamedProjects] = useState<ProjectRef[]>([]);
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

  // A review created while a request for the pending list is in progress
  // is missing from the response. Each request therefore records the
  // events that arrive while it waits and applies them to the response
  // before showing it. When requests overlap, the response to an earlier
  // request is discarded if a later one has already been shown.
  const fetching = useRef(new Set<Notice[]>());
  const asked = useRef(0);
  const shown = useRef(0);
  const refresh = useCallback(async () => {
    const missed: Notice[] = [];
    const turn = ++asked.current;
    fetching.current.add(missed);
    try {
      // every pending review, up to the server's maximum: the sidebar, the
      // project counts and the inbox's pages are all drawn from this list
      const listing = await api.listReviews({ status: "pending", limit: String(PENDING_LOADED) });
      if (turn < shown.current) return;
      shown.current = turn;
      setPending(missed.reduce(applyNotice, listing.reviews));
      setUnloaded(Math.max(0, (listing.total ?? listing.reviews.length) - listing.reviews.length));
    } catch {
      // the connection indicator reports the outage; the next event retries
    } finally {
      fetching.current.delete(missed);
    }
  }, []);

  // the projects, read again whenever one changes
  useEffect(() => {
    let cancelled = false;
    api
      .projects()
      .then(
        (l) =>
          !cancelled && setNamedProjects(l.projects.map(({ id, name, color, emoji }) => ({ id, name, color, emoji }))),
      )
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [projectsVersion, connected]);

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
        for (const missed of fetching.current) missed.push(notice);
        if (notice.review_id) setPending((pending) => applyNotice(pending, notice));
        if (ENDINGS.has(notice.kind) || notice.kind === "history_swept") setHistoryVersion((v) => v + 1);
        // a star on one round marks the whole review, in the inbox and the history
        if (notice.kind === "starred" || notice.kind === "unstarred") {
          setHistoryVersion((v) => v + 1);
          refresh();
        }
        // the pending reviews name their project, which may have changed
        if (notice.kind === "projects_changed") {
          setProjectsVersion((v) => v + 1);
          setHistoryVersion((v) => v + 1);
          refresh();
        }
        // a review that moved may have taken a linked folder as it is now
        if (notice.kind === "plugins_reloaded" || notice.kind === "plugin_changed") loadPlugins();
      },
    });
  }, [refresh, loadPlugins]);

  const projects = useMemo(
    () => [...new Set(pending.map((r) => r.origin.repo).filter((r): r is string => !!r))].sort(),
    [pending],
  );
  const unassigned = useMemo(() => pending.filter((r) => !r.origin.repo).length, [pending]);

  useEffect(() => {
    const count = pending.length + unloaded;
    const badge = count > 0 ? `(${count}) ` : "";
    document.title = `${badge}Pinrail`;
  }, [pending.length, unloaded]);

  const pluginIcon = useCallback((name: string) => plugins.get(name)?.icon ?? null, [plugins]);
  const value = useMemo<Live>(
    () => ({
      connected,
      pending,
      pendingCount: pending.length + unloaded,
      pendingUnloaded: unloaded,
      projects,
      unassigned,
      historyVersion,
      projectsVersion,
      namedProjects,
      lastNotice,
      plugins,
      pluginIcon,
    }),
    [
      connected,
      pending,
      unloaded,
      projects,
      unassigned,
      historyVersion,
      projectsVersion,
      namedProjects,
      lastNotice,
      plugins,
      pluginIcon,
    ],
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

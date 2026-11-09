// What every screen shares: the pending reviews, kept up to date from the
// server's events, the connection state, the installed plugins, and the
// events themselves, which a screen listens to with useNotice.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  useEffectEvent,
} from "react";
import { api, subscribe } from "../api/client";
import type { Notice, Plugin, ProjectRef, ProjectsListing, Review } from "../api/types";
import { setItemPrefixes } from "../lib/markdown";
import { ENDINGS, applyNotice } from "./pending";
import { useLoad } from "../lib/useLoad";

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
  /** the projects with their repositories, and the repositories in none */
  projectsListing: ProjectsListing | null;
  /** how many work items are open, in progress or on hold */
  openItems: number;
  /** how many of them are in progress */
  itemsInProgress: number;
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
// whether the server is connected, alone: a send button needs nothing else
const ConnectedContext = createContext(false);
// the events, delivered to listeners: a screen renders only for the events
// it acts on, not for every status an agent reports
type Listen = (listener: (notice: Notice) => void) => () => void;
const NoticesContext = createContext<Listen | null>(null);

export function LiveProvider({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(false);
  const [pending, setPending] = useState<Review[]>([]);
  const [unloaded, setUnloaded] = useState(0);
  const [historyVersion, setHistoryVersion] = useState(0);
  const [projectsVersion, setProjectsVersion] = useState(0);
  const [projectsListing, setProjectsListing] = useState<ProjectsListing | null>(null);
  // advances when an item changes, and when one names a key prefix not yet
  // known: a repository gets its prefix with its first item
  const [itemsVersion, setItemsVersion] = useState(0);
  const [prefixesVersion, setPrefixesVersion] = useState(0);
  const prefixes = useRef(new Set<string>());
  const listeners = useRef(new Set<(notice: Notice) => void>());
  const listen = useCallback<Listen>((listener) => {
    listeners.current.add(listener);
    return () => listeners.current.delete(listener);
  }, []);
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

  // the projects, read once for every screen, again whenever one changes;
  // their repositories' key prefixes are what links item keys in Markdown
  useEffect(() => {
    let cancelled = false;
    api
      .projects()
      .then((listing) => {
        if (cancelled) return;
        setProjectsListing(listing);
        const known = [...listing.projects.flatMap((p) => p.repos), ...listing.loose]
          .map((r) => r.prefix)
          .filter((p): p is string => !!p);
        prefixes.current = new Set(known);
        setItemPrefixes(known);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [projectsVersion, prefixesVersion, connected]);
  const namedProjects = useMemo<ProjectRef[]>(
    () =>
      (projectsListing?.projects ?? []).map(({ id, name, color, emoji, image }) => ({ id, name, color, emoji, image })),
    [projectsListing],
  );

  // how many work items are open, and how many of them in progress,
  // beside their entry in the sidebar
  const itemCounts = useLoad(
    () =>
      api.items().then((d) => ({
        open: d.items.length,
        inProgress: d.items.filter((i) => i.status === "in_progress").length,
      })),
    [itemsVersion, connected],
  ).data;
  const openItems = itemCounts?.open ?? 0;
  const itemsInProgress = itemCounts?.inProgress ?? 0;

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- loads from the core
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
        for (const listener of listeners.current) listener(notice);
        for (const missed of fetching.current) missed.push(notice);
        if (notice.review_id) setPending((pending) => applyNotice(pending, notice));
        if (ENDINGS.has(notice.kind) || notice.kind === "history_swept") setHistoryVersion((v) => v + 1);
        // a deleted review leaves the inbox and the history
        if (notice.kind === "review_deleted") {
          setHistoryVersion((v) => v + 1);
          refresh();
        }
        // a star on one round marks the whole review, in the inbox and the history
        if (notice.kind === "starred" || notice.kind === "unstarred") {
          setHistoryVersion((v) => v + 1);
          refresh();
        }
        // the pending reviews name their project, which may have changed;
        // a deleted repository leaves the lists and its project
        if (notice.kind === "projects_changed" || notice.kind === "repository_deleted") {
          setProjectsVersion((v) => v + 1);
          setHistoryVersion((v) => v + 1);
          refresh();
        }
        if (notice.item) {
          setItemsVersion((v) => v + 1);
          const prefix = notice.item.key.replace(/-\d+$/, "");
          if (!prefixes.current.has(prefix)) setPrefixesVersion((v) => v + 1);
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
      projectsListing,
      openItems,
      itemsInProgress,
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
      projectsListing,
      openItems,
      itemsInProgress,
      plugins,
      pluginIcon,
    ],
  );
  const pluginsValue = useMemo<Plugins>(() => ({ plugins, pluginIcon }), [plugins, pluginIcon]);
  return (
    <NoticesContext.Provider value={listen}>
      <ConnectedContext.Provider value={connected}>
        <LiveContext.Provider value={value}>
          <PluginsContext.Provider value={pluginsValue}>{children}</PluginsContext.Provider>
        </LiveContext.Provider>
      </ConnectedContext.Provider>
    </NoticesContext.Provider>
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

/** Whether the server is connected, for what needs nothing else. */
export function useConnected(): boolean {
  return useContext(ConnectedContext);
}

/** Calls `handler` with each event from the server, as it arrives. The
 *  latest `handler` is called, so it can read the component's state. */
export function useNotice(handler: (notice: Notice) => void) {
  const listen = useContext(NoticesContext);
  if (!listen) throw new Error("useNotice outside LiveProvider");
  const onNotice = useEffectEvent(handler);
  useEffect(() => listen((notice) => onNotice(notice)), [listen]);
}

/** A number that goes up with each live notice `about` keeps: an effect that
 *  depends on it loads its data again for those notices alone. */
export function useNoticeCount(about: (notice: Notice) => boolean): number {
  const [count, setCount] = useState(0);
  useNotice((notice) => {
    if (about(notice)) setCount((n) => n + 1);
  });
  return count;
}

/** Whether a notice can change what a list of work items shows: a change
 *  to an item, a review of items, a session archived or restored, which
 *  gives up or keeps its claims, a history sweep, which releases the
 *  claims of the sessions it deletes, or a change to the projects. */
export const aboutItems = (notice: Notice) =>
  !!notice.item ||
  !!notice.review?.items?.length ||
  ["session_archived", "session_unarchived", "history_swept", "projects_changed", "repository_deleted"].includes(
    notice.kind,
  );

/** Whether a notice is about a work item itself. */
export const anItem = (notice: Notice) => !!notice.item;

/** Whether a notice can change what a list of sessions shows: a review, a
 *  session's status, message or archive, or a change to the projects. */
export const aboutSessions = (notice: Notice) =>
  !!notice.review_id ||
  notice.kind.startsWith("session_") ||
  ["projects_changed", "repository_deleted"].includes(notice.kind);

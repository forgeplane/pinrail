// The settings, from the core's settings.json through the API: read at
// start, applied as they arrive, changed with a PATCH, and followed live
// when something else changes them (the CLI, a hand edit, the tray). The
// last appearance is cached in localStorage for the first paint only.
// Launch-at-login is the app's own, through a command.

import type { Ordering } from "../lib/itemOrder";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, inTauri } from "../api/client";
import type { LinkPermission, ServerSettings, ThemeEntry } from "../api/types";
import { DEFAULT_GLOBAL_SHORTCUT } from "../lib/shortcuts";
import { type Choices, DEFAULT_CHOICES } from "../lib/sidebarSessions";
import { cacheAppearance, setThemeOverlays, setThemePreference, type ThemePreference } from "../lib/theme";
import { chosenTheme } from "../lib/themes";
import { useNotice, useConnected } from "./live";
import { useLatest } from "../lib/useLatest";

export type TextSize = "small" | "default" | "large";

export type Settings = {
  appearance: { theme: ThemePreference; dark_theme: string; light_theme: string; text_size: TextSize };
  /** whether the sidebar is open, and how it lists the sessions */
  sidebar: { open: boolean; sessions: Choices & { collapsed: string[] } };
  close_window: "hide" | "quit";
  menu_bar_icon: boolean;
  notifications: { enabled: boolean; paused_until: string | null; sound: boolean; muted_plugins: string[] };
  shortcut: { global: string; global_opens: "oldest" | "inbox" };
  /** where the inbox opens a review: in the side panel, or on its own page */
  inbox: { open_in: "panel" | "page"; layout: "project" | "repository" | "list" };
  sessions: { layout: "project" | "repository" | "list" };
  /** how the Work items page lists the items, and shows sub-items */
  items: {
    layout: "project" | "repository" | "list";
    nesting: "nested" | "flat";
    ordering: Ordering;
    /** whether a parent closes when its last open sub-item does */
    close_parents: boolean;
  };
  /** each plugin's own settings, only the values someone changed */
  plugins: Record<string, Record<string, unknown>>;
  /** each plugin's permission to open links without asking */
  links: Record<string, LinkPermission>;
  /** the loopback server's port; applies at the next start */
  port: number;
  /** how long ended reviews are kept, in days; null keeps them forever */
  history: { keep_days: number | null };
  /** look for a new version at start and every few hours */
  updates: { check: boolean };
  /** update the connected agents' skills when the app starts */
  agents: { auto_update: boolean };
  /** features to try before they are finished, each off until turned on */
  labs: { sessions: boolean };
  /** the welcome screen was closed; until then the app opens on it */
  welcome: { seen: boolean };
  /** the release whose notes were last shown, or null if none has been */
  whats_new: { seen: string | null };
  /** anonymous usage data and crash reports, and when the notice about
   *  them was shown; nothing is recorded before it was */
  telemetry: { usage: boolean; crashes: boolean; notice_shown: string | null };
  /** launch at login; null when the app cannot say (a browser) */
  autostart: boolean | null;
};

type Patch = {
  appearance?: Partial<Settings["appearance"]>;
  sidebar?: { open?: boolean; sessions?: Partial<Settings["sidebar"]["sessions"]> };
  close_window?: Settings["close_window"];
  menu_bar_icon?: boolean;
  notifications?: Partial<Settings["notifications"]>;
  shortcut?: Partial<Settings["shortcut"]>;
  inbox?: Partial<Settings["inbox"]>;
  sessions?: Partial<Settings["sessions"]>;
  items?: Partial<Settings["items"]>;
  /** a change to one or more plugins' settings, merged key by key */
  plugins?: Record<string, Record<string, unknown>>;
  /** a plugin's link permission replaced, or null to forget it */
  links?: Record<string, LinkPermission | null>;
  port?: number;
  history?: Partial<Settings["history"]>;
  updates?: Partial<Settings["updates"]>;
  agents?: Partial<Settings["agents"]>;
  labs?: Partial<Settings["labs"]>;
  welcome?: Partial<Settings["welcome"]>;
  whats_new?: Partial<Settings["whats_new"]>;
  telemetry?: Partial<Settings["telemetry"]>;
  autostart?: boolean;
};

type Store = {
  settings: Settings;
  /** every theme the core lists */
  themes: ThemeEntry[];
  /** Changes the settings. A function builds the change from the settings
   *  as last applied, which a toggle needs: two presses before a redraw
   *  would otherwise both read the same value. */
  update: (patch: Patch | ((settings: Settings) => Patch)) => Promise<void>;
  native: boolean;
  loaded: boolean;
};

const ZOOM: Record<TextSize, string> = { small: "0.92", default: "1", large: "1.08" };

const SettingsContext = createContext<Store | null>(null);

export function applyTextSize(size: TextSize) {
  const root = document.getElementById("root");
  if (root) (root.style as CSSStyleDeclaration & { zoom: string }).zoom = ZOOM[size];
}

type Served = Pick<
  Settings,
  | "appearance"
  | "sidebar"
  | "close_window"
  | "menu_bar_icon"
  | "notifications"
  | "shortcut"
  | "inbox"
  | "sessions"
  | "items"
  | "plugins"
  | "links"
  | "port"
  | "history"
  | "updates"
  | "agents"
  | "labs"
  | "welcome"
  | "whats_new"
  | "telemetry"
>;
/** The sidebar's sessions as they are listed until the person changes it. */
const SESSIONS: Settings["sidebar"]["sessions"] = { ...DEFAULT_CHOICES, collapsed: [] };

/** The settings as the core answers them, with the defaults of what it
 *  leaves out. */
export const fromServer = (s: ServerSettings): Served => ({
  appearance: {
    theme: s.appearance.theme,
    dark_theme: s.appearance.dark_theme,
    light_theme: s.appearance.light_theme,
    text_size: s.appearance.text_size,
  },
  sidebar: { open: s.sidebar.open, sessions: { ...SESSIONS, ...s.sidebar.sessions } },
  close_window: s.close_window,
  menu_bar_icon: s.menu_bar_icon,
  notifications: {
    enabled: s.notifications.enabled,
    paused_until: s.notifications.paused_until,
    sound: s.notifications.sound,
    muted_plugins: s.notifications.muted_plugins,
  },
  shortcut: { global: s.shortcut.global, global_opens: s.shortcut.global_opens },
  inbox: { open_in: s.inbox?.open_in ?? "panel", layout: s.inbox?.layout ?? "repository" },
  sessions: { layout: s.sessions?.layout ?? "list" },
  items: {
    layout: s.items?.layout ?? "project",
    nesting: s.items?.nesting ?? "nested",
    ordering: s.items?.ordering ?? "oldest",
    close_parents: s.items?.close_parents ?? true,
  },
  plugins: s.plugins ?? {},
  links: s.links ?? {},
  port: s.port,
  history: { keep_days: s.history?.keep_days ?? null },
  updates: { check: s.updates?.check ?? true },
  agents: { auto_update: s.agents?.auto_update ?? false },
  labs: { sessions: s.labs?.sessions ?? false },
  welcome: { seen: s.welcome?.seen ?? false },
  whats_new: { seen: s.whats_new?.seen ?? null },
  telemetry: {
    usage: s.telemetry?.usage ?? true,
    crashes: s.telemetry?.crashes ?? true,
    notice_shown: s.telemetry?.notice_shown ?? null,
  },
});

const mergeLinks = (current: Settings["links"], patch?: Patch["links"]): Settings["links"] => {
  if (!patch) return current;
  const out = { ...current };
  for (const [name, entry] of Object.entries(patch)) {
    if (entry) out[name] = entry;
    else delete out[name];
  }
  return out;
};

const mergePlugins = (current: Settings["plugins"], patch?: Settings["plugins"]): Settings["plugins"] => {
  if (!patch) return current;
  const out = { ...current };
  for (const [name, values] of Object.entries(patch)) out[name] = { ...current[name], ...values };
  return out;
};

/** The settings with a patch applied: each section merged key by key, a
 *  plugin's settings per plugin, and a link set to null removed. */
export function mergeSettings(base: Served, patch: Patch): Served {
  return {
    appearance: { ...base.appearance, ...patch.appearance },
    sidebar: {
      open: patch.sidebar?.open ?? base.sidebar.open,
      sessions: { ...base.sidebar.sessions, ...patch.sidebar?.sessions },
    },
    close_window: patch.close_window ?? base.close_window,
    menu_bar_icon: patch.menu_bar_icon ?? base.menu_bar_icon,
    notifications: { ...base.notifications, ...patch.notifications },
    shortcut: { ...base.shortcut, ...patch.shortcut },
    inbox: { ...base.inbox, ...patch.inbox },
    sessions: { ...base.sessions, ...patch.sessions },
    items: { ...base.items, ...patch.items },
    plugins: mergePlugins(base.plugins, patch.plugins),
    links: mergeLinks(base.links, patch.links),
    port: patch.port ?? base.port,
    history: { ...base.history, ...patch.history },
    updates: { ...base.updates, ...patch.updates },
    agents: { ...base.agents, ...patch.agents },
    labs: { ...base.labs, ...patch.labs },
    welcome: { ...base.welcome, ...patch.welcome },
    whats_new: { ...base.whats_new, ...patch.whats_new },
    telemetry: { ...base.telemetry, ...patch.telemetry },
  };
}

/** What a patch sends to the core: only the sections it changes. */
export function patchBody(patch: Patch): Record<string, unknown> {
  const body: Record<string, unknown> = {};
  if (patch.appearance) body.appearance = patch.appearance;
  if (patch.sidebar) body.sidebar = patch.sidebar;
  if (patch.close_window !== undefined) body.close_window = patch.close_window;
  if (patch.menu_bar_icon !== undefined) body.menu_bar_icon = patch.menu_bar_icon;
  if (patch.notifications) body.notifications = patch.notifications;
  if (patch.shortcut) body.shortcut = patch.shortcut;
  if (patch.inbox) body.inbox = patch.inbox;
  if (patch.sessions) body.sessions = patch.sessions;
  if (patch.items) body.items = patch.items;
  if (patch.plugins) body.plugins = patch.plugins;
  if (patch.links) body.links = patch.links;
  if (patch.port !== undefined) body.port = patch.port;
  if (patch.history) body.history = patch.history;
  if (patch.updates) body.updates = patch.updates;
  if (patch.agents) body.agents = patch.agents;
  if (patch.labs) body.labs = patch.labs;
  if (patch.welcome) body.welcome = patch.welcome;
  if (patch.whats_new) body.whats_new = patch.whats_new;
  if (patch.telemetry) body.telemetry = patch.telemetry;
  return body;
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const native = inTauri();
  const connected = useConnected();
  const [settings, setSettings] = useState<Settings>(() => ({
    appearance: { theme: "system", dark_theme: "dark", light_theme: "light", text_size: "default" },
    sidebar: { open: true, sessions: SESSIONS },
    close_window: "hide",
    menu_bar_icon: true,
    notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
    shortcut: { global: DEFAULT_GLOBAL_SHORTCUT, global_opens: "oldest" },
    inbox: { open_in: "panel", layout: "repository" },
    sessions: { layout: "list" },
    items: { layout: "project", nesting: "nested", ordering: "oldest", close_parents: true },
    plugins: {},
    links: {},
    port: 4747,
    history: { keep_days: null },
    updates: { check: true },
    agents: { auto_update: false },
    labs: { sessions: false },
    welcome: { seen: false },
    whats_new: { seen: null },
    telemetry: { usage: true, crashes: true, notice_shown: null },
    autostart: null,
  }));
  const [loaded, setLoaded] = useState(false);
  const [themes, setThemes] = useState<ThemeEntry[]>([]);
  const themesLoaded = useRef(false);
  // the settings as last applied, ahead of the next render: a change made
  // before React renders the previous one builds on it, not on a stale copy
  const settingsRef = useLatest(settings);
  // the latest change sent: an older answer that lands after it is dropped
  const sent = useRef(0);
  // the changes sent and not yet answered
  const unanswered = useRef(0);
  // a reload was skipped while a change was unanswered: the core may have
  // taken a change from elsewhere after it answered ours
  const stale = useRef(false);
  // the last change sent: each waits for the one before it, so the core
  // stores them in the order they were made
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const applied = useRef<Pick<Settings, "appearance"> | null>(null);

  const apply = useCallback((s: Pick<Settings, "appearance">) => {
    if (applied.current?.appearance.theme !== s.appearance.theme) setThemePreference(s.appearance.theme);
    if (applied.current?.appearance.text_size !== s.appearance.text_size) applyTextSize(s.appearance.text_size);
    applied.current = s;
    cacheAppearance(s.appearance);
  }, []);

  const load = useCallback(
    async function load(): Promise<void> {
      // A reload that overlaps a change of ours may have read the settings
      // from before it, and would undo it; it runs again once every change
      // is answered.
      const before = sent.current;
      const overlapped = () => unanswered.current > 0 || sent.current !== before;
      try {
        if (overlapped()) {
          stale.current = true;
          return;
        }
        const s = fromServer(await api.settings());
        if (overlapped()) {
          // a change sent and answered meanwhile has nothing left to rerun it
          if (unanswered.current === 0) void load();
          else stale.current = true;
          return;
        }
        apply(s);
        // a change made before the next redraw builds on these, not on the
        // defaults the provider started with
        settingsRef.current = { ...settingsRef.current, ...s };
        setSettings((prev) => ({ ...prev, ...s }));
        setLoaded(true);
      } catch {
        // the server is away; the cached appearance stands until it is back
      }
    },
    [apply, settingsRef],
  );

  useEffect(() => {
    load();
  }, [load]);

  // the latest request for the themes: an answer to an earlier one that
  // arrives after it is dropped
  const themesAsked = useRef(0);
  const loadThemes = useCallback(async () => {
    const asked = ++themesAsked.current;
    try {
      const { themes } = await api.themes();
      if (asked !== themesAsked.current) return;
      setThemes(themes);
      themesLoaded.current = true;
    } catch {
      // the server is away; the cached colours stand until it is back
    }
  }, []);

  useEffect(() => {
    loadThemes();
  }, [loadThemes]);
  useNotice((notice) => {
    if (notice.kind === "themes_changed") loadThemes();
  });
  useEffect(() => {
    if (connected) loadThemes();
  }, [connected, loadThemes]);

  const { dark_theme: darkTheme, light_theme: lightTheme } = settings.appearance;
  useEffect(() => {
    if (!themesLoaded.current || !loaded) return;
    const overlay = (id: string, scheme: "dark" | "light") => {
      const theme = chosenTheme(themes, id, scheme);
      return { shell: theme?.shell ?? {}, plugin: theme?.plugin ?? {} };
    };
    setThemeOverlays({ dark: overlay(darkTheme, "dark"), light: overlay(lightTheme, "light") });
  }, [themes, darkTheme, lightTheme, loaded]);

  // a change made elsewhere — the CLI, the file, another window
  useNotice((notice) => {
    if (notice.kind === "settings_changed") load();
  });
  useEffect(() => {
    if (connected) load();
  }, [connected, load]);

  // what the app knows: launch at login
  useEffect(() => {
    if (!native) return;
    let cancelled = false;
    import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke<boolean>("autostart_enabled")
        .then((autostart) => !cancelled && setSettings((s) => ({ ...s, autostart })))
        .catch(() => {}),
    );
    return () => {
      cancelled = true;
    };
  }, [native]);

  const update = useCallback(
    async (change: Patch | ((settings: Settings) => Patch)) => {
      const patch = typeof change === "function" ? change(settingsRef.current) : change;
      // the core's settings go to the core; applied at once, confirmed by the response
      if (
        patch.appearance ||
        patch.sidebar ||
        patch.close_window !== undefined ||
        patch.menu_bar_icon !== undefined ||
        patch.notifications ||
        patch.shortcut ||
        patch.inbox ||
        patch.sessions ||
        patch.items ||
        patch.plugins ||
        patch.links ||
        patch.port !== undefined ||
        patch.history ||
        patch.updates ||
        patch.agents ||
        patch.labs ||
        patch.welcome ||
        patch.whats_new ||
        patch.telemetry
      ) {
        const base = settingsRef.current;
        const next = mergeSettings(base, patch);
        apply(next);
        settingsRef.current = { ...base, ...next };
        setSettings((s) => ({ ...s, ...next }));
        const body = patchBody(patch);
        const mine = ++sent.current;
        unanswered.current++;
        try {
          const request = queue.current.then(() => api.patchSettings(body));
          queue.current = request.catch(() => {});
          const s = fromServer(await request);
          unanswered.current--;
          if (mine === sent.current) {
            apply(s);
            settingsRef.current = { ...settingsRef.current, ...s };
            setSettings((prev) => ({ ...prev, ...s }));
          }
        } catch {
          unanswered.current--;
          // refused or the server is away: what was applied stays for this session
        }
        if (unanswered.current === 0 && stale.current) {
          stale.current = false;
          void load();
        }
      }
      if (native && patch.autostart !== undefined) {
        const { invoke } = await import("@tauri-apps/api/core");
        try {
          await invoke("set_autostart", { enabled: patch.autostart });
          setSettings((s) => ({ ...s, autostart: patch.autostart ?? s.autostart }));
        } catch (error) {
          // the toggle stays as the system has it
          console.error("launch at login could not be changed", error);
        }
      }
    },
    [settingsRef, native, apply, load],
  );

  const value = useMemo(
    () => ({ settings, themes, update, native, loaded }),
    [settings, themes, update, native, loaded],
  );
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Store {
  const store = useContext(SettingsContext);
  if (!store) throw new Error("useSettings outside SettingsProvider");
  return store;
}

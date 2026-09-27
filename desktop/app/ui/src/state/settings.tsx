// The settings, from the core's settings.json through the API: read at
// start, applied as they arrive, changed with a PATCH, and followed live
// when something else changes them (the CLI, a hand edit, the tray). The
// last appearance is cached in localStorage for the first paint only.
// Launch-at-login is the app's own, through a command.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, inTauri } from "../api/client";
import type { LinkPermission, ServerSettings } from "../api/types";
import { DEFAULT_GLOBAL_SHORTCUT } from "../lib/shortcuts";
import { cacheAppearance, setThemePreference, type ThemePreference } from "../lib/theme";
import { useLive } from "./live";

export type TextSize = "small" | "default" | "large";

export type Settings = {
  appearance: { theme: ThemePreference; text_size: TextSize };
  sidebar: { open: boolean };
  close_window: "hide" | "quit";
  menu_bar_icon: boolean;
  notifications: { enabled: boolean; paused_until: string | null; sound: boolean; muted_plugins: string[] };
  shortcut: { global: string; global_opens: "oldest" | "inbox" };
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
  /** the welcome screen was closed; until then the app opens on it */
  welcome: { seen: boolean };
  /** launch at login; null when the app cannot say (a browser) */
  autostart: boolean | null;
};

type Patch = {
  appearance?: Partial<Settings["appearance"]>;
  sidebar?: Partial<Settings["sidebar"]>;
  close_window?: Settings["close_window"];
  menu_bar_icon?: boolean;
  notifications?: Partial<Settings["notifications"]>;
  shortcut?: Partial<Settings["shortcut"]>;
  /** a change to one or more plugins' settings, merged key by key */
  plugins?: Record<string, Record<string, unknown>>;
  /** a plugin's link permission replaced, or null to forget it */
  links?: Record<string, LinkPermission | null>;
  port?: number;
  history?: Partial<Settings["history"]>;
  updates?: Partial<Settings["updates"]>;
  welcome?: Partial<Settings["welcome"]>;
  autostart?: boolean;
};

type Store = { settings: Settings; update: (patch: Patch) => Promise<void>; native: boolean; loaded: boolean };

const ZOOM: Record<TextSize, string> = { small: "0.92", default: "1", large: "1.08" };

const SettingsContext = createContext<Store | null>(null);

export function applyTextSize(size: TextSize) {
  const root = document.getElementById("root");
  if (root) (root.style as CSSStyleDeclaration & { zoom: string }).zoom = ZOOM[size];
}

type Served = Pick<Settings, "appearance" | "sidebar" | "close_window" | "menu_bar_icon" | "notifications" | "shortcut" | "plugins" | "links" | "port" | "history" | "updates" | "welcome">;
const fromServer = (s: ServerSettings): Served => ({
  appearance: { theme: s.appearance.theme, text_size: s.appearance.text_size },
  sidebar: { open: s.sidebar.open },
  close_window: s.close_window,
  menu_bar_icon: s.menu_bar_icon,
  notifications: { enabled: s.notifications.enabled, paused_until: s.notifications.paused_until, sound: s.notifications.sound, muted_plugins: s.notifications.muted_plugins },
  shortcut: { global: s.shortcut.global, global_opens: s.shortcut.global_opens },
  plugins: s.plugins ?? {},
  links: s.links ?? {},
  port: s.port,
  history: { keep_days: s.history?.keep_days ?? null },
  updates: { check: s.updates?.check ?? true },
  welcome: { seen: s.welcome?.seen ?? false },
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

export function SettingsProvider({ children }: { children: ReactNode }) {
  const native = inTauri();
  const live = useLive();
  const [settings, setSettings] = useState<Settings>(() => ({
    appearance: { theme: "system", text_size: "default" },
    sidebar: { open: true },
    close_window: "hide",
    menu_bar_icon: true,
    notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
    shortcut: { global: DEFAULT_GLOBAL_SHORTCUT, global_opens: "oldest" },
    plugins: {},
    links: {},
    port: 4747,
    history: { keep_days: null },
    updates: { check: true },
    welcome: { seen: false },
    autostart: null,
  }));
  const [loaded, setLoaded] = useState(false);
  // the settings as last applied, ahead of the next render: a change made
  // before React renders the previous one builds on it, not on a stale copy
  const current = useRef(settings);
  current.current = settings;
  // the latest change sent: an older answer that lands after it is dropped
  const sent = useRef(0);
  const applied = useRef<Pick<Settings, "appearance"> | null>(null);

  const apply = useCallback((s: Pick<Settings, "appearance">) => {
    if (applied.current?.appearance.theme !== s.appearance.theme) setThemePreference(s.appearance.theme);
    if (applied.current?.appearance.text_size !== s.appearance.text_size) applyTextSize(s.appearance.text_size);
    applied.current = s;
    cacheAppearance(s.appearance);
  }, []);

  const load = useCallback(async () => {
    try {
      const s = fromServer(await api.settings());
      apply(s);
      setSettings((prev) => ({ ...prev, ...s }));
      setLoaded(true);
    } catch {
      // the server is away; the cached appearance stands until it is back
    }
  }, [apply]);

  useEffect(() => {
    load();
  }, [load]);

  // a change made elsewhere — the CLI, the file, another window
  useEffect(() => {
    if (live.lastNotice?.kind === "settings_changed") load();
  }, [live.lastNotice, load]);
  useEffect(() => {
    if (live.connected) load();
  }, [live.connected, load]);

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
    async (patch: Patch) => {
      // the core's settings go to the core; applied at once, confirmed by the response
      if (patch.appearance || patch.sidebar || patch.close_window !== undefined || patch.menu_bar_icon !== undefined || patch.notifications || patch.shortcut || patch.plugins || patch.links || patch.port !== undefined || patch.history || patch.updates || patch.welcome) {
        const base = current.current;
        const next: Served = {
          appearance: { ...base.appearance, ...patch.appearance },
          sidebar: { ...base.sidebar, ...patch.sidebar },
          close_window: patch.close_window ?? base.close_window,
          menu_bar_icon: patch.menu_bar_icon ?? base.menu_bar_icon,
          notifications: { ...base.notifications, ...patch.notifications },
          shortcut: { ...base.shortcut, ...patch.shortcut },
          plugins: mergePlugins(base.plugins, patch.plugins),
          links: mergeLinks(base.links, patch.links),
          port: patch.port ?? base.port,
          history: { ...base.history, ...patch.history },
          updates: { ...base.updates, ...patch.updates },
          welcome: { ...base.welcome, ...patch.welcome },
        };
        apply(next);
        current.current = { ...base, ...next };
        setSettings((s) => ({ ...s, ...next }));
        const body: Record<string, unknown> = {};
        if (patch.appearance) body.appearance = patch.appearance;
        if (patch.sidebar) body.sidebar = patch.sidebar;
        if (patch.close_window !== undefined) body.close_window = patch.close_window;
        if (patch.menu_bar_icon !== undefined) body.menu_bar_icon = patch.menu_bar_icon;
        if (patch.notifications) body.notifications = patch.notifications;
        if (patch.shortcut) body.shortcut = patch.shortcut;
        if (patch.plugins) body.plugins = patch.plugins;
        if (patch.links) body.links = patch.links;
        if (patch.port !== undefined) body.port = patch.port;
        if (patch.history) body.history = patch.history;
        if (patch.updates) body.updates = patch.updates;
        if (patch.welcome) body.welcome = patch.welcome;
        const mine = ++sent.current;
        try {
          const s = fromServer(await api.patchSettings(body));
          if (mine !== sent.current) return;
          apply(s);
          current.current = { ...current.current, ...s };
          setSettings((prev) => ({ ...prev, ...s }));
        } catch {
          // refused or the server is away: what was applied stays for this session
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
    [native, apply],
  );

  const value = useMemo(() => ({ settings, update, native, loaded }), [settings, update, native, loaded]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Store {
  const store = useContext(SettingsContext);
  if (!store) throw new Error("useSettings outside SettingsProvider");
  return store;
}

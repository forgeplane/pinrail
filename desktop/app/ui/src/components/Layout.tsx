import { ArrowLeft, ArrowRight, Blocks, FolderGit2, History, Inbox, Keyboard, Moon, PanelLeft, RefreshCw, Search, Settings, Sun, SunMoon, X } from "lucide-react";
import { Fragment, useEffect, useState, type ReactNode } from "react";
import { NavLink, useLocation, useNavigate } from "react-router";
import { api, inTauri } from "../api/client";
import { overlayTitleBar } from "../lib/native";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import { SettingsDialog, type SettingsSection } from "./settings/SettingsDialog";
import { SHORTCUTS } from "../lib/shortcuts";
import { MOD, hasMod } from "../lib/keys";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useTopBarContent } from "../state/topbar";
import { toggleTheme, useTheme } from "../lib/theme";
import { Tooltip } from "./Tooltip";

// On macOS the window has no title bar of its own: the traffic lights sit
// over the sidebar's first row and the bars are the drag handles.

// Cmd+I for the inbox; History takes the shift, as Cmd+H hides the app.
// Plugins live in the settings: Cmd+Shift+P opens that section.
const NAV = [
  { key: "inbox", label: "Inbox", to: "/", Icon: Inbox, letter: "i", shift: false, keys: [MOD, "I"] },
  { key: "history", label: "History", to: "/history", Icon: History, letter: "h", shift: true, keys: [MOD, "⇧", "H"] },
];
const PLUGINS_KEYS = [MOD, "⇧", "P"];

const pageTitle = (path: string) => (path === "/" ? "Inbox" : path.startsWith("/history") ? "History" : "Review");

function isTyping(target: EventTarget | null) {
  const el = target as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
}

export function Layout({ children }: { children: ReactNode }) {
  const live = useLive();
  const location = useLocation();
  const navigate = useNavigate();
  const theme = useTheme();
  const topbar = useTopBarContent();
  const [help, setHelp] = useState(false);
  const [palette, setPalette] = useState(false);
  const [settings, setSettings] = useState<SettingsSection | null>(null);
  const [settingsPlugin, setSettingsPlugin] = useState<string | null>(null);
  const { settings: prefs, update } = useSettings();
  const sidebar = prefs.sidebar.open;
  const toggleSidebar = () => update({ sidebar: { open: !sidebar } });
  const sidebarButton = (
    <Tooltip label={sidebar ? "Hide sidebar" : "Show sidebar"} keys={[MOD, "B"]}>
      <button type="button" className="bar-button" onClick={toggleSidebar} aria-label={sidebar ? "Hide sidebar" : "Show sidebar"}>
        <PanelLeft size={16} />
      </button>
    </Tooltip>
  );

  const actions: PaletteAction[] = [
    { id: "inbox", label: "Go to inbox", keys: NAV[0].keys, icon: Inbox, run: () => navigate("/") },
    { id: "oldest", label: "Open the oldest pending review", icon: Inbox, run: () => {
      const oldest = live.pending[live.pending.length - 1];
      navigate(oldest ? `/reviews/${oldest.id}` : "/");
    } },
    { id: "history", label: "Go to history", keys: NAV[1].keys, icon: History, run: () => navigate("/history") },
    { id: "plugins", label: "Plugins", keys: PLUGINS_KEYS, icon: Blocks, run: () => setSettings("plugins") },
    { id: "reload-plugins", label: "Reload plugins", icon: RefreshCw, run: () => void api.reloadPlugins().catch(() => {}) },
    { id: "theme", label: theme === "dark" ? "Switch to the light theme" : "Switch to the dark theme", keys: ["T"], icon: SunMoon, run: toggleTheme },
    { id: "sidebar", label: sidebar ? "Hide the sidebar" : "Show the sidebar", keys: [MOD, "B"], icon: PanelLeft, run: toggleSidebar },
    { id: "shortcuts", label: "Keyboard shortcuts", keys: ["?"], icon: Keyboard, run: () => setHelp(true) },
    { id: "settings", label: "Open settings", keys: [MOD, ","], icon: Settings, run: () => setSettings("general") },
  ];

  // a link to /plugins lands in the settings section
  useEffect(() => {
    const state = location.state as { settings?: SettingsSection; plugin?: string } | null;
    if (state?.settings) {
      setSettings(state.settings);
      setSettingsPlugin(state.plugin ?? null);
      navigate(location.pathname, { replace: true, state: null });
    }
  }, [location.state, location.pathname, navigate]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (hasMod(event) && !event.altKey) {
        const page = NAV.find((n) => n.letter === event.key.toLowerCase() && n.shift === event.shiftKey);
        if (page) {
          event.preventDefault();
          navigate(page.to);
          return;
        }
        if (event.shiftKey && event.key.toLowerCase() === "p") {
          event.preventDefault();
          setSettings("plugins");
          return;
        }
      }
      if (hasMod(event) && !event.altKey && !event.shiftKey) {
        if (event.key === "k" || event.key === "K") {
          event.preventDefault();
          setPalette((p) => !p);
        } else if (event.key === ",") {
          event.preventDefault();
          setSettings((s) => (s ? null : "general"));
        } else if (event.key === "b" || event.key === "B") {
          event.preventDefault();
          toggleSidebar();
        } else if (event.key === "[") {
          event.preventDefault();
          navigate(-1);
        } else if (event.key === "]") {
          event.preventDefault();
          navigate(1);
        }
        return;
      }
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "Escape") {
        setHelp(false);
        return;
      }
      if (isTyping(event.target)) return;
      if (event.key === "?") setHelp((h) => !h);
      if (event.key === "t" || event.key === "T") toggleTheme();
    };
    const onCommand = (event: Event) => {
      switch ((event as CustomEvent<string>).detail) {
        case "search":
          setPalette((p) => !p);
          break;
        case "settings":
          setSettings((s) => (s ? null : "general"));
          break;
        case "go-inbox":
          navigate("/");
          break;
        case "go-history":
          navigate("/history");
          break;
        case "go-plugins":
          setSettings("plugins");
          break;
        case "toggle-sidebar":
          toggleSidebar();
          break;
        case "back":
          navigate(-1);
          break;
        case "forward":
          navigate(1);
          break;
      }
    };
    const onThemeToggle = (event: Event) => update({ appearance: { theme: (event as CustomEvent<"dark" | "light">).detail } });
    window.addEventListener("keydown", onKey);
    window.addEventListener("wicket:command", onCommand);
    window.addEventListener("wicket:theme-toggle", onThemeToggle);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wicket:command", onCommand);
      window.removeEventListener("wicket:theme-toggle", onThemeToggle);
    };
  }, [navigate, update]);

  return (
    <div className={`app-frame ${overlayTitleBar ? "has-overlay-bar" : ""} ${sidebar ? "" : "sidebar-closed"}`}>
      {sidebar ? (
        <aside className="app-sidebar" aria-label="Workspace">
          <div className="sidebar-bar" data-tauri-drag-region>
            {sidebarButton}
          </div>
          {inTauri() ? null : (
            <NavLink to="/" className="app-brand" aria-label="Wicket home">
              <span className="wicket-mark" aria-hidden="true" />
              <span>Wicket</span>
            </NavLink>
          )}
          <nav className="app-nav" aria-label="Main">
            {NAV.map(({ key, label, to, Icon, keys }) => (
              <Fragment key={key}>
                <Tooltip label={label} keys={keys} side="bottom">
                  <NavLink to={to} end={to === "/"} className={({ isActive }) => (isActive ? "is-active" : "")}>
                    <span className="nav-label">
                      <Icon size={15} strokeWidth={1.75} />
                      {label}
                    </span>
                    {key === "inbox" ? <span className="nav-count">{live.pendingCount}</span> : null}
                  </NavLink>
                </Tooltip>
                {key === "history" ? (
                  <button type="button" className="nav-search" onClick={() => setPalette(true)}>
                    <span className="nav-label">
                      <Search size={15} strokeWidth={1.75} />
                      Search
                    </span>
                    <span className="nav-keys">
                      <kbd>{MOD}</kbd>
                      <kbd>K</kbd>
                    </span>
                  </button>
                ) : null}
              </Fragment>
            ))}
          </nav>
          {live.repositories.length > 0 ? (
            <section className="sidebar-repositories" aria-label="Repositories">
              <h2>Repositories</h2>
              {live.repositories.map((repo) => (
                <NavLink key={repo} to={`/?repo=${encodeURIComponent(repo)}`} className="sidebar-repo">
                  <FolderGit2 size={14} strokeWidth={1.75} />
                  {repo}
                </NavLink>
              ))}
            </section>
          ) : null}
          <div className="sidebar-bottom">
            <span className={`connection-dot ${live.connected ? "is-on" : ""}`} />
            <span>{live.connected ? "Connected" : "Reconnecting…"}</span>
            <Tooltip label="Settings" keys={[MOD, ","]} side="top">
              <button type="button" className="bar-button sidebar-gear" onClick={() => setSettings("general")} aria-label="Settings">
                <Settings size={15} />
              </button>
            </Tooltip>
          </div>
        </aside>
      ) : null}
      <div className="app-shell">
        <header className="app-topbar" data-tauri-drag-region>
          {sidebar ? null : sidebarButton}
          <span className="topbar-history">
            <Tooltip label="Back" keys={[MOD, "["]}>
              <button type="button" className="bar-button" onClick={() => navigate(-1)} aria-label="Back">
                <ArrowLeft size={16} />
              </button>
            </Tooltip>
            <Tooltip label="Forward" keys={[MOD, "]"]}>
              <button type="button" className="bar-button" onClick={() => navigate(1)} aria-label="Forward">
                <ArrowRight size={16} />
              </button>
            </Tooltip>
          </span>
          {topbar?.crumb ?? <span className="topbar-title">{pageTitle(location.pathname)}</span>}
          {topbar?.actions}
          <Tooltip label="Keyboard shortcuts" keys={["?"]}>
            <button type="button" className="bar-button" onClick={() => setHelp(true)} aria-label="Keyboard shortcuts">
              <Keyboard size={16} />
            </button>
          </Tooltip>
          <Tooltip label={theme === "dark" ? "Light theme" : "Dark theme"} keys={["T"]}>
            <button type="button" className="bar-button" onClick={toggleTheme} aria-label="Toggle light and dark theme">
              {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
            </button>
          </Tooltip>
        </header>
        <main className="app-main" tabIndex={-1}>
          {children}
        </main>
      </div>
      <CommandPalette open={palette} onClose={() => setPalette(false)} actions={actions} />
      <SettingsDialog
        open={settings !== null}
        section={settings ?? "general"}
        plugin={settingsPlugin}
        onSection={(s) => {
          setSettings(s);
          setSettingsPlugin(null);
        }}
        onClose={() => {
          setSettings(null);
          setSettingsPlugin(null);
        }}
      />
      {help ? (
        <div className="app-dialog-backdrop" onClick={() => setHelp(false)}>
          <div className="app-dialog" role="dialog" aria-labelledby="keyboard-title" onClick={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              <h2 id="keyboard-title">Keyboard shortcuts</h2>
              <Tooltip label="Close" keys={["Esc"]}>
                <button type="button" className="bar-button" onClick={() => setHelp(false)} aria-label="Close">
                  <X size={16} />
                </button>
              </Tooltip>
            </div>
            <dl className="shortcut-list">
              {SHORTCUTS.map((s) => (
                <Fragment key={s.what}>
                  <dt>{s.what}</dt>
                  <dd>
                    {s.keys.map((combo, i) => (
                      <span key={i} className="combo">
                        {combo.map((k, j) => (
                          <kbd key={j}>{k}</kbd>
                        ))}
                      </span>
                    ))}
                  </dd>
                </Fragment>
              ))}
            </dl>
          </div>
        </div>
      ) : null}
    </div>
  );
}

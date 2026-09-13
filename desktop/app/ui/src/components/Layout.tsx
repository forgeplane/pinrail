import { ArrowLeft, ArrowRight, Blocks, FolderGit2, History, Inbox, Keyboard, Moon, PanelLeft, RefreshCw, Search, Sun, SunMoon, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { NavLink, useLocation, useNavigate } from "react-router";
import { api, inTauri } from "../api/client";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import { MOD, hasMod } from "../lib/keys";
import { useLive } from "../state/live";
import { toggleTheme, useTheme } from "../lib/theme";
import { Tooltip } from "./Tooltip";

// On macOS the window has no title bar of its own: the traffic lights sit
// over the sidebar's first row and the bars are the drag handles.
const overlayTitleBar = inTauri() && /Mac/i.test(navigator.platform);
const SIDEBAR_KEY = "wicket:sidebar";

const NAV = [
  { key: "inbox", label: "Inbox", to: "/", Icon: Inbox },
  { key: "history", label: "History", to: "/history", Icon: History },
  { key: "plugins", label: "Plugins", to: "/plugins", Icon: Blocks },
];

const pageTitle = (path: string) =>
  path === "/" ? "Inbox" : path.startsWith("/history") ? "History" : path.startsWith("/plugins") ? "Plugins" : "Review";

function isTyping(target: EventTarget | null) {
  const el = target as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
}

export function Layout({ children }: { children: ReactNode }) {
  const live = useLive();
  const location = useLocation();
  const navigate = useNavigate();
  const theme = useTheme();
  const [help, setHelp] = useState(false);
  const [palette, setPalette] = useState(false);
  const [sidebar, setSidebar] = useState(() => {
    try {
      return localStorage.getItem(SIDEBAR_KEY) !== "closed";
    } catch {
      return true;
    }
  });
  const toggleSidebar = () => {
    setSidebar((open) => {
      try {
        localStorage.setItem(SIDEBAR_KEY, open ? "closed" : "open");
      } catch {
        // a preference that cannot be saved still applies for this session
      }
      return !open;
    });
  };
  const sidebarButton = (
    <Tooltip label={sidebar ? "Hide sidebar" : "Show sidebar"} keys={[MOD, "B"]}>
      <button type="button" className="bar-button" onClick={toggleSidebar} aria-label={sidebar ? "Hide sidebar" : "Show sidebar"}>
        <PanelLeft size={16} />
      </button>
    </Tooltip>
  );

  const actions: PaletteAction[] = [
    { id: "inbox", label: "Go to inbox", icon: Inbox, run: () => navigate("/") },
    { id: "oldest", label: "Open the oldest pending review", icon: Inbox, run: () => {
      const oldest = live.pending[live.pending.length - 1];
      navigate(oldest ? `/reviews/${oldest.id}` : "/");
    } },
    { id: "history", label: "Go to history", icon: History, run: () => navigate("/history") },
    { id: "plugins", label: "Go to plugins", icon: Blocks, run: () => navigate("/plugins") },
    { id: "reload-plugins", label: "Reload plugins", icon: RefreshCw, run: () => void api.reloadPlugins().catch(() => {}) },
    { id: "theme", label: theme === "dark" ? "Switch to the light theme" : "Switch to the dark theme", keys: ["T"], icon: SunMoon, run: toggleTheme },
    { id: "sidebar", label: sidebar ? "Hide the sidebar" : "Show the sidebar", keys: [MOD, "B"], icon: PanelLeft, run: toggleSidebar },
    { id: "shortcuts", label: "Keyboard shortcuts", keys: ["?"], icon: Keyboard, run: () => setHelp(true) },
  ];

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (hasMod(event) && !event.altKey && !event.shiftKey) {
        if (event.key === "k" || event.key === "K") {
          event.preventDefault();
          setPalette((p) => !p);
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
    window.addEventListener("keydown", onKey);
    window.addEventListener("wicket:command", onCommand);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wicket:command", onCommand);
    };
  }, [navigate]);

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
            {NAV.map(({ key, label, to, Icon }) => (
              <NavLink key={key} to={to} end={to === "/"} className={({ isActive }) => (isActive ? "is-active" : "")}>
                <span className="nav-label">
                  <Icon size={15} strokeWidth={1.75} />
                  {label}
                </span>
                {key === "inbox" ? <span className="nav-count">{live.pendingCount}</span> : null}
              </NavLink>
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
          <span className="topbar-title">{pageTitle(location.pathname)}</span>
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
              <dt>Next / previous review</dt>
              <dd>
                <kbd>J</kbd> <kbd>K</kbd>
              </dd>
              <dt>Open the focused review</dt>
              <dd>
                <kbd>Enter</kbd>
              </dd>
              <dt>Search the inbox</dt>
              <dd>
                <kbd>/</kbd>
              </dd>
              <dt>Search everything</dt>
              <dd>
                <kbd>{MOD}</kbd> <kbd>K</kbd>
              </dd>
              <dt>Switch theme</dt>
              <dd>
                <kbd>T</kbd>
              </dd>
              <dt>Show or hide the sidebar</dt>
              <dd>
                <kbd>{MOD}</kbd> <kbd>B</kbd>
              </dd>
              <dt>Back / forward</dt>
              <dd>
                <kbd>{MOD}</kbd> <kbd>[</kbd> <kbd>]</kbd>
              </dd>
              <dt>Hand over to the agent</dt>
              <dd>
                <kbd>{MOD}</kbd> <kbd>Enter</kbd>
              </dd>
              <dt>Previous / next round</dt>
              <dd>
                <kbd>[</kbd> <kbd>]</kbd>
              </dd>
              <dt>Close this</dt>
              <dd>
                <kbd>Esc</kbd>
              </dd>
            </dl>
          </div>
        </div>
      ) : null}
    </div>
  );
}

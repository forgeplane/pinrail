import { useEffect, useState, type ReactNode } from "react";
import { NavLink, useLocation } from "react-router";
import { useLive } from "../state/live";
import { toggleTheme } from "../lib/theme";

const NAV = [
  { key: "inbox", label: "Inbox", to: "/" },
  { key: "history", label: "History", to: "/history" },
  { key: "plugins", label: "Plugins", to: "/plugins" },
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
  const [help, setHelp] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "Escape") {
        setHelp(false);
        return;
      }
      if (isTyping(event.target)) return;
      if (event.key === "?") setHelp((h) => !h);
      if (event.key === "t" || event.key === "T") toggleTheme();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <aside className="app-sidebar" aria-label="Workspace">
        <NavLink to="/" className="app-brand" aria-label="wicket home">
          <span className="wicket-mark" aria-hidden="true" />
          <span>wicket</span>
        </NavLink>
        <nav className="app-nav" aria-label="Main">
          {NAV.map((item) => (
            <NavLink key={item.key} to={item.to} end={item.to === "/"} className={({ isActive }) => (isActive ? "is-active" : "")}>
              <span>{item.label}</span>
              {item.key === "inbox" ? <span className="nav-count">{live.pendingCount}</span> : null}
            </NavLink>
          ))}
        </nav>
        {live.repositories.length > 0 ? (
          <section className="sidebar-repositories" aria-label="Repositories">
            <h2>Repositories</h2>
            {live.repositories.map((repo) => (
              <NavLink key={repo} to={`/?repo=${encodeURIComponent(repo)}`} className="sidebar-repo">
                <span className="repo-square" />
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
      <div className="app-shell">
        <header className="app-topbar">
          <span className="topbar-title">{pageTitle(location.pathname)}</span>
          <button type="button" className="chrome-button" onClick={() => setHelp(true)}>
            Keyboard shortcuts <kbd>?</kbd>
          </button>
          <button type="button" className="chrome-button" onClick={toggleTheme} title="Switch theme (T)" aria-label="Toggle light and dark theme">
            Theme
          </button>
        </header>
        <main className="app-main" tabIndex={-1}>
          {children}
        </main>
      </div>
      {help ? (
        <div className="app-dialog-backdrop" onClick={() => setHelp(false)}>
          <div className="app-dialog" role="dialog" aria-labelledby="keyboard-title" onClick={(e) => e.stopPropagation()}>
            <h2 id="keyboard-title">Keyboard shortcuts</h2>
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
              <dt>Switch theme</dt>
              <dd>
                <kbd>T</kbd>
              </dd>
              <dt>Hand over to the agent</dt>
              <dd>
                <kbd>⌘/Ctrl</kbd> <kbd>Enter</kbd>
              </dd>
              <dt>Previous / next round</dt>
              <dd>
                <kbd>[</kbd> <kbd>]</kbd>
              </dd>
            </dl>
            <button type="button" className="chrome-button" onClick={() => setHelp(false)}>
              Close <kbd>Esc</kbd>
            </button>
          </div>
        </div>
      ) : null}
    </>
  );
}

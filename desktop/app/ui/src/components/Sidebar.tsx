// The app's sidebar: its pages, the search, the sessions or projects, the
// update notice, feedback and the connection, with an edge that resizes it
// by dragging or with the arrow keys.

import { History, Inbox, LayoutGrid, ListChecks, MessageSquare, MessagesSquare, Search, Settings } from "lucide-react";
import { Fragment, useState, type ReactNode } from "react";
import { NavLink } from "react-router";
import { command } from "../lib/commands";
import { MOD } from "../lib/keys";
import { useLive } from "../state/live";
import type { SettingsSection } from "./settings/SettingsDialog";
import { SidebarProjects } from "./SidebarProjects";
import { SidebarSessions } from "./SidebarSessions";
import { Tooltip } from "./Tooltip";
import { UpdateNotice } from "./UpdateNotice";

// The sidebar's pages, with the History tab, which has a shortcut and no
// entry of its own; their keys come from the commands table.
const NAV = [
  { key: "inbox", label: "Inbox", to: "/", Icon: Inbox, keys: command("go-inbox").keys![0] },
  { key: "sessions", label: "Sessions", to: "/sessions", Icon: MessagesSquare, keys: command("go-sessions").keys![0] },
  { key: "history", label: "History", to: "/?tab=history", Icon: History, keys: command("go-history").keys![0] },
  { key: "items", label: "Work items", to: "/items", Icon: ListChecks, keys: command("go-items").keys![0] },
  { key: "projects", label: "Projects", to: "/projects", Icon: LayoutGrid, keys: command("go-projects").keys![0] },
];
const SIDEBAR = NAV.filter((n) => n.key !== "history");

/** Where the History tab is, which the commands go to too. */
export const HISTORY_PATH = NAV[2].to;

/** The sidebar's width, remembered on this machine, and its bounds. */
const SIDEBAR_WIDTH = "pinrail.sidebar.width";
const SIDEBAR_MIN = 200;
const SIDEBAR_MAX = 420;
const SIDEBAR_DEFAULT = 220;
const clampSidebar = (w: number) => Math.round(Math.max(SIDEBAR_MIN, Math.min(w, SIDEBAR_MAX)));
function savedSidebarWidth(): number {
  try {
    const n = Number(localStorage.getItem(SIDEBAR_WIDTH));
    return n > 0 ? clampSidebar(n) : SIDEBAR_DEFAULT;
  } catch {
    return SIDEBAR_DEFAULT;
  }
}

/** The sidebar's width, dragged by its edge or changed with the arrow keys
 *  on it, and remembered on this machine. */
export function useSidebarWidth() {
  const [sidebarWidth, setSidebarWidth] = useState(savedSidebarWidth);
  const keepSidebarWidth = (w: number) => {
    setSidebarWidth(w);
    try {
      localStorage.setItem(SIDEBAR_WIDTH, String(w));
    } catch {
      // private mode or no storage: the width lasts until the app restarts
    }
  };
  const resizeSidebar = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const left = event.currentTarget.parentElement?.getBoundingClientRect().left ?? 0;
    document.body.classList.add("is-resizing");
    const onMove = (e: PointerEvent) => setSidebarWidth(clampSidebar(e.clientX - left));
    const onUp = (e: PointerEvent) => {
      document.body.classList.remove("is-resizing");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      keepSidebarWidth(clampSidebar(e.clientX - left));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const resizeSidebarBy = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 80 : 20;
    if (event.key === "ArrowRight") keepSidebarWidth(clampSidebar(sidebarWidth + step));
    else if (event.key === "ArrowLeft") keepSidebarWidth(clampSidebar(sidebarWidth - step));
    else return;
    event.preventDefault();
  };
  const reset = () => keepSidebarWidth(SIDEBAR_DEFAULT);
  return { width: sidebarWidth, resize: resizeSidebar, resizeBy: resizeSidebarBy, reset };
}

export function Sidebar({
  width,
  toggle,
  sessionsOn,
  shown,
  openIn,
  openItems,
  onSearch,
  onSettings,
  onFeedback,
}: {
  width: ReturnType<typeof useSidebarWidth>;
  /** the button that hides the sidebar */
  toggle: ReactNode;
  /** whether Sessions is on in Settings › Labs */
  sessionsOn: boolean;
  /** the review shown, which the sidebar marks */
  shown: string | null;
  openIn: "panel" | "page";
  /** how many work items are open or in progress */
  openItems: number;
  onSearch: () => void;
  onSettings: (section: SettingsSection) => void;
  onFeedback: () => void;
}) {
  const live = useLive();
  return (
    <aside className="app-sidebar" aria-label="Workspace">
      <div
        className="panel-resizer sidebar-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize the sidebar"
        aria-valuenow={width.width}
        aria-valuemin={SIDEBAR_MIN}
        aria-valuemax={SIDEBAR_MAX}
        tabIndex={0}
        onPointerDown={width.resize}
        onKeyDown={width.resizeBy}
        onDoubleClick={width.reset}
      />
      <div className="sidebar-bar" data-tauri-drag-region>
        {toggle}
      </div>
      <div className="sidebar-scroll" data-testid="sidebar-scroll">
        <nav className="app-nav" aria-label="Main">
          {SIDEBAR.filter((n) => sessionsOn || n.key !== "sessions").map(({ key, label, to, Icon, keys }) => (
            <Fragment key={key}>
              <Tooltip label={label} keys={keys.length ? keys : undefined} side="bottom">
                <NavLink to={to} end={to === "/"} className={({ isActive }) => (isActive ? "is-active" : "")}>
                  <span className="nav-label">
                    <Icon size={15} strokeWidth={1.75} />
                    {label}
                  </span>
                  {key === "inbox" ? (
                    <span className="nav-count" data-testid="inbox-count">
                      {live.pendingCount}
                    </span>
                  ) : null}
                  {key === "items" && openItems ? (
                    <Tooltip label={`${openItems} open or in progress`} side="bottom">
                      <span className="nav-count is-quiet" data-testid="items-count">
                        {openItems}
                      </span>
                    </Tooltip>
                  ) : null}
                </NavLink>
              </Tooltip>
            </Fragment>
          ))}
          <button type="button" className="nav-search" onClick={onSearch}>
            <span className="nav-label">
              <Search size={15} strokeWidth={1.75} />
              Search
            </span>
            <span className="nav-keys">
              <kbd>{MOD}</kbd>
              <kbd>K</kbd>
            </span>
          </button>
        </nav>
        {sessionsOn ? <SidebarSessions shown={shown} /> : <SidebarProjects shown={shown} openIn={openIn} />}
        <UpdateNotice onDetails={() => onSettings("about")} />
        <button type="button" className="sidebar-feedback" onClick={onFeedback}>
          <MessageSquare size={15} strokeWidth={1.75} />
          Send feedback
        </button>
        <div className="sidebar-bottom">
          <span className={`connection-dot ${live.connected ? "is-on" : ""}`} />
          <span>{live.connected ? "Connected" : "Reconnecting…"}</span>
          <Tooltip label="Settings" keys={[MOD, ","]} side="top">
            <button
              type="button"
              className="bar-button sidebar-gear"
              onClick={() => onSettings("general")}
              aria-label="Settings"
            >
              <Settings size={15} />
            </button>
          </Tooltip>
        </div>
      </div>
    </aside>
  );
}

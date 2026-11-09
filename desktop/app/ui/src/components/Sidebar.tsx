// The app's sidebar: its pages, the search, the sessions or projects, the
// update notice, feedback and the connection, with an edge that resizes it
// by dragging or with the arrow keys.

import { History, Inbox, LayoutGrid, ListChecks, MessageSquare, MessagesSquare, Search, Settings } from "lucide-react";
import { Fragment, type ReactNode } from "react";
import { NavLink } from "react-router";
import { command } from "../lib/commands";
import { MOD } from "../lib/keys";
import { useLive } from "../state/live";
import type { SettingsSection } from "./settings/SettingsDialog";
import { SidebarProjects } from "./SidebarProjects";
import { SidebarSessions } from "./SidebarSessions";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { Tooltip } from "./Tooltip";
import { UpdateNotice } from "./UpdateNotice";
import { useResizableWidth } from "../lib/useResizableWidth";

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
/** The sidebar's width, dragged by its edge or changed with the arrow keys
 *  on it, and remembered on this machine. */
export function useSidebarWidth() {
  const { width, keep, onKeyDown, edgeProps } = useResizableWidth({
    key: SIDEBAR_WIDTH,
    min: SIDEBAR_MIN,
    max: SIDEBAR_MAX,
    fallback: SIDEBAR_DEFAULT,
    grows: "right",
    anchor: (edge) => edge.parentElement?.getBoundingClientRect().left ?? 0,
  });
  return { width: width ?? SIDEBAR_DEFAULT, edgeProps, onKeyDown, reset: () => keep(SIDEBAR_DEFAULT) };
}

export function Sidebar({
  width,
  toggle,
  sessionsOn,
  shown,
  openIn,
  openItems,
  itemsInProgress,
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
  itemsInProgress: number;
  onSearch: () => void;
  onSettings: (section: SettingsSection) => void;
  onFeedback: () => void;
}) {
  const live = useLive();
  return (
    <aside className="app-sidebar" aria-label="Workspace">
      <div
        className="panel-resizer sidebar-resizer"
        {...width.edgeProps}
        aria-label="Resize the sidebar"
        onKeyDown={(event) => width.onKeyDown(event)}
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
                    <Tooltip
                      label={`${openItems} open or in progress${itemsInProgress ? `, ${itemsInProgress} of them in progress` : ""}`}
                      side="bottom"
                    >
                      <span className="nav-counts">
                        {itemsInProgress ? (
                          <span className="nav-count is-working" data-testid="items-in-progress">
                            <ItemStatusIcon status="in_progress" size={10} decorative />
                            {itemsInProgress}
                          </span>
                        ) : null}
                        <span className="nav-count is-quiet" data-testid="items-count">
                          {openItems}
                        </span>
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

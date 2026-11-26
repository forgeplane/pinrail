// The app's sidebar: its pages, the search, the sessions or projects, the
// update notice, feedback and the connection, with an edge that resizes it
// by dragging or with the arrow keys.

import { pageLink } from "../lib/pageMemory";
import { CountBadge } from "./CountBadge";
import {
  History,
  Inbox,
  LayoutGrid,
  ListChecks,
  MessageSquare,
  MessagesSquare,
  Search,
  Settings,
  SquarePen,
} from "lucide-react";
import { Fragment, type ReactNode } from "react";
import { NavLink, useLocation } from "react-router";
import { command } from "../lib/commands";
import { MOD } from "../lib/keys";
import { useLive } from "../state/live";
import { NEW_SESSION_PATH } from "./NewSession";
import type { SettingsSection } from "./settings/SettingsDialog";
import { SidebarSessions } from "./SidebarSessions";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { Tooltip } from "./Tooltip";
import { UpdateNotice } from "./UpdateNotice";
import { useResizableWidth } from "../lib/useResizableWidth";

// The sidebar's pages, with the History tab, which has a shortcut and no
// entry of its own; their keys come from the commands table.
const NAV = [
  { key: "inbox", label: "Inbox", to: "/", Icon: Inbox, keys: command("go-inbox").keys![0] },
  { key: "history", label: "History", to: "/?tab=history", Icon: History, keys: command("go-history").keys![0] },
  { key: "items", label: "Work items", to: "/items", Icon: ListChecks, keys: command("go-items").keys![0] },
  { key: "sessions", label: "Sessions", to: "/sessions", Icon: MessagesSquare, keys: command("go-sessions").keys![0] },
  { key: "projects", label: "Projects", to: "/projects", Icon: LayoutGrid, keys: command("go-projects").keys![0] },
];
const SIDEBAR = NAV.filter((n) => n.key !== "history");

/** Where the History tab is, which the commands go to too. */
export const HISTORY_PATH = NAV[1].to;

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

/** The Inbox entry's counts: what waits on the person, on the accent
 *  colour, and the unread updates after it in the quiet tone. With no
 *  update unread, the waiting count alone, as before. */
function InboxCounts({ waiting, unread }: { waiting: number; unread: number }) {
  if (!unread) return <CountBadge count={waiting} data-testid="inbox-count" />;
  return (
    <Tooltip
      label={`${waiting ? `${waiting} waiting on you, ` : ""}${unread} ${unread === 1 ? "update" : "updates"} unread`}
      side="bottom"
    >
      <span className="nav-counts">
        {waiting ? <CountBadge count={waiting} data-testid="inbox-count" /> : null}
        <CountBadge count={unread} tone="quiet" data-testid="inbox-unread" />
      </span>
    </Tooltip>
  );
}

export function Sidebar({
  width,
  toggle,
  shown,
  openItems,
  itemsInProgress,
  onSearch,
  onSettings,
  onFeedback,
}: {
  width: ReturnType<typeof useSidebarWidth>;
  /** the button that hides the sidebar */
  toggle: ReactNode;
  /** the review shown, which the sidebar marks */
  shown: string | null;
  /** how many work items are open or in progress */
  openItems: number;
  itemsInProgress: number;
  onSearch: () => void;
  onSettings: (section: SettingsSection) => void;
  onFeedback: () => void;
}) {
  // drawn again on each change of the address, so that each page's link
  // carries the query that page was last left with
  const { pathname } = useLocation();
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
          {SIDEBAR.map(({ key, label, to, Icon, keys }) => (
            <Fragment key={key}>
              <Tooltip label={label} keys={keys.length ? keys : undefined} side="bottom">
                <NavLink
                  // a list page comes back with the filters it was left with
                  to={pageLink(to)}
                  // a session's own page is marked by its row, not by Sessions
                  end={to === "/" || to === "/sessions"}
                  // the New session page is no session's
                  className={({ isActive }) => (isActive && pathname !== NEW_SESSION_PATH ? "is-active" : "")}
                >
                  <span className="nav-label">
                    <Icon size={15} strokeWidth={1.75} />
                    {label}
                  </span>
                  {key === "inbox" ? <InboxCounts waiting={live.pendingCount} unread={live.unreadUpdates} /> : null}
                  {key === "items" && openItems ? (
                    <Tooltip
                      label={`${openItems} open or in progress${itemsInProgress ? `, ${itemsInProgress} of them in progress` : ""}`}
                      side="bottom"
                    >
                      <span className="nav-counts">
                        {itemsInProgress ? (
                          <CountBadge
                            count={itemsInProgress}
                            tone="plain"
                            icon={<ItemStatusIcon status="in_progress" size={10} decorative />}
                            data-testid="items-in-progress"
                          />
                        ) : null}
                        <CountBadge count={openItems} tone="quiet" data-testid="items-count" />
                      </span>
                    </Tooltip>
                  ) : null}
                </NavLink>
              </Tooltip>
              {/* a new session, under the inbox */}
              {key === "inbox" ? (
                <NavLink to={NEW_SESSION_PATH} className={({ isActive }) => `nav-new ${isActive ? "is-active" : ""}`}>
                  <span className="nav-label">
                    <SquarePen size={15} strokeWidth={1.75} />
                    New session
                  </span>
                  <span className="nav-keys">
                    <kbd>{MOD}</kbd>
                    <kbd>N</kbd>
                  </span>
                </NavLink>
              ) : null}
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
        <SidebarSessions shown={shown} />
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

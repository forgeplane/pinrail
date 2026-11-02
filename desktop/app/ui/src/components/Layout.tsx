import {
  ArrowLeft,
  ArrowRight,
  Ban,
  Blocks,
  Hand,
  History,
  Inbox,
  Keyboard,
  MessageSquare,
  MessagesSquare,
  Moon,
  PanelLeft,
  Search,
  Settings,
  Sparkles,
  Sun,
  SunMoon,
  Palette,
} from "lucide-react";
import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { NavLink, useLocation, useNavigate } from "react-router";
import { api, sendFeedback } from "../api/client";
import { overlayTitleBar } from "../lib/native";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import { SettingsDialog, type SettingsSection } from "./settings/SettingsDialog";
import { Toasts } from "./Toasts";
import { ShortcutsDialog } from "./ShortcutsDialog";
import { FeedbackDialog, rememberFeedbackEmail, type FeedbackDraft } from "./FeedbackDialog";
import { MOD, SHIFT, hasMod } from "../lib/keys";
import { reviewPath, reviewShown } from "../lib/url";
import { useLive } from "../state/live";
import { useToast } from "../state/toasts";
import { useSettings } from "../state/settings";
import { useTopBarContent } from "../state/topbar";
import { toggleTheme, useTheme } from "../lib/theme";
import { chooseTheme, chosenTheme } from "../lib/themes";
import { SidebarSessions } from "./SidebarSessions";
import { Tooltip } from "./Tooltip";
import { UpdateNotice } from "./UpdateNotice";
import { FIRST_REVIEW, WelcomeDialog, type WelcomeAt } from "./welcome/WelcomeDialog";
import { WhatsNewDialog } from "./WhatsNewDialog";
import { RELEASES, compareVersions, latestNotes, releasesToShow, type CardAction, type Release } from "../lib/whatsNew";

// On macOS the window has no title bar of its own: the traffic lights sit
// over the sidebar's first row and the bars are the drag handles.

// Cmd+I for the inbox; History takes the shift, as Cmd+H hides the app.
// Plugins live in the settings: Cmd+Shift+P opens that section.
const NAV = [
  { key: "inbox", label: "Inbox", to: "/", Icon: Inbox, letter: "i", shift: false, keys: [MOD, "I"] },
  // no shortcut of its own, which a plugin's view could no longer have
  { key: "sessions", label: "Sessions", to: "/sessions", Icon: MessagesSquare, letter: "", shift: false, keys: [] },
  // the inbox's History tab, which has a shortcut and no entry of its own
  {
    key: "history",
    label: "History",
    to: "/?tab=history",
    Icon: History,
    letter: "h",
    shift: true,
    keys: [MOD, SHIFT, "H"],
  },
];
const SIDEBAR = NAV.filter((n) => n.key !== "history");
const PLUGINS_KEYS = [MOD, SHIFT, "P"];

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

const pageTitle = (path: string) =>
  path === "/"
    ? "Inbox"
    : path.startsWith("/sessions/")
      ? "Session"
      : path.startsWith("/sessions")
        ? "Sessions"
        : "Review";

function isTyping(target: EventTarget | null) {
  const el = target as HTMLElement | null;
  return (
    !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
  );
}

export function Layout({ children }: { children: ReactNode }) {
  const live = useLive();
  const location = useLocation();
  const navigate = useNavigate();
  const theme = useTheme();
  const topbar = useTopBarContent();
  const [help, setHelp] = useState(false);
  const [feedback, setFeedback] = useState(false);
  // a report that was not sent, to open the dialog again with
  const [unsent, setUnsent] = useState<{ draft: FeedbackDraft; reason: string } | null>(null);
  const feedbackOpen = useRef(false);
  feedbackOpen.current = feedback;
  const toast = useToast();
  const [palette, setPalette] = useState(false);
  const [settings, setSettings] = useState<SettingsSection | null>(null);
  const [settingsPlugin, setSettingsPlugin] = useState<string | null>(null);
  const { settings: prefs, themes, update, loaded } = useSettings();
  const sidebar = prefs.sidebar.open;
  const toggleSidebar = useCallback(() => update({ sidebar: { open: !sidebar } }), [update, sidebar]);
  const sidebarButton = (
    <Tooltip label={sidebar ? "Hide sidebar" : "Show sidebar"} keys={[MOD, "B"]}>
      <button
        type="button"
        className="bar-button"
        onClick={toggleSidebar}
        aria-label={sidebar ? "Hide sidebar" : "Show sidebar"}
      >
        <PanelLeft size={16} />
      </button>
    </Tooltip>
  );

  // Show What's new once for each release that has notes, when the settings
  // first load. A fresh install shows nothing and records the installed
  // release as seen.
  const [whatsNew, setWhatsNew] = useState<Release[] | null>(null);
  const [running, setRunning] = useState<string | null>(null);
  const newsAsked = useRef(false);
  useEffect(() => {
    if (!loaded || newsAsked.current) return;
    newsAsked.current = true;
    const fresh = !prefs.welcome.seen;
    const seen = prefs.whats_new.seen;
    api
      .info()
      .then(({ version }) => {
        setRunning(version);
        const releases = releasesToShow(RELEASES, version, seen, fresh);
        if (releases.length) setWhatsNew(releases);
        // record a release without notes as seen, but never an older one than recorded
        else if (!seen || compareVersions(seen, version) < 0) void update({ whats_new: { seen: version } });
      })
      .catch(() => {});
  }, [loaded, prefs.welcome.seen, prefs.whats_new.seen, update]);
  // opened from the menu, the command palette or Settings › About
  const latest = running ? latestNotes(RELEASES, running, import.meta.env.DEV) : [];
  const openWhatsNew = () => {
    if (latest.length) setWhatsNew(latest);
  };
  const openWhatsNewRef = useRef(openWhatsNew);
  openWhatsNewRef.current = openWhatsNew;
  const closeWhatsNew = () => {
    const seen = prefs.whats_new.seen;
    if (running && (!seen || compareVersions(seen, running) < 0)) void update({ whats_new: { seen: running } });
    setWhatsNew(null);
  };
  const openFromWhatsNew = (action: CardAction) => {
    if (action.settings) setSettings(action.settings as SettingsSection);
    else if (action.to) navigate(action.to);
  };

  const actions: PaletteAction[] = [
    { id: "inbox", label: "Go to inbox", keys: NAV[0].keys, icon: Inbox, run: () => navigate("/") },
    {
      id: "oldest",
      label: "Open the oldest pending review",
      icon: Inbox,
      run: () => {
        const oldest = live.pending[live.pending.length - 1];
        navigate(oldest ? reviewPath(oldest.id, prefs.inbox.open_in) : "/");
      },
    },
    { id: "history", label: "Go to history", keys: NAV[2].keys, icon: History, run: () => navigate(NAV[2].to) },
    { id: "plugins", label: "Plugins", keys: PLUGINS_KEYS, icon: Blocks, run: () => setSettings("plugins") },
    {
      id: "theme",
      label: theme === "dark" ? "Switch to the light theme" : "Switch to the dark theme",
      keys: [MOD, SHIFT, "L"],
      icon: SunMoon,
      run: toggleTheme,
    },
    ...themes.flatMap((entry) => {
      const scheme = entry.scheme;
      if (!scheme || entry.error) return [];
      const chosen = (of: "dark" | "light") =>
        chosenTheme(themes, of === "dark" ? prefs.appearance.dark_theme : prefs.appearance.light_theme, of)?.id ?? of;
      if (entry.id === chosen(theme)) return [];
      // the action above already switches to the other scheme's built-in theme
      if (scheme !== theme && entry.id === scheme && chosen(scheme) === scheme) return [];
      return [
        {
          id: `theme-${entry.id}`,
          label: `Switch to the ${entry.name} theme`,
          icon: Palette,
          run: () => void update(chooseTheme(entry.id, scheme, prefs.appearance.theme)),
        },
      ];
    }),
    {
      id: "sidebar",
      label: sidebar ? "Hide the sidebar" : "Show the sidebar",
      keys: [MOD, "B"],
      icon: PanelLeft,
      run: toggleSidebar,
    },
    { id: "shortcuts", label: "Keyboard shortcuts", keys: ["?"], icon: Keyboard, run: () => setHelp(true) },
    { id: "settings", label: "Open settings", keys: [MOD, ","], icon: Settings, run: () => setSettings("general") },
    { id: "welcome", label: "Set up Pinrail", icon: Hand, run: () => setWelcome({ step: 0 }) },
    ...(latest.length ? [{ id: "whats-new", label: "What's new in Pinrail", icon: Sparkles, run: openWhatsNew }] : []),
    { id: "feedback", label: "Send feedback…", icon: MessageSquare, run: () => setFeedback(true) },
    ...(reviewShown(location.pathname, location.search)
      ? [
          {
            id: "discard",
            label: "Discard this review",
            keys: [MOD, "D"],
            icon: Ban,
            run: () => window.dispatchEvent(new Event("pinrail:discard")),
          },
        ]
      : []),
  ];

  // Pinrail opens the setup until it is finished or skipped; asked once,
  // when the settings first arrive
  const [welcome, setWelcome] = useState<WelcomeAt | null>(null);
  const welcomed = useRef(false);
  useEffect(() => {
    if (!loaded || welcomed.current) return;
    welcomed.current = true;
    if (!prefs.welcome.seen) setWelcome({ step: 0 });
  }, [loaded, prefs.welcome.seen]);
  const closeWelcome = () => {
    setWelcome(null);
    setAway(null);
    if (!prefs.welcome.seen) void update({ welcome: { seen: true } });
  };
  // the setup steps aside while its first review is decided, and comes
  // back on that step once it is
  const [away, setAway] = useState<string | null>(null);
  const openFromWelcome = (id: string) => {
    setAway(id);
    setWelcome(null);
    navigate(reviewPath(id, prefs.inbox.open_in));
  };
  useEffect(() => {
    if (!away || !live.connected || live.pending.some((r) => r.id === away)) return;
    setWelcome({ step: FIRST_REVIEW, sample: away, decided: true });
    setAway(null);
  }, [away, live.connected, live.pending]);

  // a link to /plugins lands in the settings section
  useEffect(() => {
    const state = location.state as { settings?: SettingsSection; plugin?: string } | null;
    if (state?.settings) {
      setSettings(state.settings);
      setSettingsPlugin(state.plugin ?? null);
      navigate(location.pathname, { replace: true, state: null });
    }
  }, [location.state, location.pathname, navigate]);

  // The reviews waiting, oldest first, the order the global shortcut uses;
  // kept in a ref so the key handler below sees the current list.
  // the review shown, on its own page or in the inbox's side panel
  const shown = reviewShown(location.pathname, location.search);
  const waiting = useMemo(
    () => [...live.pending].sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id)),
    [live.pending],
  );
  const waitingRef = useRef(waiting);
  waitingRef.current = waiting;
  const shownRef = useRef<string | null>(null);
  shownRef.current = reviewShown(location.pathname, location.search);
  const openInRef = useRef(prefs.inbox.open_in);
  openInRef.current = prefs.inbox.open_in;

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // ⌥↓ and ⌥↑ walk the waiting reviews from wherever you are
      // the inbox handles them itself while a review is open in its side panel
      if (event.defaultPrevented) return;
      if (event.altKey && !hasMod(event) && !event.shiftKey && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        const list = waitingRef.current;
        if (!list.length) return;
        event.preventDefault();
        const current = shownRef.current;
        const at = list.findIndex((r) => r.id === current);
        const next =
          at < 0
            ? event.key === "ArrowDown"
              ? 0
              : list.length - 1
            : (at + (event.key === "ArrowDown" ? 1 : list.length - 1)) % list.length;
        navigate(reviewPath(list[next].id, openInRef.current));
        return;
      }
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
        if (event.shiftKey && event.key.toLowerCase() === "l") {
          event.preventDefault();
          toggleTheme();
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
      if (event.key === "?") {
        // the key must not land in the dialog's search field
        event.preventDefault();
        setHelp((h) => !h);
      }
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
          navigate(NAV[2].to);
          break;
        case "go-plugins":
          setSettings("plugins");
          break;
        case "toggle-sidebar":
          toggleSidebar();
          break;
        case "toggle-theme":
          toggleTheme();
          break;
        case "back":
          navigate(-1);
          break;
        case "forward":
          navigate(1);
          break;
        case "feedback":
          setFeedback(true);
          break;
        case "whats-new":
          openWhatsNewRef.current();
          break;
      }
    };
    const onThemeToggle = (event: Event) =>
      update({ appearance: { theme: (event as CustomEvent<"dark" | "light">).detail } });
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:command", onCommand);
    window.addEventListener("pinrail:theme-toggle", onThemeToggle);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:command", onCommand);
      window.removeEventListener("pinrail:theme-toggle", onThemeToggle);
    };
  }, [navigate, update, toggleSidebar]);

  // the sidebar's width: dragged by its edge, or with the arrow keys on it
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

  return (
    <div
      className={`app-frame ${overlayTitleBar ? "has-overlay-bar" : ""} ${sidebar ? "" : "sidebar-closed"}`}
      style={{ "--sidebar-width": `${sidebarWidth}px` } as CSSProperties}
    >
      {sidebar ? (
        <aside className="app-sidebar" aria-label="Workspace">
          <div
            className="panel-resizer sidebar-resizer"
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize the sidebar"
            aria-valuenow={sidebarWidth}
            aria-valuemin={SIDEBAR_MIN}
            aria-valuemax={SIDEBAR_MAX}
            tabIndex={0}
            onPointerDown={resizeSidebar}
            onKeyDown={resizeSidebarBy}
            onDoubleClick={() => keepSidebarWidth(SIDEBAR_DEFAULT)}
          />
          <div className="sidebar-bar" data-tauri-drag-region>
            {sidebarButton}
          </div>
          <div className="sidebar-scroll" data-testid="sidebar-scroll">
            <nav className="app-nav" aria-label="Main">
              {SIDEBAR.map(({ key, label, to, Icon, keys }) => (
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
                    </NavLink>
                  </Tooltip>
                  {key === "sessions" ? (
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
            <SidebarSessions shown={shown} />
            <UpdateNotice onDetails={() => setSettings("about")} />
            <button type="button" className="sidebar-feedback" onClick={() => setFeedback(true)}>
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
                  onClick={() => setSettings("general")}
                  aria-label="Settings"
                >
                  <Settings size={15} />
                </button>
              </Tooltip>
            </div>
          </div>
        </aside>
      ) : null}
      <div className="app-shell">
        <header className="app-topbar" data-tauri-drag-region data-testid="top-bar">
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
          <Tooltip label="Keyboard shortcuts" keys={["?"]}>
            <button type="button" className="bar-button" onClick={() => setHelp(true)} aria-label="Keyboard shortcuts">
              <Keyboard size={16} />
            </button>
          </Tooltip>
          <Tooltip label={theme === "dark" ? "Light theme" : "Dark theme"} keys={[MOD, SHIFT, "L"]}>
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
      {welcome ? <WelcomeDialog at={welcome} onClose={closeWelcome} onOpenReview={openFromWelcome} /> : null}
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
        onWhatsNew={
          latest.length
            ? () => {
                setSettings(null);
                openWhatsNew();
              }
            : undefined
        }
      />
      {help ? <ShortcutsDialog plugin={topbar?.plugin} onClose={() => setHelp(false)} /> : null}
      {whatsNew ? <WhatsNewDialog releases={whatsNew} onAction={openFromWhatsNew} onClose={closeWhatsNew} /> : null}
      {feedback ? (
        <FeedbackDialog
          draft={unsent?.draft}
          refusal={unsent?.reason}
          onClose={() => {
            setFeedback(false);
            setUnsent(null);
          }}
          onSend={(form, draft) => {
            setFeedback(false);
            setUnsent(null);
            toast("Sending your feedback…");
            sendFeedback(form).then(
              () => {
                rememberFeedbackEmail(draft.email.trim());
                toast("Thank you. Your feedback was sent.");
              },
              (e: unknown) => {
                const reason = e instanceof Error ? e.message : "The feedback was not sent.";
                // a dialog opened since holds another report; leave it as it is
                if (feedbackOpen.current) {
                  toast(reason, "danger");
                  return;
                }
                setUnsent({ draft, reason });
                setFeedback(true);
              },
            );
          }}
        />
      ) : null}
      <Toasts />
    </div>
  );
}

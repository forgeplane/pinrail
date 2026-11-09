import {
  ArrowLeft,
  ArrowRight,
  Ban,
  Blocks,
  Hand,
  History,
  Inbox,
  Keyboard,
  LayoutGrid,
  ListChecks,
  ListPlus,
  MessageSquare,
  Moon,
  PanelLeft,
  Settings,
  Sparkles,
  Sun,
  SunMoon,
  Palette,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router";
import { sendFeedback } from "../api/client";
import { overlayTitleBar } from "../lib/native";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import { SettingsDialog, type SettingsSection } from "./settings/SettingsDialog";
import { Toasts } from "./Toasts";
import { ShortcutsDialog } from "./ShortcutsDialog";
import { FeedbackDialog, rememberFeedbackEmail, type FeedbackDraft } from "./FeedbackDialog";
import { MOD, SHIFT, modalOpen, isTyping } from "../lib/keys";
import { QuickAddDialog } from "./QuickAddDialog";
import { HISTORY_PATH, Sidebar, useSidebarWidth } from "./Sidebar";
import { useWelcome } from "./useWelcome";
import { useWhatsNew } from "./useWhatsNew";
import { useAppCommands, type CommandRun } from "./useAppCommands";
import { command, type CommandId } from "../lib/commands";
import { reviewPath, reviewShown } from "../lib/url";
import { useLive } from "../state/live";
import { useToast } from "../state/toasts";
import { useSettings } from "../state/settings";
import { useTopBarContent } from "../state/topbar";
import { toggleTheme, useTheme } from "../lib/theme";
import { chooseTheme, chosenTheme } from "../lib/themes";
import { Tooltip } from "./Tooltip";
import { WelcomeDialog } from "./welcome/WelcomeDialog";
import { WhatsNewDialog } from "./WhatsNewDialog";

// On macOS the window has no title bar of its own: the traffic lights sit
// over the sidebar's first row and the bars are the drag handles.

const pageTitle = (path: string) =>
  path === "/"
    ? "Inbox"
    : path.startsWith("/sessions/")
      ? "Session"
      : path.startsWith("/sessions")
        ? "Sessions"
        : path.startsWith("/projects")
          ? "Projects"
          : path.startsWith("/items")
            ? "Work items"
            : "Review";

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
  // ⌘⇧C adds a work item: in a dialog, or on the Work items page in its own
  // quick add
  const [quickAdd, setQuickAdd] = useState(false);
  const onItemsPage = useRef(false);
  onItemsPage.current = location.pathname === "/items";
  const startQuickAdd = useCallback(() => {
    if (onItemsPage.current) window.dispatchEvent(new CustomEvent("pinrail:quick-add"));
    else setQuickAdd(true);
  }, []);
  const [settings, setSettings] = useState<SettingsSection | null>(null);
  const [settingsPlugin, setSettingsPlugin] = useState<string | null>(null);
  const { settings: prefs, themes, update } = useSettings();
  const sidebar = prefs.sidebar.open;
  // sessions, their page and their list in the sidebar, are in Settings › Labs
  const sessionsOn = prefs.labs.sessions;
  const sessionsOnRef = useRef(sessionsOn);
  sessionsOnRef.current = sessionsOn;
  const toggleSidebar = useCallback(() => update((s) => ({ sidebar: { open: !s.sidebar.open } })), [update]);
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

  const { whatsNew, latest, openWhatsNew, openWhatsNewRef, closeWhatsNew, openFromWhatsNew } = useWhatsNew({
    onSettings: (section) => setSettings(section),
  });

  const { welcome, setWelcome, closeWelcome, openFromWelcome } = useWelcome();

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
      if (event.defaultPrevented) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "Escape") {
        setHelp(false);
        // Esc leaves a page's search field, keeping what was typed, so
        // the page's keys act again; a dialog's field has its own Esc
        const field = event.target as HTMLElement | null;
        if (field?.matches?.('input[type="search"]') && !field.closest('[role="dialog"]')) {
          // the browser would empty the field: the search stays, to move through its results
          event.preventDefault();
          field.blur();
        }
        return;
      }
      if (isTyping(event.target)) return;
      if (event.key === "?") {
        // the key must not land in the dialog's search field
        event.preventDefault();
        setHelp((h) => !h);
      }
    };
    const onThemeToggle = (event: Event) =>
      update({ appearance: { theme: (event as CustomEvent<"dark" | "light">).detail } });
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:theme-toggle", onThemeToggle);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:theme-toggle", onThemeToggle);
    };
  }, [update]);

  /** ⌥↓ and ⌥↑ walk the waiting reviews from wherever you are, in a circle;
   *  the inbox handles them itself while a review is open in its side
   *  panel. Nothing happens, and the keys keep their use, when none waits. */
  const walkWaiting = (by: 1 | -1) => {
    const list = waitingRef.current;
    if (!list.length) return false;
    const at = list.findIndex((r) => r.id === shownRef.current);
    const next = at < 0 ? (by > 0 ? 0 : list.length - 1) : (at + by + list.length) % list.length;
    navigate(reviewPath(list[next].id, openInRef.current));
  };
  const runs: Partial<Record<CommandId, CommandRun>> = {
    search: () => setPalette((p) => !p),
    "go-inbox": () => void navigate("/"),
    // Sessions is a page only while it is on in Settings › Labs
    "go-sessions": () => (sessionsOn ? void navigate("/sessions") : false),
    "go-items": () => void navigate("/items"),
    "go-projects": () => void navigate("/projects"),
    "go-history": () => void navigate(HISTORY_PATH),
    "go-plugins": () => setSettings("plugins"),
    "next-waiting": () => walkWaiting(1),
    "previous-waiting": () => walkWaiting(-1),
    "add-item": () => {
      if (!modalOpen()) startQuickAdd();
    },
    settings: () => setSettings((s) => (s ? null : "general")),
    "toggle-theme": () => void toggleTheme(),
    "toggle-sidebar": () => void toggleSidebar(),
    back: () => void navigate(-1),
    forward: () => void navigate(1),
    shortcuts: () => setHelp(true),
    "whats-new": () => openWhatsNewRef.current(),
    feedback: () => setFeedback(true),
  };
  useAppCommands(runs);
  /** A palette entry for a command of the table: its label and keys. */
  const fromCommand = (id: CommandId, icon: LucideIcon): PaletteAction => {
    const c = command(id);
    return { id, label: c.label ?? id, keys: c.keys?.[0], icon, run: () => void runs[id]?.() };
  };

  const actions: PaletteAction[] = [
    fromCommand("go-inbox", Inbox),
    {
      id: "oldest",
      label: "Open the oldest pending review",
      icon: Inbox,
      run: () => {
        const oldest = live.pending[live.pending.length - 1];
        navigate(oldest ? reviewPath(oldest.id, prefs.inbox.open_in) : "/");
      },
    },
    fromCommand("go-history", History),
    fromCommand("go-items", ListChecks),
    { ...fromCommand("add-item", ListPlus), run: startQuickAdd },
    fromCommand("go-projects", LayoutGrid),
    {
      id: "new-project",
      label: "New project…",
      icon: LayoutGrid,
      run: () => navigate("/projects", { state: { newProject: true } }),
    },
    // each project, to see what waits in it
    ...live.namedProjects.map<PaletteAction>((project) => ({
      id: `project-${project.id}`,
      label: `Show ${project.name} in the inbox`,
      icon: LayoutGrid,
      run: () => navigate(`/?project=${encodeURIComponent(project.id)}`),
    })),
    fromCommand("go-plugins", Blocks),
    {
      ...fromCommand("toggle-theme", SunMoon),
      label: theme === "dark" ? "Switch to the light theme" : "Switch to the dark theme",
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
    { ...fromCommand("toggle-sidebar", PanelLeft), label: sidebar ? "Hide the sidebar" : "Show the sidebar" },
    fromCommand("shortcuts", Keyboard),
    fromCommand("settings", Settings),
    { id: "welcome", label: "Set up Pinrail", icon: Hand, run: () => setWelcome({ step: 0 }) },
    ...(latest.length ? [fromCommand("whats-new", Sparkles)] : []),
    fromCommand("feedback", MessageSquare),
    ...(reviewShown(location.pathname, location.search)
      ? [
          {
            id: "discard",
            label: "Discard this review",
            keys: [MOD, "D"],
            icon: Ban,
            run: () => window.dispatchEvent(new Event("pinrail:discard")),
          },
          {
            id: "delete-review",
            label: "Delete this review…",
            icon: Trash2,
            run: () => window.dispatchEvent(new Event("pinrail:delete-review")),
          },
        ]
      : []),
  ];

  const sidebarWidth = useSidebarWidth();

  return (
    <div
      className={`app-frame ${overlayTitleBar ? "has-overlay-bar" : ""} ${sidebar ? "" : "sidebar-closed"}`}
      style={{ "--sidebar-width": `${sidebarWidth.width}px` } as CSSProperties}
    >
      {sidebar ? (
        <Sidebar
          width={sidebarWidth}
          toggle={sidebarButton}
          sessionsOn={sessionsOn}
          shown={shown}
          openIn={prefs.inbox.open_in}
          openItems={live.openItems}
          onSearch={() => setPalette(true)}
          onSettings={setSettings}
          onFeedback={() => setFeedback(true)}
        />
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
      {quickAdd ? <QuickAddDialog onClose={() => setQuickAdd(false)} /> : null}
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

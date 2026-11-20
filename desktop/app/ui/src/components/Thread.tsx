// An agent session's thread: the questions it asked, as the agent's
// messages, and the reviews it sent, oldest first. Questions are answered
// here, each with its chips or a reply box, and sent as a set.
import { ArrowDown, Expand, GitBranch, MoreHorizontal, PanelRightClose, PanelRightOpen, Pencil, X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate, type To } from "react-router";
import { api, errorMessage } from "../api/client";
import { useToast } from "../state/toasts";
import { useNotice } from "../state/live";
import { elementOf, useElements } from "../lib/elements";
import { ElementTile } from "./ElementTile";
import type { Review, Session } from "../api/types";
import { age, stamp } from "../lib/format";
import { MOD, SHIFT, hasMod, isMac, modalOpen, isTyping } from "../lib/keys";
import { isViewToggle } from "../lib/terminalKeys";
import { carryRun, useSessionRun } from "./session/useSessionRun";
import { useSessionView } from "./session/useSessionView";
import { ViewSwitch } from "./session/ViewSwitch";
import { useSplit } from "./session/useSplit";
import { FailedStart } from "./session/FailedStart";
import { EndedRun } from "./session/EndedRun";
import { OpenInPinrail, ResumeButton, StopButton } from "./session/RunControls";
import { MovedRun } from "./session/MovedRun";
import { OutsideRun } from "./session/OutsideRun";
import { WaitMark } from "./WaitMark";
import { runtime } from "../api/runtime";
import { sessionGrid } from "./session/startRun";
import { canOpenWhileRunning, canResume, resumeProblem } from "../lib/resume";
import { stateOf } from "../lib/sidebarSessions";
import { isStopKey } from "../lib/terminalKeys";
import { TerminalPane } from "./terminal/TerminalPane";
import { finder, type Finder, type Found } from "../lib/findText";
import { usePageRepo } from "../lib/pageRepo";
import { countFeature } from "../lib/usage";
import { FindBar, useFindBar } from "./FindBar";
import { useUrlParams } from "../lib/url";
import {
  timeline as entriesOf,
  entryKey,
  fromSession,
  newestRounds,
  generatedBeside,
  sessionLabel,
  stageBeside,
  stages,
  waitsOnAgent,
  type Entry,
} from "../lib/sessions";
import { useSettings } from "../state/settings";
import { ItemOpener, itemPath, useItemLinkClick } from "./ItemChip";
import { SessionMenu } from "./SessionMenu";
import { AgentIcon, agentOf } from "./AgentIcon";
import { useToggleArchive } from "./ArchiveButton";
import { PresenceChip } from "./PresenceChip";
import { useMarkSeen } from "./thread/useMarkSeen";
import { SessionItemPanel } from "./SessionItemPanel";
import { SidePanel } from "./SidePanel";
import { Tooltip } from "./Tooltip";
import { ThreadEntries } from "./thread/ThreadEntries";
import { WorkingLine } from "./thread/runLines";
import { IdleComposer, MessageComposer } from "./thread/composers";
import { ThreadDetails } from "./thread/ThreadDetails";
import { useSessionThread } from "./thread/useSessionThread";
import { useNewestFirst } from "./thread/useNewestFirst";
import { placeOf, useThreadScroll } from "./thread/useThreadScroll";
import { store, stored } from "./thread/storage";
import { useWindowKey } from "../lib/useWindowKey";
import { useResizableWidth } from "../lib/useResizableWidth";
import { useLatest } from "../lib/useLatest";
import { readStored } from "../lib/storage";

/** Whether the session's details show beside the thread, and their width,
 *  stored on this machine. */
const DETAILS_SHOWN = "pinrail.thread.details";
/** The session header's and the terminal bar's heights. */
const HEAD_HEIGHT = 72;
const TERMINAL_BAR_HEIGHT = 26;
/** Whether a side panel shows the session's details, which it hides at first. */
const DETAILS_SHOWN_PANEL = "pinrail.thread.panel-details";
const DETAILS_WIDTH = "pinrail.thread.detailsWidth";
const DETAILS_MIN = 220;
const DETAILS_MAX = 520;

type ThreadProps = {
  /** the questions opened, which the thread shows within their session */
  focus?: Review;
  /** the session to show, when it is not the focus's */
  session?: { agent: string; id: string };
  inPanel?: boolean;
  onClose?: () => void;
  /** called with the session each time it is loaded */
  onSession?: (session: Session) => void;
  /** called when the session's run goes on in another session, as after
   *  /clear, with that session and the view shown, for the page to follow */
  onMoved?: (session: { agent: string; id: string }, view: string) => void;
  /** the start of a stage to open the thread at, by its status's id */
  stage?: number;
  /** the review shown in the side panel beside the thread, when reviews
   *  open in a panel: the session's page passes it */
  reviewPanel?: (panel: ReviewPanelProps) => ReactNode;
};

/** What the side panel beside a thread needs to show a review. */
export type ReviewPanelProps = {
  id: string;
  /** the link to another round of the review */
  linkTo: (id: string) => To;
  /** the review was decided or discarded */
  onEnded: () => void;
  onClose: () => void;
};

/** A session's page. */

export const sessionPath = (s: { agent: string; id: string }) =>
  `/sessions/${encodeURIComponent(s.agent)}/${encodeURIComponent(s.id)}`;

/** The space a session page's terminal has in the main area `main`: less
 *  the header and the terminal bar, and the details when they are shown. */
export function terminalArea(main: DOMRect): { width: number; height: number } {
  const details = stored(DETAILS_SHOWN) !== "hidden" ? Number(readStored(DETAILS_WIDTH)) || 280 : 0;
  return {
    width: Math.max(0, main.width - details),
    height: Math.max(0, main.height - HEAD_HEIGHT - TERMINAL_BAR_HEIGHT),
  };
}

/** Where each session's thread was left, by `agent/id`, the newest last. */
const places = new Map<string, { key: string; offset: number }>();
const PLACES_KEPT = 50;

/** The session of a review, when the agent that sent it said which. */
export function sessionOf(review: Review): { agent: string; id: string } | null {
  return review.requested_by && review.session ? { agent: review.requested_by, id: review.session } : null;
}

export function Thread({
  focus,
  session: given,
  inPanel = false,
  onClose,
  onSession,
  onMoved,
  stage,
  reviewPanel,
}: ThreadProps) {
  const { settings: prefs } = useSettings();
  const [params, updateParams, currentParams] = useUrlParams();
  // with reviews opening in a panel, ?review= names the review shown beside
  // the thread, in place of the session's details
  const panelMode = !!reviewPanel && prefs.inbox.open_in === "panel";
  const openReview = panelMode ? params.get("review") : null;
  // and ?item= the work item shown there, as a review is
  const openItem = panelMode ? params.get("item") : null;
  /** Shows a review beside the thread, or closes the panel with null. A
   *  review shown in place of another, or of an item, replaces its history
   *  entry. */
  const showReview = useCallback(
    (id: string | null, options?: { replace?: boolean }) =>
      updateParams(
        (next) => {
          next.delete("item");
          if (id) next.set("review", id);
          else next.delete("review");
        },
        {
          replace: options?.replace ?? (!!id && (currentParams().has("review") || currentParams().has("item"))),
        },
      ),
    [updateParams, currentParams],
  );
  /** Shows a work item beside the thread, or closes the panel with null. */
  const showItem = useCallback(
    (key: string | null) =>
      updateParams(
        (next) => {
          next.delete("review");
          if (key) next.set("item", key);
          else next.delete("item");
        },
        { replace: !!key && (currentParams().has("review") || currentParams().has("item")) },
      ),
    [updateParams, currentParams],
  );
  const onItemLink = (event: React.MouseEvent, key: string) => {
    if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    showItem(key);
  };
  /** Where a link to a review goes: the panel, or the review's page. */
  const reviewTo = useCallback(
    (id: string): To => {
      if (!panelMode) return `/reviews/${id}`;
      const next = new URLSearchParams(params);
      next.set("review", id);
      return { search: `?${next}` };
    },
    [panelMode, params],
  );
  /** A plain click on a review's link, which opens it in the panel. */
  const openFromLink = useCallback(
    (event: React.MouseEvent, id: string) => {
      if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      showReview(id);
    },
    [panelMode, showReview],
  );
  const body = useRef<HTMLDivElement>(null);
  const named = given ?? (focus ? sessionOf(focus) : null);
  const {
    session,
    rename,
    items,
    statuses,
    messages,
    comments,
    itemEvents,
    turns,
    error,
    load,
    sessionItems,
    loaded,
    kept,
  } = useSessionThread(named, focus, onSession);
  // where the person left this session's thread, when it opens as it was
  // shown then, at no question or stage
  const placeKey = named ? `${named.agent}/${named.id}` : null;
  // a message goes to the session by Pinrail's id: an old address names it
  // by its agent's own id until the session has loaded
  const composeTo = session ? { agent: session.agent, id: session.id } : named?.id.startsWith("ses_") ? named : null;
  const [place] = useState(() => (kept && placeKey && !focus && !stage ? places.get(placeKey) : undefined));
  const { markUnread } = useMarkSeen(session);
  // a session's page shows its live run's terminal beside the summary
  const onPage = !focus && !inPanel;
  const {
    live,
    failed,
    ended,
    runs,
    movedTo,
    reload: reloadRuns,
    loaded: runsLoaded,
  } = useSessionRun(onPage ? named : null, prefs.labs.runtime);
  const terminal = live?.terminal ?? null;
  // until its runs have loaded, whether it runs in Pinrail is not known: the
  // terminal's place shows that it loads, and nothing offers to open or
  // resume it
  const runsLoading = onPage && prefs.labs.runtime && !!named && !runsLoaded;
  // a stopped session can be resumed in Pinrail, also one that ran outside it
  const resumable = !runsLoading && onPage && prefs.labs.runtime && canResume(session, runs);
  // the run went on in another session, as after /clear, and the terminal
  // with it
  const movedOn = !live && movedTo?.session ? movedTo.session : null;
  // one still running in another terminal can be opened here, after a
  // warning, unless what runs is Pinrail's run, which went on elsewhere
  const openable = !runsLoading && onPage && prefs.labs.runtime && !movedOn && canOpenWhileRunning(session, runs);
  // without hooks, nothing tells whether it still runs there
  const unsure = openable && !session?.presence;
  const [askingOpen, setAskingOpen] = useState(false);
  const outside = openable || (resumable && runs.length === 0);
  // a run that ended shows how in the terminal's place
  const hasTerminalView = runsLoading || !!terminal || !!failed || !!ended || !!movedOn || outside;
  const { view, choose: chooseView, toggle: toggleView, switches } = useSessionView(named, hasTerminalView);
  // the run goes on in another session, as after /clear: the page follows
  // its terminal there
  useNotice((notice) => {
    const to = notice.kind === "run_bound" ? notice.run?.session : null;
    const from = notice.run?.previous;
    if (!onPage || !named || !to || !from || !onMoved) return;
    if (from.agent !== named.agent || from.id !== named.id || to.id === named.id) return;
    carryRun(named, to);
    onMoved(to, view);
  });
  const [resuming, setResuming] = useState(false);
  // why the last resume did not happen, for the session it was of
  const sessionKey = named ? `${named.agent}/${named.id}` : null;
  const [resumeFailure, setResumeFailure] = useState<{ key: string | null; message: string } | null>(null);
  const resumeError = resumeFailure?.key === sessionKey ? resumeFailure.message : null;
  const resume = async (anyway = false) => {
    if (!named) return;
    setResuming(true);
    setResumeFailure(null);
    try {
      await runtime.resume(named, { ...(await sessionGrid()), local_id: crypto.randomUUID(), anyway });
      reloadRuns();
      chooseView("terminal");
    } catch (e) {
      setResumeFailure({ key: sessionKey, message: resumeProblem(e, runs[0]?.folder ?? session?.folder ?? null) });
    } finally {
      setResuming(false);
    }
  };
  // the terminal over the summary, or under it in the split
  const showTerminal = view === "terminal" && hasTerminalView;
  const split = view === "split" && hasTerminalView;
  const splitRows = useSplit();
  const showTerminalRef = useLatest(showTerminal);
  // what a live run's agent waits for the person to allow, if anything
  const wait = session?.presence?.state === "needs_input" ? session.presence.wait : null;
  const waitingFor = live && wait?.kind === "permission" ? [wait.tool, wait.summary].filter(Boolean).join(" · ") : null;
  // Stop asks first while the agent works, and always from its key
  const [askingStop, setAskingStop] = useState(false);
  const sessionNow = useLatest(session);
  // asked when Stop is pressed
  const working = () => !!sessionNow.current && stateOf(sessionNow.current) === "working";
  useWindowKey(
    (event) => {
      if (!isStopKey(event, isMac) || modalOpen()) return;
      event.preventDefault();
      event.stopPropagation();
      setAskingStop(true);
    },
    { capture: true, enabled: !!live },
  );
  // Ctrl+` and MOD+Shift+T switch the view, before the terminal sees them,
  // so it never keeps the keyboard
  useWindowKey(
    (event) => {
      if (!isViewToggle(event, isMac) || modalOpen()) return;
      event.preventDefault();
      event.stopPropagation();
      toggleView();
    },
    { capture: true, enabled: hasTerminalView },
  );
  // switching to the summary puts the focus in its chat box
  useEffect(() => {
    if (switches === 0 || view !== "summary") return;
    document.querySelector<HTMLElement>(".thread-footer textarea")?.focus();
  }, [switches, view]);
  // what the session holds now, in its header
  const held = sessionItems.filter(
    (i) => i.status === "in_progress" && i.claim?.agent === named?.agent && i.claim?.session === named?.id,
  );
  const navigate = useNavigate();
  // renaming the session in its header, which Rename in a session's menu
  // elsewhere asks for as it opens the page
  const routed = useLocation();
  const element = elementOf(useElements(), session?.element);
  const asksRename = (routed.state as { rename?: boolean } | null)?.rename === true;
  const [renaming, setRenaming] = useState(asksRename);
  const [routedKey, setRoutedKey] = useState(routed.key);
  if (routed.key !== routedKey) {
    setRoutedKey(routed.key);
    if (asksRename) setRenaming(true);
  }
  // the request is answered: a reload or Back does not ask again
  useEffect(() => {
    if (asksRename) navigate(`${routed.pathname}${routed.search}`, { replace: true, state: null });
  }, [asksRename, routed.pathname, routed.search, navigate]);
  // a side panel has details only for a question asked in a session
  const hasDetails = !inPanel || !!named;

  // the questions the keys and the chat box answer: those picked, else the
  // ones opened, else the first waiting
  const [picked, setPicked] = useState<string | null>(null);
  const active = useMemo(() => {
    const waitingQuestions = (id: string | null | undefined) =>
      !!id && items.some((r) => r.id === id && r.kind === "question" && r.status === "pending");
    if (waitingQuestions(picked)) return picked;
    if (waitingQuestions(focus?.id)) return focus!.id;
    return items.find((r) => r.kind === "question" && r.status === "pending")?.id ?? null;
  }, [items, focus, picked]);
  // where the active questions put their chat box, under the timeline
  const [footer, setFooter] = useState<HTMLDivElement | null>(null);

  // the timeline: scrolled to what was opened, with room for the chat box
  // over it, and whether it shows its newest message
  const timeline = useRef<HTMLDivElement>(null);
  const { atEnd, pinned, holdAt } = useThreadScroll(timeline, footer, {
    focus,
    items,
    // the thread is there, not only the session a list gave
    session: loaded ? session : null,
    stage,
    placed: !!place,
  });
  // the entries drawn and the element before them, if any, to find an
  // entry's element by its key and back
  const drawnRef = useRef<{ shown: Entry[]; leading: number }>({ shown: [], leading: 0 });
  // where it is left, for the next time it is opened
  useLayoutEffect(() => {
    const el = timeline.current;
    return () => {
      if (!placeKey || !el) return;
      // whether it is at its end as it is left, not as it opened
      // eslint-disable-next-line react-hooks/exhaustive-deps
      const left = placeOf(el, pinned.current);
      const { shown, leading } = drawnRef.current;
      const entry = left ? shown[[...el.children].indexOf(left.element) - leading] : undefined;
      places.delete(placeKey);
      if (left && entry) places.set(placeKey, { key: entryKey(entry), offset: left.offset });
      while (places.size > PLACES_KEPT) places.delete(places.keys().next().value!);
    };
  }, [placeKey, pinned]);

  // ⌘F finds text in the thread, unless the focus is in the panel beside
  // it, where a review's view has its own find
  const {
    open: finding,
    focus: findFocus,
    query: findQuery,
    setQuery: setFindQuery,
    show: showFind,
    hide: hideFind,
  } = useFindBar();
  useEffect(() => {
    if (finding) countFeature("find_in_thread");
  }, [finding]);
  const [found, setFound] = useState<Found | null>(null);
  const finderRef = useRef<Finder | null>(null);
  useWindowKey((event) => {
    if (!hasMod(event) || event.shiftKey || event.altKey || (event.key !== "f" && event.key !== "F")) return;
    if (showTerminalRef.current) return;
    // the thread may itself be in the Sessions page's panel
    const panel = document.activeElement?.closest(".review-panel");
    if (modalOpen() || (panel && !panel.contains(timeline.current))) return;
    event.preventDefault();
    showFind();
  });
  useEffect(() => {
    const el = timeline.current;
    if (!finding || !el) return;
    const made = finder(el, setFound);
    finderRef.current = made;
    return () => {
      made.stop();
      finderRef.current = null;
    };
  }, [finding]);
  const find = useCallback((text: string, backwards?: boolean) => {
    const found = finderRef.current?.find(text, backwards) ?? null;
    setFound(text ? found : null);
  }, []);
  const closeFind = () => {
    hideFind();
    setFound(null);
  };

  const agent = named?.agent ?? focus?.requested_by ?? null;
  // the agent at work, as its hooks report it, which the thread says at
  // its end
  const agentWorking = !!session?.presence && session.attention === "working";
  const agentName = agentOf(agent)?.label ?? agent ?? "The agent";
  // a review opened from here names the session in its breadcrumb
  // the same objects from one render to the next, for the memoized entries
  const namedRef = useMemo(
    () => (named ? { agent: named.agent, id: named.id } : null),
    [named?.agent, named?.id], // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the object
  );
  const from = useMemo(
    () => (namedRef ? fromSession({ ...namedRef, title: session ? sessionLabel(session) : null }) : undefined),
    [namedRef, session],
  );
  // each review once, by its newest round
  const newest = useMemo(() => newestRounds(items), [items]);
  const waiting = newest.filter((r) => r.status === "pending");
  const started = useMemo(() => stages(statuses), [statuses]);
  // the reviews each stage sent, for its tooltip
  // the messages the agent has not received yet, oldest first, which the
  // message box shows at its top
  const waitingMessages = useMemo(() => messages.filter(waitsOnAgent), [messages]);
  const entries = useMemo(
    () => entriesOf(items, statuses, messages, comments, itemEvents, turns, runs),
    [items, statuses, messages, comments, itemEvents, turns, runs],
  );
  // a long thread draws its newest entries, and the rest as the person
  // scrolls up or, a chunk a frame, while find searches them, unless it
  // opens at one further up
  const openedAt = useMemo(() => {
    if (focus)
      return entries.findIndex(
        (entry) => (entry.kind === "item" || entry.kind === "revised") && entry.review.id === focus.id,
      );
    if (stage) return entries.findIndex((entry) => entry.kind === "started" && entry.status.id === stage);
    return place ? entries.findIndex((entry) => entryKey(entry) === place.key) : -1;
  }, [entries, focus, stage, place]);
  const { shown, whole, drawAll } = useNewestFirst(entries, timeline, {
    // the entry it opens at, the question or stage asked for or the one it
    // was left at, and older ones above it, so that it opens away from the
    // top, where older ones would be drawn at once; every entry when that
    // entry is not among them
    least: openedAt >= 0 ? entries.length - openedAt + 100 : 0,
    all: (!!focus || !!stage) && openedAt < 0,
    grow: finding,
    thread: named ? `${named.agent}/${named.id}` : null,
    pinned,
  });
  useLayoutEffect(() => {
    drawnRef.current = { shown, leading: loaded && session && whole ? 1 : 0 };
  });
  // where it was left, once it is drawn
  useLayoutEffect(() => {
    if (!place || !session || !loaded) return;
    const i = shown.findIndex((entry) => entryKey(entry) === place.key);
    const leading = whole ? 1 : 0;
    holdAt(i < 0 ? null : (timeline.current?.children[leading + i] ?? null), place.offset);
  }, [place, session, loaded, shown, whole, holdAt]);

  /** Scrolls to an entry's element, drawing every entry first if it is not. */
  const jump = useCallback(
    (id: string, block: ScrollLogicalPosition) => {
      if (!document.getElementById(id)) drawAll();
      document.getElementById(id)?.scrollIntoView({ block, behavior: "smooth" });
    },
    [drawAll],
  );
  // a jump asked for from the Terminal view shows the summary first, and
  // scrolls once it is shown
  const [pendingJump, setPendingJump] = useState<{ id: string; block: ScrollLogicalPosition } | null>(null);
  const jumpInSummary = useCallback(
    (id: string, block: ScrollLogicalPosition) => {
      if (!showTerminalRef.current) return jump(id, block);
      setPendingJump({ id, block });
      chooseView("summary");
    },
    [jump, chooseView, showTerminalRef],
  );
  useEffect(() => {
    if (!pendingJump || showTerminal) return;
    const frame = requestAnimationFrame(() => {
      jump(pendingJump.id, pendingJump.block);
      setPendingJump(null);
    });
    return () => cancelAnimationFrame(frame);
  }, [pendingJump, showTerminal, jump]);

  // the session's details beside the thread: shown or hidden with the
  // button or ⌘⇧B, and as wide as their edge is dragged; a side panel keeps
  // its own choice, and hides them at first
  const shownKey = inPanel ? DETAILS_SHOWN_PANEL : DETAILS_SHOWN;
  const [details, setDetails] = useState(() =>
    inPanel ? stored(DETAILS_SHOWN_PANEL) === "shown" : stored(DETAILS_SHOWN) !== "hidden",
  );
  const {
    width: storedDetailsWidth,
    onKeyDown: resizeDetailsBy,
    edgeProps: detailsEdge,
  } = useResizableWidth({
    key: DETAILS_WIDTH,
    min: DETAILS_MIN,
    max: DETAILS_MAX,
    fallback: 280,
    grows: "left",
    anchor: (edge) => edge.parentElement?.getBoundingClientRect().right ?? window.innerWidth,
    // while dragging, the width goes to the CSS variable alone: the thread
    // is not drawn again for every pointer move
    preview: (width) => body.current?.style.setProperty("--details-width", `${width}px`),
  });
  const detailsWidth = storedDetailsWidth ?? 280;
  const toggleDetails = useCallback(() => {
    setDetails((shown) => {
      store(shownKey, shown ? "hidden" : "shown");
      return !shown;
    });
  }, [shownKey]);
  useWindowKey(
    (event) => {
      if (modalOpen() || !hasMod(event) || !event.shiftKey || event.altKey) return;
      if (event.key !== "b" && event.key !== "B") return;
      event.preventDefault();
      toggleDetails();
    },
    { enabled: !!hasDetails },
  );
  // E archives the session open on its page or in the side panel, or brings
  // it back, when the focus is not in a text field. With a work item open
  // beside the thread, E is the item's, which edits its description.
  const toggleArchive = useToggleArchive();
  const archivable = !focus && !openItem && named && session ? { ...named, archived_at: session.archived_at } : null;
  const archivableRef = useLatest(archivable);
  useWindowKey(
    (event) => {
      if (event.key !== "e" && event.key !== "E") return;
      if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      if (isTyping(el)) return;
      if (!archivableRef.current) return;
      event.preventDefault();
      toggleArchive(archivableRef.current);
    },
    { enabled: !focus },
  );
  const repo = session?.repo ?? focus?.origin.repo ?? null;
  usePageRepo(repo);
  const ref = session?.ref ?? focus?.origin.ref ?? null;

  return (
    <ItemOpener.Provider value={panelMode ? showItem : null}>
      <div className={`thread ${inPanel ? "in-panel" : ""}`} data-testid="thread">
        <header className="thread-head">
          {/* the element it is named after, with its agent on its corner,
            or else the agent */}
          {element ? (
            <ElementTile element={element} size="lg" card badge={<AgentIcon requestedBy={agent} size={10} />} />
          ) : (
            <span className="thread-avatar">
              <AgentIcon requestedBy={agent} size={18} />
            </span>
          )}
          <div className="thread-title">
            {/* the stage the agent works on names the session, with the agent
              and where it works under it; without one, those are the title */}
            <div className="thread-title-row">
              <h1
                className={
                  session && (session.name || session.title || session.generated_name) ? "thread-stage" : undefined
                }
                title={session && !renaming ? sessionLabel(session) : undefined}
              >
                {session && named && !focus ? (
                  <SessionName
                    session={session}
                    editing={renaming}
                    onEdit={setRenaming}
                    onRename={rename}
                    onFailed={load}
                  />
                ) : session && (session.name || session.title || session.generated_name) ? (
                  sessionLabel(session)
                ) : (
                  <Where agent={agent} repo={repo} reference={ref} />
                )}
              </h1>
              {session ? <PresenceChip session={session} /> : null}
              {session?.archived_at ? <span className="archived-chip">Archived</span> : null}
              {/* what can be done with the session, beside its name */}
              {session && !focus ? (
                <SessionMenuButton session={session} markUnread={markUnread} onRename={() => setRenaming(true)} />
              ) : null}
            </div>
            {/* one quiet line, kept when it is short, so that the header
              keeps its height and the terminal under it its size: where it
              works, since when, and the items it holds */}
            <div className="thread-meta">
              {/* the name Pinrail gave it, when another heads it */}
              {session && generatedBeside(session) ? (
                <span className="thread-meta-generated">{generatedBeside(session)}</span>
              ) : null}
              {/* the stage it is on, when a name the person gave heads it */}
              {session && stageBeside(session) ? (
                <span className="thread-meta-stage" title={stageBeside(session) ?? undefined}>
                  {stageBeside(session)}
                </span>
              ) : null}
              {session?.name || session?.title || session?.generated_name ? (
                <span className="thread-meta-where">
                  <Where agent={agent} repo={repo} reference={ref} />
                </span>
              ) : null}
              {session ? <span title={stamp(session.first_at)}>started {age(session.first_at)} ago</span> : null}
              {held.length ? <HeldItems items={held} /> : null}
              {named && focus ? (
                // from a question, the session's own page
                <Link
                  to={sessionPath(named)}
                  className="thread-session"
                  title={session ? `Open ${sessionLabel(session)}` : "Open the session"}
                >
                  open the session
                </Link>
              ) : null}
            </div>
          </div>
          <span className="thread-head-end">
            {live ? (
              <StopButton
                run={live}
                working={working}
                keys={[MOD, SHIFT, "X"]}
                asking={askingStop}
                onAsking={setAskingStop}
              />
            ) : null}
            {resumable && named ? (
              <ResumeButton agent={named.agent} resuming={resuming} onResume={() => void resume()} />
            ) : null}
            {openable && named ? (
              <OpenInPinrail
                agent={named.agent}
                unsure={unsure}
                opening={resuming}
                asking={askingOpen}
                onAsking={setAskingOpen}
                onOpen={() => void resume(true)}
              />
            ) : null}
            {hasTerminalView ? <ViewSwitch view={view} onView={chooseView} /> : null}
            {inPanel && named ? (
              <Tooltip label="Open as a page" side="bottom">
                <button
                  type="button"
                  className="bar-button"
                  onClick={() => navigate(sessionPath(named))}
                  aria-label="Open as a page"
                >
                  <Expand size={15} />
                </button>
              </Tooltip>
            ) : null}
            {hasDetails ? (
              <Tooltip label={details ? "Hide the details" : "Show the details"} keys={[MOD, SHIFT, "B"]} side="bottom">
                <button
                  type="button"
                  className="bar-button"
                  aria-label={details ? "Hide the session's details" : "Show the session's details"}
                  aria-pressed={details}
                  onClick={toggleDetails}
                >
                  {details ? <PanelRightClose size={15} /> : <PanelRightOpen size={15} />}
                </button>
              </Tooltip>
            ) : null}
            {inPanel && onClose ? (
              <Tooltip label="Close" keys={["Esc"]} side="bottom">
                <button type="button" className="bar-button" onClick={onClose} aria-label="Close the panel">
                  <X size={15} />
                </button>
              </Tooltip>
            ) : null}
          </span>
        </header>
        {error ? <p className="notice notice-danger">{error}</p> : null}
        {resumeError ? (
          <p className="notice notice-danger" role="alert">
            {resumeError}
          </p>
        ) : null}
        <div
          className="thread-body"
          ref={body}
          style={{ "--details-width": `${detailsWidth}px` } as React.CSSProperties}
        >
          <div
            className={`thread-views ${split ? "is-split" : ""}`}
            style={split ? { gridTemplateRows: splitRows.rows } : undefined}
            {...(split ? { id: "session-view", role: "tabpanel", "aria-labelledby": "view-tab-split" } : {})}
          >
            {showTerminal || split ? (
              <div
                className="thread-terminal"
                {...(split ? {} : { id: "session-view", role: "tabpanel", "aria-labelledby": "view-tab-terminal" })}
              >
                {runsLoading ? (
                  <div
                    className="thread-terminal-loading"
                    data-testid="terminal-loading"
                    role="status"
                    aria-label="Loading"
                  >
                    <WaitMark />
                  </div>
                ) : terminal ? (
                  <TerminalPane terminal={terminal} autoFocus />
                ) : failed ? (
                  <FailedStart run={failed} />
                ) : movedOn ? (
                  <MovedRun agent={named!.agent} to={movedOn} />
                ) : openable ? (
                  <OutsideRun
                    agent={named!.agent}
                    running
                    unsure={unsure}
                    resuming={resuming}
                    onResume={() => setAskingOpen(true)}
                  />
                ) : ended ? (
                  <EndedRun run={ended} onResume={resumable ? () => void resume() : undefined} resuming={resuming} />
                ) : (
                  <OutsideRun agent={named!.agent} resuming={resuming} onResume={() => void resume()} />
                )}
              </div>
            ) : null}
            <div
              className="thread-main"
              // behind the terminal it stays as it is, which costs nothing
              // to show again; a live terminal keeps the keyboard, Tab
              // included, and anything else in its place leaves the summary
              // out of the keyboard's way
              aria-hidden={showTerminal || undefined}
              inert={showTerminal && !terminal}
              {...(hasTerminalView && !showTerminal && !split
                ? { id: "session-view", role: "tabpanel", "aria-labelledby": "view-tab-summary" }
                : {})}
            >
              {movedOn && !split ? (
                <p className="notice thread-moved" role="status" data-testid="run-moved">
                  The agent went on in a new session, as it does after /clear.{" "}
                  <Link to={sessionPath(movedOn)}>Open the new session</Link>
                </p>
              ) : null}
              {finding ? (
                <FindBar
                  label="Find in the thread"
                  query={findQuery}
                  onQuery={setFindQuery}
                  found={found}
                  onFind={find}
                  onClose={closeFind}
                  focus={findFocus}
                />
              ) : null}
              <div className={`thread-items ${finding ? "is-finding" : ""}`} ref={timeline}>
                <ThreadEntries
                  entries={shown}
                  startedAt={session && whole ? session.first_at : null}
                  named={namedRef}
                  agent={agent}
                  active={active}
                  footer={footer}
                  openReview={openReview}
                  panelMode={panelMode}
                  reviewTo={reviewTo}
                  from={from}
                  load={load}
                  onPick={setPicked}
                  showReview={showReview}
                  openFromLink={openFromLink}
                />
              </div>
              <div className="thread-footer" ref={setFooter}>
                {/* what the agent is doing now, on a line of its own whose
                  height is kept, so that nothing moves as it changes */}
                <div className="thread-state">
                  {waitingFor ? (
                    <div className="thread-waiting" data-testid="waiting-line">
                      <span className="truncate">Waiting for permission: {waitingFor}</span>
                      <button type="button" className="chrome-button" onClick={() => chooseView("terminal")}>
                        Open the terminal
                      </button>
                    </div>
                  ) : agentWorking ? (
                    <WorkingLine name={agentName} />
                  ) : null}
                </div>
                {atEnd ? null : (
                  <button
                    type="button"
                    className="thread-to-end"
                    aria-label="Scroll to the newest message"
                    onClick={() => {
                      const el = timeline.current;
                      el?.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
                    }}
                  >
                    <ArrowDown size={15} />
                  </button>
                )}
                {/* in the split, the terminal is where the person types */}
                {active || split ? null : composeTo ? (
                  <MessageComposer
                    session={composeTo}
                    repo={session?.repo ?? null}
                    typed={!!live && !!session?.types}
                    waiting={waitingMessages}
                    onSent={load}
                  />
                ) : (
                  <IdleComposer />
                )}
              </div>
            </div>
            {split ? (
              <div className="thread-split-divider" {...splitRows.divider}>
                <span aria-hidden="true" />
              </div>
            ) : null}
          </div>
          {openReview && reviewPanel ? (
            <SidePanel split={body}>
              {reviewPanel({
                id: openReview,
                linkTo: reviewTo,
                onEnded: () => showReview(null, { replace: true }),
                onClose: () => showReview(null),
              })}
            </SidePanel>
          ) : null}
          {openItem && !openReview ? (
            <SidePanel split={body} label="Work item">
              <SessionItemPanel key={openItem} itemKey={openItem} onOpen={showItem} onClose={() => showItem(null)} />
            </SidePanel>
          ) : null}
          {!hasDetails || !details || openReview || openItem ? null : (
            <div
              className="panel-resizer thread-aside-resizer"
              {...detailsEdge}
              aria-label="Resize the session's details"
              onKeyDown={(event) => resizeDetailsBy(event)}
            />
          )}
          {!hasDetails || !details || openReview || openItem ? null : (
            <ThreadDetails
              session={session}
              waiting={waiting}
              newest={newest}
              messages={messages}
              sessionItems={sessionItems}
              started={started}
              stage={stage}
              entries={entries}
              onWaiting={(r) => {
                if (r.kind !== "question" && panelMode) showReview(r.id);
                else if (r.kind !== "question") navigate(`/reviews/${r.id}`, { state: from });
                else jumpInSummary(`set-${r.id}`, "start");
              }}
              onJump={(id) => jumpInSummary(id, "center")}
              reviewLink={(r) => ({
                to: reviewTo(r.id),
                state: panelMode ? undefined : from,
                onClick: (e) => openFromLink(e, r.id),
              })}
              onItemLink={onItemLink}
            />
          )}
        </div>
      </div>
    </ItemOpener.Provider>
  );
}

/** The agent and the project it works in, with the branch. */
function Where({ agent, repo, reference }: { agent: string | null; repo: string | null; reference: string | null }) {
  return (
    <>
      {agent ?? "Questions"}
      {repo ? (
        <>
          {" "}
          <span className="faint thread-in">in</span> {repo}
          {reference ? (
            <span className="thread-ref" title={`Branch ${reference}`}>
              <GitBranch size={13} aria-label="Branch" />
              {reference}
            </span>
          ) : null}
        </>
      ) : null}
    </>
  );
}

/** The work items a session holds, on its header's meta line: one by its
 *  key and title, several by their first two keys and how many more, with
 *  every title on hover. */
function HeldItems({ items }: { items: { key: string; title: string }[] }) {
  const onLinkClick = useItemLinkClick();
  const shown = items.length === 1 ? items : items.slice(0, 2);
  const rest = items.slice(shown.length);
  return (
    <span className="thread-meta-items" data-testid="thread-held-item">
      {shown.map((item) => (
        <span key={item.key} className="thread-meta-item">
          <Link
            to={itemPath(item.key)}
            className="item-ref"
            title={`${item.key} ${item.title}`}
            onClick={(e) => onLinkClick(e, item.key)}
          >
            {item.key}
          </Link>
          {items.length === 1 ? <span className="thread-meta-item-title">{item.title}</span> : null}
        </span>
      ))}
      {rest.length ? (
        <span className="thread-meta-more" title={rest.map((i) => `${i.key} ${i.title}`).join("\n")}>
          +{rest.length}
        </span>
      ) : null}
    </span>
  );
}

/** The session's name in its header, which a click turns into a field to
 *  rename it in place: Enter or a click elsewhere saves, Esc cancels, and
 *  an empty name gives back the name Pinrail gave it. */
function SessionName({
  session,
  editing,
  onEdit,
  onRename,
  onFailed,
}: {
  session: Session;
  editing: boolean;
  onEdit: (editing: boolean) => void;
  /** shows the new name, or with null the generated one */
  onRename: (name: string | null) => void;
  onFailed: () => void;
}) {
  const label = sessionLabel(session);
  const toast = useToast();
  // Esc ends the edit before the blur that follows would save it
  const cancelled = useRef(false);
  const save = async (text: string) => {
    onEdit(false);
    const name = text.trim();
    if (cancelled.current || name === label) return;
    // shown at once; a rename that fails loads the session's name again
    onRename(name || null);
    try {
      await api.renameSession(session.agent, session.id, name || null);
    } catch (e) {
      onFailed();
      toast(`Could not rename the session: ${errorMessage(e)}`, "danger");
    }
  };
  if (!editing)
    return (
      <button
        type="button"
        className="thread-name"
        onClick={() => {
          cancelled.current = false;
          onEdit(true);
        }}
        title="Rename"
      >
        <span className="thread-name-text">{label}</span>
        <Pencil size={12} className="thread-name-pencil" aria-hidden="true" />
      </button>
    );
  return (
    <NameField
      label={label}
      onSave={(text) => void save(text)}
      onCancel={() => {
        cancelled.current = true;
        onEdit(false);
      }}
    />
  );
}

/** The field that renames the session, as wide as its text: a hidden copy
 *  of the text beside it sets its width, since WebKit cannot size a field
 *  to its content. */
function NameField({
  label,
  onSave,
  onCancel,
}: {
  label: string;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(label);
  return (
    <span className="thread-name-edit">
      <span className="thread-name-sizer" aria-hidden="true">
        {text || " "}
      </span>
      <input
        className="thread-name-field"
        aria-label="Session name"
        // no width of its own: the copy of the text sets it
        size={1}
        value={text}
        maxLength={60}
        autoFocus
        onChange={(e) => setText(e.target.value)}
        onFocus={(e) => e.currentTarget.select()}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            e.currentTarget.blur();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel();
          }
        }}
        onBlur={(e) => onSave(e.currentTarget.value)}
      />
    </span>
  );
}

/** The ⋯ button of a session's header, with the menu its sidebar row has. */
function SessionMenuButton({
  session,
  markUnread,
  onRename,
}: {
  session: Session;
  markUnread: (() => void) | null;
  onRename: () => void;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <button
        type="button"
        className="bar-button thread-menu"
        aria-label="Session actions"
        aria-haspopup="menu"
        aria-expanded={!!anchor}
        data-testid="thread-menu"
        onClick={(event) => {
          const button = event.currentTarget;
          setAnchor((open) => (open ? null : button));
        }}
      >
        <MoreHorizontal size={15} />
      </button>
      {anchor ? (
        <SessionMenu
          session={session}
          anchor={anchor}
          markUnread={markUnread}
          onRename={onRename}
          onClose={() => setAnchor(null)}
        />
      ) : null}
    </>
  );
}

// An agent session's thread: the questions it asked, as the agent's
// messages, and the reviews it sent, oldest first. Questions are answered
// here, each with its chips or a reply box, and sent as a set.
import {
  ArrowDown,
  Check,
  CheckCircle2,
  ClipboardCheck,
  ClipboardX,
  Clock,
  ChevronRight,
  Copy,
  Expand,
  GitBranch,
  PanelRightClose,
  PanelRightOpen,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useNavigate, type To } from "react-router";
import { ApiError, api } from "../api/client";
import type {
  Review,
  Session,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
  WorkItem,
} from "../api/types";
import { copyText } from "../lib/clipboard";
import { age, stamp } from "../lib/format";
import { MOD, SHIFT, hasMod, modalOpen } from "../lib/keys";
import { finder, type Finder, type Found } from "../lib/findText";
import { usePageRepo } from "../lib/pageRepo";
import { FindBar } from "./FindBar";
import { useUrlParams } from "../lib/url";
import { timeline as entriesOf, fromSession, newestRounds, stageReviews, stages } from "../lib/sessions";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { ItemChip, ItemOpener, itemPath } from "./ItemChip";
import { ItemStatusIcon, STATUS_LABEL } from "./ItemStatusIcon";
import { AgentIcon } from "./AgentIcon";
import { ArchiveButton, useToggleArchive } from "./ArchiveButton";
import { outcomeOf } from "./Badges";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { SessionItemPanel } from "./SessionItemPanel";
import { SidePanel } from "./SidePanel";
import { Tooltip } from "./Tooltip";
import {
  CommentMessage,
  FinishedLine,
  ItemEventLine,
  MessageBubble,
  NoteMessage,
  ReviewCard,
  RevisedLine,
  StarMark,
  StartedLine,
  StatusGroup,
  WithdrawnLine,
} from "./thread/messages";
import { QuestionSet } from "./thread/QuestionSet";
import { IdleComposer, MessageComposer } from "./thread/composers";

/** Whether the box under a thread sends the agent messages. Off until
 *  Pinrail can reach an agent that waits for nothing: until then a message
 *  waits for the agent's next status, which may never come. */
const MESSAGES = false;

/** Whether the session's details show beside the thread, and their width,
 *  stored on this machine. */
const DETAILS_SHOWN = "pinrail.thread.details";
/** Whether a side panel shows the session's details, which it hides at first. */
const DETAILS_SHOWN_PANEL = "pinrail.thread.panel-details";
const DETAILS_WIDTH = "pinrail.thread.detailsWidth";
/** Whether the session's details list the reviews that ended, stored on
 *  this machine; they are folded away until opened. */
const HANDLED_SHOWN = "pinrail.thread.handled";
const DETAILS_MIN = 220;
const DETAILS_MAX = 520;

function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // without storage, the choice lasts until the app restarts
  }
}

const clampDetails = (w: number) => Math.round(Math.max(DETAILS_MIN, Math.min(w, DETAILS_MAX)));

type ThreadProps = {
  /** the questions opened, which the thread shows within their session */
  focus?: Review;
  /** the session to show, when it is not the focus's */
  session?: { agent: string; id: string };
  inPanel?: boolean;
  onClose?: () => void;
  /** called with the session each time it is loaded */
  onSession?: (session: Session) => void;
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
/** At most this many of a stage's reviews in its tooltip. */
const STAGE_REVIEWS = 5;
/** How many of the latest stages the details show before the rest fold. */
const STAGES_SHOWN = 8;

export const sessionPath = (s: { agent: string; id: string }) =>
  `/sessions/${encodeURIComponent(s.agent)}/${encodeURIComponent(s.id)}`;

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
  stage,
  reviewPanel,
}: ThreadProps) {
  const live = useLive();
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
  const openFromLink = (event: React.MouseEvent, id: string) => {
    if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    showReview(id);
  };
  const body = useRef<HTMLDivElement>(null);
  const named = given ?? (focus ? sessionOf(focus) : null);
  // the work items the session holds or worked on, with what changes them
  const [sessionItems, setSessionItems] = useState<WorkItem[]>([]);
  useEffect(() => {
    if (!named) return;
    let cancelled = false;
    api
      .items({ agent: named.agent, session: named.id, status: "open,in_progress,done,cancelled" })
      .then((d) => !cancelled && setSessionItems(d.items))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [named?.agent, named?.id, live.lastNotice]); // eslint-disable-line react-hooks/exhaustive-deps -- the session's names, not the object
  // what the session holds now, in its header; every item it worked on, in
  // its details, those in progress first and the closed ones last
  const held = sessionItems.filter(
    (i) => i.status === "in_progress" && i.claim?.agent === named?.agent && i.claim?.session === named?.id,
  );
  const STANDING = { in_progress: 0, open: 1, on_hold: 2, done: 3, cancelled: 4 } as const;
  const workedOn = [...sessionItems].sort(
    (a, b) => STANDING[a.status] - STANDING[b.status] || a.repo.localeCompare(b.repo) || a.number - b.number,
  );
  const navigate = useNavigate();
  // a side panel has details only for a question asked in a session
  const hasDetails = !inPanel || !!named;
  const [session, setSession] = useState<Session | null>(null);
  const [items, setItems] = useState<Review[]>(focus ? [focus] : []);
  const [statuses, setStatuses] = useState<SessionStatus[]>([]);
  const [messages, setMessages] = useState<SessionMessage[]>([]);
  const [comments, setComments] = useState<SessionComment[]>([]);
  const [itemEvents, setItemEvents] = useState<SessionItemEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      if (named) {
        const thread = await api.session(named.agent, named.id);
        setSession(thread.session);
        onSession?.(thread.session);
        setItems(thread.items);
        setStatuses(thread.statuses);
        setMessages(thread.messages ?? []);
        setComments(thread.comments ?? []);
        setItemEvents(thread.item_events ?? []);
      } else if (focus) {
        setItems([await api.getReview(focus.id)]);
      }
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "The server did not answer.");
    }
  }, [named?.agent, named?.id, focus?.id]); // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the objects
  useEffect(() => {
    load();
  }, [load]);

  // anything about one of its reviews, or a new one from the session
  const ids = useRef(new Set<string>());
  ids.current = new Set(items.map((r) => r.id));
  useEffect(() => {
    const notice = live.lastNotice;
    if (!notice) return;
    const ours =
      (notice.review_id && ids.current.has(notice.review_id)) ||
      (named && notice.review?.requested_by === named.agent && notice.review?.session === named.id) ||
      (named && notice.session?.agent === named.agent && notice.session.id === named.id) ||
      // a comment its agent wrote on a work item, or a change to its items
      (named && notice.kind === "item_changed");
    if (ours) load();
  }, [live.lastNotice]); // eslint-disable-line react-hooks/exhaustive-deps -- each notice once

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

  // once the thread is there, the questions opened are scrolled to, or,
  // for the session alone, the stage asked for or its newest message
  const timeline = useRef<HTMLDivElement>(null);
  const scrolled = useRef(false);
  useEffect(() => {
    if (scrolled.current) return;
    if (focus) {
      if (items.length < 2) return;
      scrolled.current = true;
      document.getElementById(`set-${focus.id}`)?.scrollIntoView({ block: "start" });
    } else if (session && timeline.current) {
      scrolled.current = true;
      const started = stage ? document.getElementById(`status-${stage}`) : null;
      if (started) started.scrollIntoView({ block: "center" });
      else timeline.current.scrollTop = timeline.current.scrollHeight;
    }
  }, [items, focus, session, stage]);

  // the timeline keeps room under its last message for the chat box over it
  useEffect(() => {
    const el = timeline.current;
    if (!footer || !el) return;
    const size = () => el.style.setProperty("--footer-height", `${footer.offsetHeight}px`);
    size();
    const observer = new ResizeObserver(size);
    observer.observe(footer);
    return () => observer.disconnect();
  }, [footer]);

  // whether the timeline shows its newest message, else a button goes there
  const [atEnd, setAtEnd] = useState(true);
  const checkEnd = useCallback(() => {
    const el = timeline.current;
    if (el) setAtEnd(el.scrollHeight - el.clientHeight - el.scrollTop < 40);
  }, []);
  useEffect(() => {
    const el = timeline.current;
    if (!el) return;
    el.addEventListener("scroll", checkEnd, { passive: true });
    const observer = new ResizeObserver(checkEnd);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", checkEnd);
      observer.disconnect();
    };
  }, [checkEnd]);
  useEffect(checkEnd, [items, checkEnd]);

  // ⌘F finds text in the thread, unless the focus is in the panel beside
  // it, where a review's view has its own find
  const [finding, setFinding] = useState(false);
  const [findFocus, setFindFocus] = useState(0);
  const [findQuery, setFindQuery] = useState("");
  const [found, setFound] = useState<Found | null>(null);
  const finderRef = useRef<Finder | null>(null);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!hasMod(event) || event.shiftKey || event.altKey || (event.key !== "f" && event.key !== "F")) return;
      // the thread may itself be in the Sessions page's panel
      const panel = document.activeElement?.closest(".review-panel");
      if (modalOpen() || (panel && !panel.contains(timeline.current))) return;
      event.preventDefault();
      setFinding(true);
      setFindFocus((n) => n + 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
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
    setFinding(false);
    setFound(null);
  };

  // When the timeline's width changes, as the side panel opens, closes or
  // is resized, its text wraps again and its content moves. What was last
  // clicked in it is scrolled back to where it was on screen.
  useEffect(() => {
    const el = timeline.current;
    if (!el) return;
    let width = el.clientWidth;
    let anchor: { target: Element; top: number } | null = null;
    const offset = (target: Element) => target.getBoundingClientRect().top - el.getBoundingClientRect().top;
    const onDown = (event: PointerEvent) => {
      anchor = event.target instanceof Element ? { target: event.target, top: offset(event.target) } : null;
    };
    // a scroll by the person moves the anchor with it; one caused by the
    // new width, before the observer runs, does not
    const onScroll = () => {
      if (anchor?.target.isConnected && el.clientWidth === width) anchor.top = offset(anchor.target);
    };
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === width) return;
      width = el.clientWidth;
      if (anchor?.target.isConnected) el.scrollTop += offset(anchor.target) - anchor.top;
    });
    el.addEventListener("pointerdown", onDown);
    el.addEventListener("scroll", onScroll, { passive: true });
    observer.observe(el);
    return () => {
      el.removeEventListener("pointerdown", onDown);
      el.removeEventListener("scroll", onScroll);
      observer.disconnect();
    };
  }, []);

  const agent = named?.agent ?? focus?.requested_by ?? null;
  // a review opened from here names the session in its breadcrumb
  const from = named ? fromSession({ ...named, title: session?.title }) : undefined;
  // each review once, by its newest round
  const newest = useMemo(() => newestRounds(items), [items]);
  const waiting = newest.filter((r) => r.status === "pending");
  const started = useMemo(() => stages(statuses), [statuses]);
  // the latest stages, and the earlier ones once unfolded; a stage the
  // thread was opened at is never folded away
  const [allStages, setAllStages] = useState(false);
  const earlier = Math.max(0, started.length - STAGES_SHOWN);
  const atEarlier = !!stage && started.slice(0, earlier).some((s) => s.start.id === stage);
  const stagesShown = allStages || atEarlier ? started : started.slice(earlier);
  // the reviews each stage sent, for its tooltip
  const entries = useMemo(
    () => entriesOf(items, statuses, messages, comments, itemEvents),
    [items, statuses, messages, comments, itemEvents],
  );
  const sentBy = useMemo(() => stageReviews(entries), [entries]);
  // the person's messages the agent has not received yet
  const queued = messages.filter((m) => !m.delivered_at);
  // the reviews the session sent that have ended, newest first
  const handled = newest.filter((r) => r.kind === "review" && r.status !== "pending").reverse();
  // the starred reviews, newest first
  const starred = newest.filter((r) => r.kind === "review" && r.starred).reverse();
  const [showHandled, setShowHandled] = useState(() => stored(HANDLED_SHOWN) === "shown");
  const toggleHandled = () =>
    setShowHandled((shown) => {
      store(HANDLED_SHOWN, shown ? "hidden" : "shown");
      return !shown;
    });

  // the session's details beside the thread: shown or hidden with the
  // button or ⌘⇧B, and as wide as their edge is dragged; a side panel keeps
  // its own choice, and hides them at first
  const shownKey = inPanel ? DETAILS_SHOWN_PANEL : DETAILS_SHOWN;
  const [details, setDetails] = useState(() =>
    inPanel ? stored(DETAILS_SHOWN_PANEL) === "shown" : stored(DETAILS_SHOWN) !== "hidden",
  );
  const [detailsWidth, setDetailsWidth] = useState(() => {
    const n = Number(stored(DETAILS_WIDTH));
    return n >= DETAILS_MIN ? clampDetails(n) : 280;
  });
  const toggleDetails = useCallback(() => {
    setDetails((shown) => {
      store(shownKey, shown ? "hidden" : "shown");
      return !shown;
    });
  }, [shownKey]);
  useEffect(() => {
    if (!hasDetails) return;
    const onKey = (event: KeyboardEvent) => {
      if (modalOpen() || !hasMod(event) || !event.shiftKey || event.altKey) return;
      if (event.key !== "b" && event.key !== "B") return;
      event.preventDefault();
      toggleDetails();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hasDetails, toggleDetails]);
  // E archives the session open on its page or in the side panel, or brings
  // it back, when the focus is not in a text field
  const toggleArchive = useToggleArchive();
  const archivable = !focus && named && session ? { ...named, archived_at: session.archived_at } : null;
  const archivableRef = useRef(archivable);
  archivableRef.current = archivable;
  useEffect(() => {
    if (focus) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "e" && event.key !== "E") return;
      if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (!archivableRef.current) return;
      event.preventDefault();
      toggleArchive(archivableRef.current);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [focus, toggleArchive]);
  const keepDetailsWidth = (w: number) => {
    setDetailsWidth(w);
    store(DETAILS_WIDTH, String(w));
  };
  const resizeDetails = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const right = event.currentTarget.parentElement?.getBoundingClientRect().right ?? window.innerWidth;
    document.body.classList.add("is-resizing");
    const onMove = (e: PointerEvent) => setDetailsWidth(clampDetails(right - e.clientX));
    const onUp = (e: PointerEvent) => {
      document.body.classList.remove("is-resizing");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      keepDetailsWidth(clampDetails(right - e.clientX));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const resizeDetailsBy = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 80 : 20;
    if (event.key === "ArrowLeft") keepDetailsWidth(clampDetails(detailsWidth + step));
    else if (event.key === "ArrowRight") keepDetailsWidth(clampDetails(detailsWidth - step));
    else return;
    event.preventDefault();
  };
  const questions = items.filter((r) => r.kind === "question").length;
  const reviews = newest.length - questions;
  const repo = session?.repo ?? focus?.origin.repo ?? null;
  usePageRepo(repo);
  const ref = session?.ref ?? focus?.origin.ref ?? null;

  return (
    <ItemOpener.Provider value={panelMode ? showItem : null}>
      <div className={`thread ${inPanel ? "in-panel" : ""}`} data-testid="thread">
        <header className="thread-head">
          <span className="thread-avatar">
            <AgentIcon requestedBy={agent} size={18} />
          </span>
          <div className="thread-title">
            {/* the stage the agent works on names the session, with the agent
              and where it works under it; without one, those are the title */}
            <div className="thread-title-row">
              <h1 className={session?.title ? "thread-stage" : undefined}>
                {session?.title ?? <Where agent={agent} repo={repo} reference={ref} />}
              </h1>
              {waiting.length > 0 ? (
                <span className="thread-waiting">
                  <span className="thread-waiting-dot" aria-hidden />
                  Waiting for you · {waiting.length}
                </span>
              ) : session?.status?.kind === "done" ? (
                <span className="thread-done" data-testid="head-finished">
                  <CheckCircle2 size={12} aria-hidden /> Finished
                </span>
              ) : null}
              {session?.archived_at ? <span className="archived-chip">Archived</span> : null}
            </div>
            <div className="thread-meta">
              {session?.title ? (
                <span className="thread-meta-where">
                  <Where agent={agent} repo={repo} reference={ref} />
                </span>
              ) : null}
              {session ? <span title={stamp(session.first_at)}>started {age(session.first_at)} ago</span> : null}
              {named ? (
                // from a question or a side panel, the session's own page; on
                // it, the id alone
                focus || inPanel ? (
                  <Link to={sessionPath(named)} className="thread-session" title={`Open session ${named.id}`}>
                    session {named.id}
                  </Link>
                ) : (
                  <span className="thread-session" title={named.id}>
                    session {named.id}
                  </span>
                )
              ) : null}
              {named ? <CopySessionId id={named.id} /> : null}
              {held.map((item) => (
                <span key={item.key} className="thread-meta-item" data-testid="thread-held-item">
                  working on <ItemChip item={item} />
                </span>
              ))}
            </div>
          </div>
          <span className="thread-head-end">
            {items.length > 0 ? (
              <span className="thread-counts">
                {[
                  reviews ? `${reviews} ${reviews === 1 ? "review" : "reviews"}` : null,
                  questions ? `${questions} ${questions === 1 ? "question" : "questions"}` : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            ) : null}
            {named && !focus ? <ArchiveButton session={{ ...named, archived_at: session?.archived_at }} /> : null}
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
        <div className="thread-body" ref={body}>
          <div className="thread-main">
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
              {session ? (
                <div className="thread-divider">
                  <span>Session started {stamp(session.first_at)}</span>
                </div>
              ) : null}
              {entries.map((entry) => {
                if (entry.kind === "message")
                  return (
                    <MessageBubble
                      key={`m-${entry.message.id}`}
                      message={entry.message}
                      session={named!}
                      onDeleted={load}
                    />
                  );
                if (entry.kind === "statuses")
                  return <StatusGroup key={`s-${entry.statuses[0].id}`} statuses={entry.statuses} />;
                if (entry.kind === "started") return <StartedLine key={`s-${entry.status.id}`} status={entry.status} />;
                if (entry.kind === "finished")
                  return <FinishedLine key={`s-${entry.status.id}`} status={entry.status} />;
                if (entry.kind === "itemEvents")
                  return <ItemEventLine key={`e-${entry.events[0].id}`} events={entry.events} />;
                if (entry.kind === "comment")
                  return <CommentMessage key={`c-${entry.comment.id}`} comment={entry.comment} />;
                if (entry.kind === "note")
                  return (
                    <NoteMessage
                      key={`s-${entry.status.id}`}
                      status={entry.status}
                      agent={agent}
                      session={named?.id ?? null}
                    />
                  );
                if (entry.kind === "revised")
                  return (
                    <RevisedLine
                      key={`v-${entry.review.id}`}
                      entry={entry}
                      onOpen={panelMode ? () => showReview(entry.card.id) : undefined}
                    />
                  );
                if (entry.kind === "withdrawn")
                  return (
                    <WithdrawnLine
                      key={`w-${entry.review.id}`}
                      entry={entry}
                      onOpen={panelMode ? () => showReview(entry.card.id) : undefined}
                    />
                  );
                const item = entry.review;
                return item.kind === "question" ? (
                  <QuestionSet
                    key={item.id}
                    review={item}
                    active={item.id === active}
                    footer={item.id === active ? footer : null}
                    onActivate={() => setPicked(item.id)}
                    onDone={load}
                  />
                ) : (
                  <ReviewCard
                    key={item.id}
                    review={item}
                    round={entry.round}
                    to={reviewTo(item.id)}
                    state={panelMode ? undefined : from}
                    open={item.id === openReview}
                    onClick={(e) => openFromLink(e, item.id)}
                  />
                );
              })}
            </div>
            <div className="thread-footer" ref={setFooter}>
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
              {active ? null : MESSAGES && named ? <MessageComposer session={named} onSent={load} /> : <IdleComposer />}
            </div>
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
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize the session's details"
              aria-valuenow={detailsWidth}
              aria-valuemin={DETAILS_MIN}
              aria-valuemax={DETAILS_MAX}
              tabIndex={0}
              onPointerDown={resizeDetails}
              onKeyDown={resizeDetailsBy}
            />
          )}
          {!hasDetails || !details || openReview || openItem ? null : (
            <aside className="thread-aside" aria-label="Session details" style={{ width: detailsWidth }}>
              <section>
                <h2>Waiting on you{waiting.length > 0 ? ` · ${waiting.length}` : ""}</h2>
                {waiting.length === 0 ? <p className="faint">Nothing is waiting.</p> : null}
                {waiting.map((r) => (
                  <a
                    key={r.id}
                    href={r.kind === "question" ? `#set-${r.id}` : `#/reviews/${r.id}`}
                    className="thread-waiting-item"
                    data-testid="waiting-item"
                    onClick={(e) => {
                      e.preventDefault();
                      if (r.kind !== "question" && panelMode) showReview(r.id);
                      else if (r.kind !== "question") navigate(`/reviews/${r.id}`, { state: from });
                      else
                        document.getElementById(`set-${r.id}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
                    }}
                  >
                    <ReviewIcon review={r} size={14} />
                    <span className="thread-waiting-item-text">
                      <span className="text">{r.title}</span>
                      <span className="faint" title={stamp(r.created_at)}>
                        {r.kind === "question" ? "questions" : reviewLabel(r)} · {age(r.created_at)} ago
                      </span>
                    </span>
                    <StarMark review={r} />
                  </a>
                ))}
              </section>
              {queued.length > 0 ? (
                <section aria-label="Waiting on the agent">
                  <h2>Waiting on the agent · {queued.length}</h2>
                  {queued.map((m) => (
                    <a
                      key={m.id}
                      href={`#message-${m.id}`}
                      className="thread-waiting-item"
                      data-testid="queued-message"
                      onClick={(e) => {
                        e.preventDefault();
                        document
                          .getElementById(`message-${m.id}`)
                          ?.scrollIntoView({ block: "center", behavior: "smooth" });
                      }}
                    >
                      <Clock size={14} className="thread-queued-icon" aria-hidden />
                      <span className="thread-waiting-item-text">
                        <span className="text">{m.text}</span>
                        <span className="faint" title={stamp(m.sent_at)}>
                          queued · {age(m.sent_at)} ago
                        </span>
                      </span>
                    </a>
                  ))}
                </section>
              ) : null}
              {workedOn.length > 0 ? (
                <section aria-label="Work items" data-testid="thread-work-items">
                  <h2>Work items · {workedOn.length}</h2>
                  {workedOn.map((item) => (
                    <Link
                      key={item.key}
                      to={itemPath(item.key)}
                      onClick={(e) => onItemLink(e, item.key)}
                      className={`thread-waiting-item ${item.status === "done" || item.status === "cancelled" ? "is-ended" : ""}`}
                      data-testid="thread-work-item"
                    >
                      <ItemStatusIcon status={item.status} />
                      <span className="thread-waiting-item-text">
                        <span className="text">{item.title}</span>
                        <span className="faint">
                          <span className="mono">{item.key}</span> · {STATUS_LABEL[item.status].toLowerCase()}
                        </span>
                      </span>
                    </Link>
                  ))}
                </section>
              ) : null}
              {starred.length > 0 ? (
                <section aria-label="Starred">
                  <h2>Starred · {starred.length}</h2>
                  {starred.map((r) => {
                    const outcome = outcomeOf(r);
                    return (
                      <Link
                        key={r.id}
                        to={reviewTo(r.id)}
                        state={panelMode ? undefined : from}
                        onClick={(e) => openFromLink(e, r.id)}
                        className={`thread-waiting-item ${r.status === "pending" ? "" : `is-ended tone-${outcome.tone}`}`}
                        data-testid="starred-review"
                      >
                        <ReviewIcon review={r} size={14} />
                        <span className="thread-waiting-item-text">
                          <span className="text">{r.title}</span>
                          <span className="faint" title={stamp(r.created_at)}>
                            {r.status === "pending" ? (
                              "waiting"
                            ) : (
                              <span className="thread-outcome">{outcome.label}</span>
                            )}{" "}
                            · {r.plugin} · {age(r.created_at)} ago
                          </span>
                        </span>
                        <StarMark review={r} />
                      </Link>
                    );
                  })}
                </section>
              ) : null}
              {started.length > 0 ? (
                <section aria-label="Stages">
                  <h2>Stages</h2>
                  {earlier > 0 && !atEarlier ? (
                    <button
                      type="button"
                      className="thread-status-more thread-rail-more"
                      aria-expanded={allStages}
                      onClick={() => setAllStages((all) => !all)}
                    >
                      {allStages ? "Show only the latest" : `${earlier} earlier ${earlier === 1 ? "stage" : "stages"}`}
                    </button>
                  ) : null}
                  <ol className="thread-rail">
                    {stagesShown.map((stage) => {
                      const i = started.indexOf(stage);
                      const last = i === started.length - 1;
                      const state = stage.done ? "is-done" : last ? "is-last" : "is-left";
                      // until it finished, until the next stage began for one left
                      // open, or until now for the one in progress
                      const end = stage.done?.at ?? (last ? null : started[i + 1].start.at);
                      const took = age(stage.start.at, end ? Date.parse(end) : Date.now());
                      const length = stage.current
                        ? `In progress for ${took}`
                        : stage.done
                          ? `Finished in ${took}`
                          : `Took ${took}`;
                      const about = [stage.start.text, length, `Started ${stamp(stage.start.at)}`].join(" · ");
                      return (
                        <li key={stage.start.id} className={`thread-rail-stop ${state}`}>
                          <Tooltip
                            side="top"
                            label={
                              <span className="stage-tip">
                                <strong>{stage.start.text}</strong>
                                <span>{length}</span>
                                <span className="faint">Started {stamp(stage.start.at)}</span>
                                {(sentBy.get(stage.start.id) ?? []).length > 0 ? (
                                  <span className="stage-tip-reviews">
                                    {sentBy
                                      .get(stage.start.id)!
                                      .slice(0, STAGE_REVIEWS)
                                      .map((r) => (
                                        <span key={r.id} className="stage-tip-review">
                                          <ReviewIcon review={r} size={12} />
                                          <span className="stage-tip-title">{r.title}</span>
                                          <span className="faint">
                                            {r.status === "pending" ? "waiting" : outcomeOf(r).label}
                                          </span>
                                        </span>
                                      ))}
                                    {sentBy.get(stage.start.id)!.length > STAGE_REVIEWS ? (
                                      <span className="faint">
                                        {sentBy.get(stage.start.id)!.length - STAGE_REVIEWS} more
                                      </span>
                                    ) : null}
                                  </span>
                                ) : null}
                              </span>
                            }
                          >
                            <a
                              href={`#status-${stage.start.id}`}
                              className="thread-rail-link"
                              data-testid="stage"
                              aria-label={about}
                              onClick={(e) => {
                                e.preventDefault();
                                document
                                  .getElementById(`status-${stage.start.id}`)
                                  ?.scrollIntoView({ block: "center", behavior: "smooth" });
                              }}
                            >
                              <span className="thread-rail-dot" aria-hidden>
                                {stage.done ? <Check size={9} strokeWidth={3} /> : null}
                              </span>
                              <span className="thread-rail-text">{stage.start.text}</span>
                              <span className="faint thread-rail-when">{took}</span>
                            </a>
                          </Tooltip>
                        </li>
                      );
                    })}
                  </ol>
                </section>
              ) : null}
              {handled.length > 0 ? (
                <section aria-label="Reviews">
                  <h2>
                    <button
                      type="button"
                      className="thread-aside-fold"
                      aria-expanded={showHandled}
                      onClick={toggleHandled}
                    >
                      <ChevronRight size={12} className="thread-aside-fold-icon" aria-hidden />
                      Reviews · {handled.length}
                    </button>
                  </h2>
                  {showHandled
                    ? handled.map((r) => {
                        // the icon and the outcome in the colour of the outcome's badge
                        const outcome = outcomeOf(r);
                        return (
                          <Link
                            key={r.id}
                            to={reviewTo(r.id)}
                            state={panelMode ? undefined : from}
                            onClick={(e) => openFromLink(e, r.id)}
                            className={`thread-waiting-item is-ended tone-${outcome.tone}`}
                            data-testid="handled-review"
                          >
                            <ReviewIcon review={r} size={14} />
                            <span className="thread-waiting-item-text">
                              <span className="text">{r.title}</span>
                              <span className="faint" title={stamp(r.created_at)}>
                                <span className="thread-outcome">{outcome.label}</span> · {r.plugin} ·{" "}
                                {age(r.created_at)} ago
                              </span>
                            </span>
                            <StarMark review={r} />
                          </Link>
                        );
                      })
                    : null}
                </section>
              ) : null}
              {session ? (
                <section>
                  <h2>Session</h2>
                  <dl className="thread-facts">
                    <dt>Agent</dt>
                    <dd>{session.agent}</dd>
                    <dt>Id</dt>
                    <dd className="mono">{session.id}</dd>
                    {session.repo ? (
                      <>
                        <dt>Project</dt>
                        <dd>{session.repo}</dd>
                      </>
                    ) : null}
                    {session.ref ? (
                      <>
                        <dt>Branch</dt>
                        <dd className="mono">{session.ref}</dd>
                      </>
                    ) : null}
                    {session.folder ? (
                      <>
                        <dt>Folder</dt>
                        <dd className="mono" title={session.folder}>
                          {session.folder}
                        </dd>
                      </>
                    ) : null}
                    <dt>Last activity</dt>
                    <dd title={stamp(session.last_at)}>{age(session.last_at)} ago</dd>
                  </dl>
                </section>
              ) : null}
            </aside>
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

/** Copies the session's id, for a message to the agent or a command line. */
function CopySessionId({ id }: { id: string }) {
  const [copied, setCopied] = useState<"done" | "failed" | null>(null);
  const copy = async () => {
    try {
      await copyText(id);
      setCopied("done");
    } catch {
      setCopied("failed");
    }
    window.setTimeout(() => setCopied(null), 3500);
  };
  return (
    <Tooltip
      label={copied === "done" ? "Copied" : copied === "failed" ? "Could not copy" : "Copy the id"}
      side="bottom"
    >
      <button
        type="button"
        className={`id-copy ${copied === "done" ? "ok" : copied === "failed" ? "danger" : ""}`}
        onClick={copy}
        aria-label="Copy the session id"
      >
        {copied === "done" ? (
          <ClipboardCheck size={14} />
        ) : copied === "failed" ? (
          <ClipboardX size={14} />
        ) : (
          <Copy size={14} />
        )}
      </button>
    </Tooltip>
  );
}

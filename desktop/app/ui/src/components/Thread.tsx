// An agent session's thread: the questions it asked, as the agent's
// messages, and the reviews it sent, oldest first. Questions are answered
// here, each with its chips or a reply box, and sent as a set.

import {
  Activity,
  ArrowDown,
  ArrowUp,
  Ban,
  Check,
  CheckCircle2,
  Clock,
  ChevronRight,
  CornerDownRight,
  Expand,
  GitBranch,
  GitCommitHorizontal,
  MessageSquarePlus,
  PanelRightClose,
  Paperclip,
  PanelRightOpen,
  Pencil,
  RefreshCw,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Link, useNavigate, type To } from "react-router";
import { ApiError, api } from "../api/client";
import type { Answer, Question, Review, Session, SessionMessage, SessionStatus } from "../api/types";
import { age, stamp } from "../lib/format";
import { MOD, SHIFT, hasMod, modalOpen } from "../lib/keys";
import { plainInline, renderInline, renderMarkdown, renderNote } from "../lib/markdown";
import { openExternal } from "../lib/native";
import { answersOf, decisionOf, pick, unanswered, type Draft } from "../lib/questions";
import { useUrlParams } from "../lib/url";
import {
  timeline as entriesOf,
  fromSession,
  newestRounds,
  stageReviews,
  stages,
  type Entry,
  type FromSession,
  type Round,
} from "../lib/sessions";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";
import { AgentIcon } from "./AgentIcon";
import { ArchiveButton, useToggleArchive } from "./ArchiveButton";
import { ComposeField } from "./ComposeField";
import { OutcomeBadge, SummaryCounts, outcomeOf } from "./Badges";
import { DiscardDialog } from "./DiscardDialog";
import { NoteFiles, filesOf, useNoteFileUrl, useSaveNoteFile } from "./NoteFiles";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
import { SidePanel } from "./SidePanel";
import { Tooltip } from "./Tooltip";

const DRAFTS = "pinrail:answers:";
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
  /** Shows a review beside the thread, or closes the panel with null. A
   *  review shown in place of another replaces its history entry. */
  const showReview = useCallback(
    (id: string | null, options?: { replace?: boolean }) =>
      updateParams(
        (next) => {
          if (id) next.set("review", id);
          else next.delete("review");
        },
        { replace: options?.replace ?? (!!id && currentParams().has("review")) },
      ),
    [updateParams, currentParams],
  );
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
  const navigate = useNavigate();
  // a side panel has details only for a question asked in a session
  const hasDetails = !inPanel || !!named;
  const [session, setSession] = useState<Session | null>(null);
  const [items, setItems] = useState<Review[]>(focus ? [focus] : []);
  const [statuses, setStatuses] = useState<SessionStatus[]>([]);
  const [messages, setMessages] = useState<SessionMessage[]>([]);
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
      (named && notice.session?.agent === named.agent && notice.session.id === named.id);
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
  const entries = useMemo(() => entriesOf(items, statuses, messages), [items, statuses, messages]);
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
  const ref = session?.ref ?? focus?.origin.ref ?? null;

  return (
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
          <div className="thread-items" ref={timeline}>
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
              if (entry.kind === "finished") return <FinishedLine key={`s-${entry.status.id}`} status={entry.status} />;
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
            {active ? null : named ? <MessageComposer session={named} onSent={load} /> : <IdleComposer />}
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
        {!hasDetails || !details || openReview ? null : (
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
        {!hasDetails || !details || openReview ? null : (
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
                    else document.getElementById(`set-${r.id}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
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
                          {r.status === "pending" ? "waiting" : <span className="thread-outcome">{outcome.label}</span>}{" "}
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
                              <span className="thread-outcome">{outcome.label}</span> · {r.plugin} · {age(r.created_at)}{" "}
                              ago
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

/** How a status reads on its own: a step as it is, the start of a stage, which
 *  the title names, as Started, and the end as Finished with what it says. */
export function statusLine(status: SessionStatus): string {
  if (status.kind === "start") return "Started";
  if (status.kind === "done") return status.text ? `Finished: ${status.text}` : "Finished";
  return status.text;
}

/** Statuses the agent reported in a row: the newest, and the earlier ones
 *  behind a button. */
function StatusGroup({ statuses }: { statuses: SessionStatus[] }) {
  const [open, setOpen] = useState(false);
  const newest = statuses[statuses.length - 1];
  const earlier = statuses.length - 1;
  const shown = open ? statuses : [newest];
  return (
    <div className="thread-statuses" data-testid="status-group">
      {earlier > 0 ? (
        <button type="button" className="thread-status-more" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          {open ? "Show only the newest" : `${earlier} earlier`}
        </button>
      ) : null}
      {shown.map((status) => (
        <div key={status.id} className="thread-status-line" data-testid="status">
          <Activity size={13} className="thread-status-icon" aria-hidden />
          <span className="thread-status-text">{status.text}</span>
          <span className="faint" title={stamp(status.at)}>
            {age(status.at)} ago
          </span>
        </div>
      ))}
    </div>
  );
}

/** A starred review's mark at the end of its row in the details. */
function StarMark({ review }: { review: Review }) {
  if (!review.starred) return null;
  return <Star size={12} className="thread-item-star" fill="currentColor" aria-label="Starred" role="img" />;
}

/** The agent saying it starts a stage, which it names. */
function StartedLine({ status }: { status: SessionStatus }) {
  return (
    <div className="thread-divider thread-started" id={`status-${status.id}`} data-testid="started">
      <span>
        <GitCommitHorizontal size={12} aria-hidden /> Started: {status.text} ·{" "}
        <span title={stamp(status.at)}>{age(status.at)} ago</span>
      </span>
    </div>
  );
}

/** How tall a note is before it folds, about 12 lines. */
const NOTE_FOLDED = 230;

/** A note the agent sent: a message under the agent's icon, as a question
 *  is, with its Markdown folded when long and its files under it. Its links
 *  open in the browser, and a link to one of its files saves it. */
function NoteMessage({
  status,
  agent,
  session,
}: {
  status: SessionStatus;
  agent: string | null;
  session: string | null;
}) {
  const files = filesOf(status);
  const note = useMemo(
    () => (agent && session ? { agent, session, status: status.id } : null),
    [agent, session, status.id],
  );
  const url = useNoteFileUrl(note);
  const save = useSaveNoteFile(note, url);
  // only an image the note carries shows in place
  const imageUrl = useCallback(
    (name: string) => (files.some((f) => f.name === name && f.media_type.startsWith("image/")) ? url(name) : null),
    [files, url],
  );
  const html = useMemo(() => renderNote(status.text, imageUrl), [status.text, imageUrl]);
  const body = useRef<HTMLDivElement>(null);
  const [long, setLong] = useState(false);
  const [open, setOpen] = useState(false);
  // measured again as its images load
  const measure = useCallback(() => setLong((body.current?.scrollHeight ?? 0) > NOTE_FOLDED + 40), []);
  useLayoutEffect(measure, [html, measure]);
  return (
    <div className="thread-note msg" data-testid="note">
      <span className="msg-avatar">
        <AgentIcon requestedBy={agent} size={14} />
      </span>
      <div className="msg-col">
        <div className="msg-meta">
          <span className="text">{agent ?? "The agent"}</span> wrote ·{" "}
          <span title={stamp(status.at)}>{age(status.at)} ago</span>
          {files.length ? (
            <span className="note-file-count">
              <Paperclip size={12} aria-hidden /> {files.length} file{files.length === 1 ? "" : "s"}
            </span>
          ) : null}
        </div>
        <div
          ref={body}
          className={`thread-note-body ${long && !open ? "is-folded" : ""}`}
          style={long && !open ? { maxHeight: NOTE_FOLDED } : undefined}
          onLoadCapture={measure}
          onClick={(e) => {
            const link = (e.target as HTMLElement).closest("a");
            if (!link) return;
            e.preventDefault();
            const file = link.dataset.attachment;
            if (file !== undefined) void save(file);
            else openExternal(link.href);
          }}
          dangerouslySetInnerHTML={{ __html: html }}
        />
        {long ? (
          <button type="button" className="thread-note-more" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
            {open ? "Show less" : "Show more"}
          </button>
        ) : null}
        <NoteFiles files={files} url={url} onSave={(name) => void save(name)} />
      </div>
    </div>
  );
}

/** The agent saying it has finished, with what it finished with. */
function FinishedLine({ status }: { status: SessionStatus }) {
  return (
    <div className="thread-divider thread-finished" data-testid="finished">
      <span>
        <CheckCircle2 size={13} aria-hidden /> Finished{status.text ? `: ${status.text}` : ""} ·{" "}
        <span title={stamp(status.at)}>{age(status.at)} ago</span>
      </span>
    </div>
  );
}

type CardProps = {
  review: Review;
  round?: Round;
  to: To;
  state?: FromSession;
  /** shown in the panel beside the thread */
  open: boolean;
  onClick: (event: React.MouseEvent) => void;
};

/** A review the session sent, by its newest round, which opens it. */
function ReviewCard({ review, round, to, state, open, onClick }: CardProps) {
  return (
    <Link
      to={to}
      state={state}
      onClick={onClick}
      id={`review-${review.id}`}
      className={`thread-card ${open ? "is-open" : ""}`}
      aria-current={open ? "true" : undefined}
      data-testid="thread-review"
    >
      <ReviewIcon review={review} size={16} />
      <span className="thread-card-text">
        <span className="thread-card-title">{review.title}</span>
        <span className="thread-card-meta faint" title={stamp(review.created_at)}>
          Review · {review.plugin}
          {round ? ` · round ${round.n} of ${round.of}` : ""} · {age(review.created_at)} ago
        </span>
        {/* under the title, where many counts cannot squeeze it */}
        <SummaryCounts summary={review.status === "pending" ? review.summary : review.decision?.summary} />
      </span>
      <OutcomeBadge review={review} />
    </Link>
  );
}

/** How long a card stays highlighted after a line leads to it. */
const FLASH_MS = 1600;

/** Where the agent sent a later round of a review shown further up: a quiet
 *  line that scrolls to the review's card, and opens the review in the
 *  panel beside the thread when reviews open there. */
function RevisedLine({ entry, onOpen }: { entry: Extract<Entry, { kind: "revised" }>; onOpen?: () => void }) {
  const { review, round, card } = entry;
  const waiting = card.status === "pending" && review === card;
  const show = () => {
    const el = document.getElementById(`review-${card.id}`);
    if (!el) return;
    el.scrollIntoView({ behavior: "smooth", block: "center" });
    el.classList.remove("is-flashed");
    void el.offsetWidth; // restart the highlight when clicked again
    el.classList.add("is-flashed");
    window.setTimeout(() => el.classList.remove("is-flashed"), FLASH_MS);
    onOpen?.();
  };
  return (
    <button type="button" className="thread-revised" data-testid="revised" onClick={show}>
      <RefreshCw size={13} className="thread-revised-icon" aria-hidden />
      <span className="thread-revised-text">
        Revised <span className="thread-revised-title">{review.title}</span>
        {round ? ` · round ${round}` : ""} · <span title={stamp(review.created_at)}>{age(review.created_at)} ago</span>
      </span>
      {waiting ? <span className="thread-revised-waiting">Waiting for you</span> : null}
      <ArrowUp size={12} className="thread-revised-up" aria-hidden />
    </button>
  );
}

function loadDrafts(id: string): Record<string, Draft> {
  try {
    return JSON.parse(sessionStorage.getItem(DRAFTS + id) ?? "{}");
  } catch {
    return {};
  }
}

/** A focus handler that runs `then` when a text box takes the focus. The
 *  questions are made current on a click, not on the press: the keys they
 *  then show widen the chips, and a press that moved the chips would lose
 *  its click. */
const onTyping = (then: () => void) => (e: React.FocusEvent) => {
  if ((e.target as HTMLElement).tagName === "TEXTAREA") then();
};

/** The values a question's chips pick, in their order. */
const optionsOf = (question: Question) =>
  question.answer === "yes-no" ? ["yes", "no"] : question.answer === "text" ? [] : (question.choices ?? []);

/** The field open in a question: a note on the answer, or an answer in the
 *  person's own words. */
type Editing = { index: number; kind: "note" | "own" };

type SetProps = {
  review: Review;
  /** the questions the keys and the chat box answer */
  active: boolean;
  /** where the chat box goes while they are active */
  footer: HTMLElement | null;
  onActivate: () => void;
  onDone: () => void;
};

/** The questions of one `pinrail ask`: the agent's message, with each
 *  question below it, answered together while they wait. */
function QuestionSet({ review, active, footer, onActivate, onDone }: SetProps) {
  const live = useLive();
  const toast = useToast();
  const questions = useMemo(
    () => (review.payload as { questions?: Question[] } | undefined)?.questions ?? [],
    [review.payload],
  );
  const pending = review.status === "pending";
  const [drafts, setDrafts] = useState<Record<string, Draft>>(() => loadDrafts(review.id));
  const [comment, setComment] = useState(() => {
    try {
      return sessionStorage.getItem(DRAFTS + review.id + ":comment") ?? "";
    } catch {
      return "";
    }
  });
  const onComment = useCallback(
    (text: string) => {
      setComment(text);
      try {
        sessionStorage.setItem(DRAFTS + review.id + ":comment", text);
      } catch {
        // no storage
      }
    },
    [review.id],
  );
  const [current, setCurrent] = useState(0);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [declining, setDeclining] = useState(false);
  const answers = useMemo(() => answersOf(review.decision?.data), [review.decision]);

  const change = useCallback(
    (id: string, next: Draft) => {
      setDrafts((all) => {
        const drafted = { ...all, [id]: next };
        try {
          sessionStorage.setItem(DRAFTS + review.id, JSON.stringify(drafted));
        } catch {
          // no storage
        }
        return drafted;
      });
    },
    [review.id],
  );
  const left = unanswered(questions, drafts);

  const send = useCallback(async () => {
    if (busy || !pending) return;
    setBusy(true);
    setProblem(null);
    try {
      await api.decide(review.id, decisionOf(questions, drafts), comment.trim());
      try {
        sessionStorage.removeItem(DRAFTS + review.id);
        sessionStorage.removeItem(DRAFTS + review.id + ":comment");
      } catch {
        // ignore
      }
      toast(questions.length === 1 ? "Answer sent" : "Answers sent");
      onDone();
    } catch (e) {
      setProblem(e instanceof Error ? e.message : "The answers were not sent.");
    } finally {
      setBusy(false);
    }
  }, [busy, pending, review.id, questions, drafts, comment, toast, onDone]);

  // a pick from the keyboard moves on to the next question still
  // unanswered; one with the pointer keeps the question selected
  const picked = useCallback(
    (index: number, value: string, advance: boolean) => {
      const question = questions[index];
      const next = pick(question, drafts[question.id], value);
      change(question.id, next);
      // an unpick stays on the question
      const answered = unanswered([question], { [question.id]: next }) === 0;
      if (!advance || !answered || question.answer === "choices") return;
      const after = questions.findIndex((q, i) => i > index && unanswered([q], { [q.id]: drafts[q.id] ?? {} }) === 1);
      if (after >= 0) setCurrent(after);
    },
    [questions, drafts, change],
  );

  // the note or the answer in their own words being written, in its field
  const [editing, setEditing] = useState<Editing | null>(null);
  const replyBox = useCallback(
    (index: number) =>
      document.getElementById(`set-${review.id}`)?.querySelector<HTMLTextAreaElement>(`[data-reply="${index}"]`) ??
      null,
    [review.id],
  );
  // N and P, and Enter in a text answer: on to another question, into its
  // box if it is answered in words
  const goTo = useCallback(
    (index: number) => {
      if (index < 0 || index >= questions.length) return;
      setCurrent(index);
      const set = document.getElementById(`set-${review.id}`);
      const block = set?.querySelectorAll<HTMLElement>("[data-testid=question]")[index];
      block?.scrollIntoView({ block: "nearest" });
      if (questions[index].answer === "text") replyBox(index)?.focus();
      else (document.activeElement as HTMLElement | null)?.blur();
    },
    [questions, review.id, replyBox],
  );

  useEffect(() => {
    if (!active || !pending) return;
    const onKey = (event: KeyboardEvent) => {
      if (modalOpen()) return;
      if (hasMod(event) && !event.shiftKey && !event.altKey && event.key === "Enter") {
        event.preventDefault();
        send();
        return;
      }
      if (hasMod(event) && !event.shiftKey && !event.altKey && (event.key === "d" || event.key === "D")) {
        event.preventDefault();
        setDeclining(true);
        return;
      }
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const question = questions[current];
      if (!question) return;
      const key = event.key.toLowerCase();
      if (key === "n" || key === "p") {
        event.preventDefault();
        goTo(key === "n" ? current + 1 : current - 1);
        return;
      }
      if (key === "c") {
        event.preventDefault();
        if (question.answer === "text") replyBox(current)?.focus();
        else setEditing({ index: current, kind: "note" });
        return;
      }
      const n = Number(event.key);
      const choice = optionsOf(question)[n - 1];
      if (Number.isInteger(n) && n >= 1 && choice !== undefined) {
        event.preventDefault();
        picked(current, choice, true);
      }
    };
    // the palette's "Discard this review"
    const onDiscard = () => setDeclining(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:discard", onDiscard);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:discard", onDiscard);
    };
  }, [active, pending, current, questions, picked, send, goTo, replyBox]);

  const several = questions.length > 1;
  return (
    <section
      id={`set-${review.id}`}
      className={`question-set msg ${pending ? "is-pending" : ""} ${active ? "is-active" : ""}`}
      aria-label={review.title}
      data-testid="question-set"
      onClick={pending ? onActivate : undefined}
      onFocus={pending ? onTyping(onActivate) : undefined}
    >
      <span className="msg-avatar">
        <AgentIcon requestedBy={review.requested_by} size={14} />
      </span>
      <div className="msg-col">
        <div className="msg-meta">
          <span className="text">{review.requested_by ?? "The agent"}</span> {pending ? "asks" : "asked"} ·{" "}
          <span title={stamp(review.created_at)}>{age(review.created_at)} ago</span>
          {pending ? <span className="msg-waiting">waiting for your answer</span> : null}
        </div>
        {questions.map((question, index) => (
          <QuestionMessage
            key={question.id}
            review={review}
            number={several ? `${index + 1}/${questions.length}` : null}
            question={question}
            draft={drafts[question.id]}
            answer={answers.get(question.id)}
            current={active && pending && index === current}
            onFocus={() => {
              onActivate();
              setCurrent(index);
            }}
            onPick={(value) => picked(index, value, false)}
            onChange={(next) => change(question.id, next)}
            onNext={() => {
              // from the last question, on to the comment on them all
              if (index + 1 < questions.length) goTo(index + 1);
              else document.querySelector<HTMLTextAreaElement>(".thread-composer textarea")?.focus();
            }}
            editing={editing?.index === index ? editing.kind : null}
            onEdit={(kind) => {
              setCurrent(index);
              setEditing(kind ? { index, kind } : null);
            }}
            index={index}
          />
        ))}
        {review.status === "decided" && review.agent_note ? (
          <div className="msg-answer set-comment" data-testid="set-comment">
            <span>{review.decision?.decided_by ?? "You"} commented</span>
            <span className="msg-answer-text">{review.agent_note}</span>
          </div>
        ) : null}
        <SetFooter review={review} />
        {problem ? <p className="notice notice-danger">{problem}</p> : null}
      </div>
      {active && pending && footer
        ? createPortal(
            <Composer
              comment={comment}
              onComment={onComment}
              left={several ? left : 0}
              busy={busy}
              connected={live.connected}
              onSend={send}
              onDecline={() => setDeclining(true)}
            />,
            footer,
          )
        : null}
      {declining && pending ? (
        <DiscardDialog
          review={review}
          onClose={() => setDeclining(false)}
          onDone={() => {
            setDeclining(false);
            toast("Declined. The agent was told to stop.");
            onDone();
          }}
        />
      ) : null}
    </section>
  );
}

/** How questions that were not answered ended. */
function SetFooter({ review }: { review: Review }) {
  switch (review.status) {
    case "discarded":
      return (
        <p className="set-ended dim">
          {review.discarded_by ?? "The person"} declined to answer {age(review.discarded_at)} ago
          {review.discarded_reason ? `: ${review.discarded_reason}` : "."}
        </p>
      );
    case "withdrawn":
      return (
        <p className="set-ended dim">
          {review.withdrawn_by ?? "The agent"} withdrew the questions {age(review.withdrawn_at)} ago
          {review.withdrawn_reason ? `: ${review.withdrawn_reason}` : "."}
        </p>
      );
    case "expired":
      return <p className="set-ended dim">The questions expired at {stamp(review.expires_at)} without an answer.</p>;
    default:
      return null;
  }
}

type MessageProps = {
  review: Review;
  /** "2/3" among several questions */
  number: string | null;
  question: Question;
  draft: Draft | undefined;
  answer: Answer | undefined;
  /** the question the keys and the chat box answer */
  current: boolean;
  onFocus: () => void;
  onPick: (value: string) => void;
  onChange: (next: Draft) => void;
  /** Enter in a text answer: on to the next question */
  onNext: () => void;
  /** which field is open to write in, if any */
  editing: Editing["kind"] | null;
  onEdit: (kind: Editing["kind"] | null) => void;
  index: number;
};

/** One question in its bubble, with its choices while it waits, what the
 *  person has written for it so far, and once sent, the answer. */
function QuestionMessage({
  review,
  number,
  question,
  draft,
  answer,
  current,
  onFocus,
  onPick,
  onChange,
  onNext,
  editing,
  onEdit,
  index,
}: MessageProps) {
  const pending = review.status === "pending";
  // the question as it reads, for the labels of what answers it
  const asked = useMemo(() => plainInline(question.question), [question.question]);
  const options = optionsOf(question);
  const isPicked = (value: string) =>
    !draft?.own &&
    (question.answer === "yes-no"
      ? draft?.yes === (value === "yes")
      : question.answer === "choices"
        ? !!draft?.choices?.includes(value)
        : draft?.choice === value);
  const own = !!draft?.own && !!draft.text?.trim();
  const note = draft?.note?.trim() ?? "";
  // a field closed with nothing in it leaves nothing behind
  const close = () => {
    if (editing === "own" && !draft?.text?.trim()) onChange({ ...draft, text: undefined, own: false });
    if (editing === "note" && !note) onChange({ ...draft, note: undefined });
    onEdit(null);
  };
  return (
    <div
      className={`msg-question-block ${pending ? "is-pending" : ""} ${current ? "is-current" : ""}`}
      onClick={pending ? onFocus : undefined}
      onFocus={pending ? onTyping(onFocus) : undefined}
      data-testid="question"
    >
      <div className="msg-body">
        <div className="msg-question-row">
          <p className="msg-question">
            {number ? <span className="msg-number">{number}</span> : null}
            <span
              className="markdown-inline"
              onClick={(e) => {
                // its links open in the browser, as the context's do
                const link = (e.target as HTMLElement).closest("a");
                if (!link) return;
                e.preventDefault();
                openExternal(link.href);
              }}
              dangerouslySetInnerHTML={{ __html: renderInline(question.question) }}
            />
          </p>
          {pending && question.answer !== "text" && !note && editing !== "note" ? (
            <Tooltip label="Add a note to this answer" keys={["C"]} side="top">
              <button
                type="button"
                className="compose-icon msg-note-add"
                aria-label={`Add a note to ${asked}`}
                onClick={() => onEdit("note")}
              >
                <MessageSquarePlus size={14} />
              </button>
            </Tooltip>
          ) : null}
        </div>
        {question.context ? <Context text={question.context} /> : null}
        {pending && question.answer === "choices" ? <span className="msg-hint">Select all that apply</span> : null}
        {pending && options.length > 0 ? (
          <div className="chips">
            <span
              className="chips-group"
              role={question.answer === "choices" ? "group" : "radiogroup"}
              aria-label={asked}
            >
              {options.map((value, i) => (
                <button
                  key={value}
                  type="button"
                  className={`chip ${isPicked(value) ? "is-picked" : ""}`}
                  role={question.answer === "choices" ? "checkbox" : "radio"}
                  aria-checked={isPicked(value)}
                  onClick={() => onPick(value)}
                >
                  {current && i < 9 ? <kbd className="chip-key">{i + 1}</kbd> : null}
                  {question.answer === "yes-no" ? (value === "yes" ? "Yes" : "No") : value}
                  {question.recommended === value ? (
                    <Star size={11} className="chip-star" aria-label="recommended" />
                  ) : null}
                </button>
              ))}
              <button
                type="button"
                className={`chip chip-other ${own ? "is-picked" : ""}`}
                aria-pressed={own}
                onClick={() => onEdit("own")}
              >
                Other…
              </button>
            </span>
          </div>
        ) : null}
        {pending && question.answer === "text" ? (
          <ComposeField
            className="msg-field"
            value={draft?.text ?? ""}
            onChange={(text) => onChange({ ...draft, text })}
            label={`Answer to ${asked}`}
            placeholder="Type your answer…"
            onSubmit={onNext}
            keys="Enter for the next question · Shift+Enter for a new line"
            boxProps={{ "data-reply": index }}
          />
        ) : null}
        {pending && editing === "own" ? (
          <ComposeField
            className="msg-field"
            value={draft?.text ?? ""}
            onChange={(text) => onChange({ ...draft, text, own: text.trim() !== "" })}
            label={`Answer to ${asked} in your own words`}
            placeholder="Answer in your own words…"
            onSubmit={close}
            onEscape={close}
            keys="Enter to save · Shift+Enter for a new line"
            send={{ label: "Save the answer", keys: ["Enter"] }}
            autoFocus
          />
        ) : pending && own ? (
          <SavedText
            label="Your answer"
            text={draft!.text!.trim()}
            what="answer"
            onEdit={() => onEdit("own")}
            onDelete={() => onChange({ ...draft, text: undefined, own: false })}
          />
        ) : null}
        {pending && editing === "note" ? (
          <ComposeField
            className="msg-field"
            value={draft?.note ?? ""}
            onChange={(text) => onChange({ ...draft, note: text })}
            label={`Note on ${asked}`}
            placeholder="Add a note…"
            onSubmit={close}
            onEscape={close}
            keys="Enter to save · Shift+Enter for a new line"
            send={{ label: "Save the note", keys: ["Enter"] }}
            autoFocus
          />
        ) : pending && note ? (
          <SavedText
            label="Note"
            text={note}
            what="note"
            onEdit={() => onEdit("note")}
            onDelete={() => onChange({ ...draft, note: undefined })}
          />
        ) : null}
      </div>
      {review.status === "decided" ? <AnswerLine question={question} answer={answer} review={review} /> : null}
    </div>
  );
}

/** A note or an answer in the person's own words, once saved: a pencil to
 *  edit it, and a bin, shown on hover, to delete it. */
function SavedText({
  label,
  text,
  what,
  onEdit,
  onDelete,
}: {
  label: string;
  text: string;
  what: "note" | "answer";
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="msg-saved" data-testid={`saved-${what}`}>
      <span className="msg-saved-label">{label}</span>
      <span className="msg-saved-text">{text}</span>
      <span className="msg-saved-actions">
        <Tooltip label={`Edit the ${what}`} side="top">
          <button type="button" className="compose-icon" aria-label={`Edit the ${what}`} onClick={onEdit}>
            <Pencil size={13} />
          </button>
        </Tooltip>
        <Tooltip label={`Delete the ${what}`} side="top">
          <button
            type="button"
            className="compose-icon msg-saved-delete"
            aria-label={`Delete the ${what}`}
            onClick={onDelete}
          >
            <Trash2 size={13} />
          </button>
        </Tooltip>
      </span>
    </div>
  );
}

type ComposerProps = {
  /** the person's comment to the agent on the whole set */
  comment: string;
  onComment: (text: string) => void;
  /** how many questions would go back unanswered, among several */
  left: number;
  busy: boolean;
  connected: boolean;
  onSend: () => void;
  onDecline: () => void;
};

/** The box under the thread: a comment to the agent on the questions as a
 *  whole, sent with the answers. Enter sends them, and a small button
 *  declines them. */
function Composer({ comment, onComment, left, busy, connected, onSend, onDecline }: ComposerProps) {
  const sendLabel = connected ? "Send the answers to the agent" : "Reconnect to send";
  return (
    <div className={`thread-composer ${busy ? "is-busy" : ""}`}>
      <ComposeField
        value={comment}
        onChange={onComment}
        label="Comment to the agent"
        placeholder="Add a comment to the agent on these questions…"
        onSubmit={onSend}
        keys={`${left > 0 ? `${left} unanswered · ` : ""}Enter to send · Shift+Enter for a new line`}
        tools={
          <Tooltip label="Decline: the agent is told to stop" keys={[MOD, "D"]} side="top">
            <button type="button" className="compose-icon" aria-label="Decline" onClick={onDecline}>
              <Ban size={14} />
            </button>
          </Tooltip>
        }
        send={{
          label: left > 0 ? `${sendLabel}, ${left} unanswered` : sendLabel,
          name: left > 0 ? `Send, ${left} unanswered` : "Send",
          keys: ["Enter"],
          disabled: busy || !connected,
          testId: "send-answers",
        }}
      />
    </div>
  );
}

/** The chat box with nothing to answer. */
/** The box under the thread while no question waits: a message to the
 *  agent, which it receives with its next status. */
function MessageComposer({ session, onSent }: { session: { agent: string; id: string }; onSent: () => void }) {
  const live = useLive();
  const toast = useToast();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const send = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    try {
      await api.sendMessage(session.agent, session.id, text);
      setText("");
      onSent();
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "The message was not sent.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className={`thread-composer ${busy ? "is-busy" : ""}`}>
      <ComposeField
        value={text}
        onChange={setText}
        label="Message to the agent"
        placeholder="Message the agent"
        onSubmit={send}
        keys="Enter to send · Shift+Enter for a new line"
        send={{
          label: live.connected ? "Send the message to the agent" : "Reconnect to send",
          name: "Send the message",
          keys: ["Enter"],
          disabled: busy || !live.connected || !text.trim(),
          testId: "send-message",
        }}
      />
      <p className="thread-composer-hint faint">The agent receives your message the next time it reports a status.</p>
    </div>
  );
}

/** A message the person sent the agent, on the right, with whether the
 *  agent has received it; one still queued can be deleted. */
function MessageBubble({
  message,
  session,
  onDeleted,
}: {
  message: SessionMessage;
  session: { agent: string; id: string };
  onDeleted: () => void;
}) {
  const toast = useToast();
  const queued = !message.delivered_at;
  const remove = async () => {
    try {
      await api.deleteMessage(session.agent, session.id, message.id);
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "The message was not deleted.");
    }
    onDeleted();
  };
  return (
    <div
      className={`thread-message ${queued ? "is-queued" : "is-delivered"}`}
      id={`message-${message.id}`}
      data-testid="message"
    >
      <div className="thread-message-row">
        {queued ? (
          <Tooltip label="Delete the message" side="top">
            <button type="button" className="thread-message-delete" aria-label="Delete the message" onClick={remove}>
              <Trash2 size={14} />
            </button>
          </Tooltip>
        ) : null}
        <div className="thread-message-bubble" title={`Sent ${stamp(message.sent_at)}`}>
          {message.text}
        </div>
      </div>
      {queued ? (
        <span className="thread-message-state" data-testid="message-state">
          <Clock size={12} aria-hidden /> Queued
        </span>
      ) : (
        <span
          className="thread-message-state"
          data-testid="message-state"
          title={`Delivered ${stamp(message.delivered_at)}`}
        >
          <Check size={12} aria-hidden /> Delivered {age(message.delivered_at)} ago
        </span>
      )}
    </div>
  );
}

function IdleComposer() {
  return (
    <div className="thread-composer is-idle">
      <ComposeField
        value=""
        onChange={() => {}}
        label="Comment to the agent"
        placeholder="No question is waiting for an answer"
        disabled
      />
    </div>
  );
}

/** The person's answer, in a line under the question. */
function AnswerLine({ question, answer, review }: { question: Question; answer?: Answer; review: Review }) {
  const text =
    !answer || answer.unanswered
      ? null
      : answer.choice !== undefined
        ? answer.choice
        : answer.choices
          ? answer.choices.join(", ")
          : answer.yes !== undefined
            ? answer.yes
              ? "Yes"
              : "No"
            : (answer.text ?? null);
  const own = !!answer?.text && question.answer !== "text";
  const by = review.decision?.decided_by ?? "You";
  return (
    <div className="msg-answer" data-testid="answer">
      {text === null ? (
        <span className="faint">Not answered</span>
      ) : (
        <>
          <Check size={13} className="msg-answer-check" aria-hidden />
          <span>
            {by} answered{own ? " in their own words" : ""}
          </span>
          <span className={`msg-answer-text ${own ? "msg-own" : ""}`}>{text}</span>
        </>
      )}
      <span className="faint" title={stamp(review.decision?.decided_at)}>
        · {age(review.decision?.decided_at)} ago
      </span>
      {answer?.note ? (
        <div className="msg-note" data-testid="answer-note">
          <CornerDownRight size={12} className="msg-note-mark" aria-label="Note" />
          <span className="msg-note-text">{answer.note}</span>
        </div>
      ) : null}
    </div>
  );
}

/** A question's context, as Markdown, with its links opened in the browser. */
function Context({ text }: { text: string }) {
  const html = useMemo(() => renderMarkdown(text), [text]);
  return (
    <div
      className="msg-context"
      onClick={(e) => {
        const link = (e.target as HTMLElement).closest("a");
        if (!link) return;
        e.preventDefault();
        openExternal(link.href);
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

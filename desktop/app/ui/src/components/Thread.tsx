// An agent session's thread: the questions it asked, as the agent's
// messages, and the reviews it sent, oldest first. Questions are answered
// here, each with its chips or a reply box, and sent as a set.
import { ArrowDown, CheckCircle2, Expand, GitBranch, PanelRightClose, PanelRightOpen, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useNavigate, type To } from "react-router";
import type { Review, Session } from "../api/types";
import { age, stamp } from "../lib/format";
import { MOD, SHIFT, hasMod, modalOpen, isTyping } from "../lib/keys";
import { finder, type Finder, type Found } from "../lib/findText";
import { usePageRepo } from "../lib/pageRepo";
import { FindBar, useFindBar } from "./FindBar";
import { useUrlParams } from "../lib/url";
import { timeline as entriesOf, fromSession, newestRounds, stages } from "../lib/sessions";
import { useSettings } from "../state/settings";
import { ItemChip, ItemOpener } from "./ItemChip";
import { AgentIcon } from "./AgentIcon";
import { ArchiveButton, useToggleArchive } from "./ArchiveButton";
import { SessionItemPanel } from "./SessionItemPanel";
import { SidePanel } from "./SidePanel";
import { CopyButton } from "./CopyButton";
import { Tooltip } from "./Tooltip";
import { ThreadEntries } from "./thread/ThreadEntries";
import { IdleComposer, MessageComposer } from "./thread/composers";
import { ThreadDetails } from "./thread/ThreadDetails";
import { useSessionThread } from "./thread/useSessionThread";
import { useNewestFirst } from "./thread/useNewestFirst";
import { useThreadScroll } from "./thread/useThreadScroll";
import { store, stored } from "./thread/storage";
import { useWindowKey } from "../lib/useWindowKey";
import { useResizableWidth } from "../lib/useResizableWidth";
import { useLatest } from "../lib/useLatest";

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
  const { session, items, statuses, messages, comments, itemEvents, error, load, sessionItems } = useSessionThread(
    named,
    focus,
    onSession,
  );
  // what the session holds now, in its header
  const held = sessionItems.filter(
    (i) => i.status === "in_progress" && i.claim?.agent === named?.agent && i.claim?.session === named?.id,
  );
  const navigate = useNavigate();
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
  const { atEnd } = useThreadScroll(timeline, footer, { focus, items, session, stage });

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
  const [found, setFound] = useState<Found | null>(null);
  const finderRef = useRef<Finder | null>(null);
  useWindowKey((event) => {
    if (!hasMod(event) || event.shiftKey || event.altKey || (event.key !== "f" && event.key !== "F")) return;
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
  // a review opened from here names the session in its breadcrumb
  // the same objects from one render to the next, for the memoized entries
  const namedRef = useMemo(
    () => (named ? { agent: named.agent, id: named.id } : null),
    [named?.agent, named?.id], // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the object
  );
  const from = useMemo(
    () => (namedRef ? fromSession({ ...namedRef, title: session?.title }) : undefined),
    [namedRef, session?.title],
  );
  // each review once, by its newest round
  const newest = useMemo(() => newestRounds(items), [items]);
  const waiting = newest.filter((r) => r.status === "pending");
  const started = useMemo(() => stages(statuses), [statuses]);
  // the reviews each stage sent, for its tooltip
  const entries = useMemo(
    () => entriesOf(items, statuses, messages, comments, itemEvents),
    [items, statuses, messages, comments, itemEvents],
  );
  // a long thread draws its newest entries first, unless it opens at one
  // further up
  const { shown, whole } = useNewestFirst(entries, timeline, {
    all: !!focus || !!stage,
    thread: named ? `${named.agent}/${named.id}` : null,
  });

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
              {held.length ? (
                // one item with its title; several by their keys, the titles
                // on hover
                <span className="thread-meta-item" data-testid="thread-held-item">
                  working on{" "}
                  {held.map((item) => (
                    <ItemChip key={item.key} item={item} keyOnly={held.length > 1} />
                  ))}
                </span>
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
        <div
          className="thread-body"
          ref={body}
          style={{ "--details-width": `${detailsWidth}px` } as React.CSSProperties}
        >
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
                else document.getElementById(`set-${r.id}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
              }}
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

/** Copies the session's id, for a message to the agent or a command line. */
function CopySessionId({ id }: { id: string }) {
  return <CopyButton text={id} label="Copy the id" ariaLabel="Copy the session id" />;
}

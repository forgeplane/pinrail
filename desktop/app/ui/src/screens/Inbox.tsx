import { Ban, CheckCheck, MessageCircleQuestion, SearchX } from "lucide-react";
import { type MouseEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, type To } from "react-router";
import type { Review } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { DiscardDialog } from "../components/DiscardDialog";
import { ReviewIcon, reviewLabel } from "../components/ReviewIcon";
import { Tooltip } from "../components/Tooltip";
import { AgentIcon } from "../components/AgentIcon";
import { FilesCount, SummaryCounts } from "../components/Badges";
import { age } from "../lib/format";
import { clearAll, useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { inProject } from "../lib/url";
import { MOD, hasMod, modalOpen } from "../lib/keys";
import { useSettings } from "../state/settings";
import { SidePanel } from "../components/SidePanel";
import { StarButton } from "../components/StarButton";
import { History } from "./History";
import { InboxHead, tabOf } from "./InboxHead";
import { ReviewView } from "./ReviewScreen";

/** How the inbox is laid out, remembered on this machine. */
type Layout = "projects" | "list";
const LAYOUT = "pinrail.inbox.layout";
function savedLayout(): Layout {
  try {
    return localStorage.getItem(LAYOUT) === "list" ? "list" : "projects";
  } catch {
    return "projects";
  }
}

function matches(review: Review, q: string) {
  if (!q) return true;
  const text = [
    review.title,
    reviewLabel(review),
    review.requested_by,
    review.origin.repo,
    review.origin.workflow,
    review.origin.ref,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return text.includes(q.toLowerCase());
}

export function Inbox() {
  const [params] = useUrlParams();
  return tabOf(params) === "history" ? <History /> : <Waiting />;
}

/** The Pending and New rounds tabs, from the live list of what waits. */
function Waiting() {
  const live = useLive();
  const navigate = useNavigate();
  const [params, updateParams, currentParams] = useUrlParams();
  const { settings: prefs } = useSettings();
  // with the side panel, ?open= names the review shown beside the list
  const panelMode = prefs.inbox.open_in === "panel";
  const openId = panelMode ? params.get("open") : null;
  // The search box keeps its own text: a router navigation per keystroke is
  // deferred, and a controlled input bound to the URL would snap back.
  const [q, setQ] = useState(params.get("q") ?? "");
  const repo = params.get("repo") ?? "";
  const plugin = params.get("plugin") ?? "";
  const rounds = params.get("rounds") ?? "";
  const starred = params.get("starred") === "1";
  const [focused, setFocused] = useState(0);
  const [layout, setLayoutState] = useState<Layout>(savedLayout);
  const setLayout = (next: Layout) => {
    setLayoutState(next);
    try {
      localStorage.setItem(LAYOUT, next);
    } catch {
      // private mode or no storage: the choice lasts until the app restarts
    }
    setFocused(0);
  };
  const [discarding, setDiscarding] = useState<Review | null>(null);
  const search = useRef<HTMLInputElement>(null);
  // While the keyboard moves the focus the list scrolls under a still
  // pointer, and the row that slides under it must not take the focus
  // back; the next real movement of the pointer hands it over again.
  const keyboard = useRef(false);
  useEffect(() => {
    const onMove = () => {
      keyboard.current = false;
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, []);

  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));

  /** A page number, or a step from the page the URL is on. */
  const setPage = (n: number | ((current: number) => number)) => {
    updateParams((next) => {
      const now = Math.min(pageOf(next.get("page")), pages);
      const wanted = typeof n === "function" ? n(now) : n;
      const value = Math.min(Math.max(1, wanted), pages);
      if (value > 1) next.set("page", String(value));
      else next.delete("page");
    });
    setFocused(0);
    window.scrollTo({ top: 0 });
  };
  const setSize = (n: number) => {
    updateParams(
      (next) => {
        next.set("per", String(n));
        next.delete("page");
      },
      { replace: true },
    );
  };

  const reviews = useMemo(
    () =>
      live.pending.filter(
        (r) =>
          matches(r, q) &&
          inProject(r.origin.repo, repo) &&
          (!plugin || r.plugin === plugin) &&
          (rounds !== "new" || !!r.revises) &&
          (!starred || !!r.starred),
      ),
    [live.pending, q, repo, plugin, rounds, starred],
  );
  const plugins = [...new Set(live.pending.map((r) => r.plugin))].sort();

  // In the order the rows are drawn. By project: the project with the
  // newest review first, and newest first within one; the reviews come
  // newest first, so a project's place is where its newest one falls. As a
  // list: newest first, as they come. J, K and Enter walk this order, so
  // the row they open is the row lit.
  const ordered = useMemo(() => {
    if (layout === "list") return reviews;
    const byRepo = new Map<string, Review[]>();
    for (const r of reviews) {
      const key = r.origin.repo ?? "";
      byRepo.set(key, [...(byRepo.get(key) ?? []), r]);
    }
    return [...byRepo.entries()].flatMap(([, items]) => items);
  }, [reviews, layout]);

  // a page past the end, after reviews were decided: the last page there is
  const pages = Math.max(1, Math.ceil(ordered.length / size));
  const current = Math.min(page, pages);
  const shown = useMemo(() => ordered.slice((current - 1) * size, current * size), [ordered, current, size]);

  const groups = useMemo(() => {
    const byRepo = new Map<string, Review[]>();
    for (const r of shown) {
      const key = r.origin.repo ?? "";
      byRepo.set(key, [...(byRepo.get(key) ?? []), r]);
    }
    return [...byRepo.entries()];
  }, [shown]);

  useEffect(() => {
    setFocused((f) => Math.min(f, Math.max(0, shown.length - 1)));
  }, [shown.length]);

  // The highlight follows the review open in the side panel, whether opened
  // by a click, a shortcut or after a decision. A pointer over another row
  // moves the highlight only once it moves.
  const shownRef = useRef(shown);
  shownRef.current = shown;
  useEffect(() => {
    if (!openId) return;
    const at = shownRef.current.findIndex((r) => r.id === openId);
    if (at < 0) return;
    keyboard.current = true;
    setFocused(at);
  }, [openId]);

  /** The link of a review's row: the side panel, keeping the filters, or its page. */
  const hrefFor = useCallback(
    (id: string): To => {
      if (!panelMode) return `/reviews/${id}`;
      const next = new URLSearchParams(params);
      next.set("open", id);
      return { pathname: "/", search: `?${next}` };
    },
    [panelMode, params],
  );
  /** Opens a review in the side panel, or closes the panel with null. */
  const openInPanel = useCallback(
    (id: string | null, options?: { replace?: boolean }) =>
      updateParams((next) => {
        if (id) next.set("open", id);
        else next.delete("open");
      }, options),
    [updateParams],
  );
  // the rows in display order, which the side panel follows
  const orderedRef = useRef(ordered);
  orderedRef.current = ordered;
  // where the open review was in the list when last seen there: once it has
  // ended, the list may have dropped it before the panel moves on
  const openAt = useRef(-1);
  const seenAt = ordered.findIndex((r) => r.id === openId);
  if (seenAt >= 0) openAt.current = seenAt;
  /** The review after the one that just ended, or before it at the end of the list. */
  const openNext = useCallback(() => {
    const list = orderedRef.current;
    const at = list.findIndex((r) => r.id === openId);
    // still listed: the one after it; gone: the one now in its place
    const next = at >= 0 ? (list[at + 1] ?? list[at - 1]) : (list[openAt.current] ?? list[list.length - 1]);
    openInPanel(next && next.id !== openId ? next.id : null, { replace: true });
  }, [openId, openInPanel]);
  const closePanel = useCallback(() => openInPanel(null), [openInPanel]);
  /**
   * A plain click on a review's link in the panel mode: opens it in the side
   * panel, replacing the history entry when the panel is open already. The
   * address decides that, as a second click may come before the render that
   * shows the first.
   */
  const openFromLink = (event: MouseEvent, id: string) => {
    if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    openInPanel(id, { replace: currentParams().has("open") });
  };

  // the element that holds the list and the side panel
  const split = useRef<HTMLDivElement>(null);

  // With a review open beside the list, J and K move to the next and
  // previous review, from the list or from the view, which forwards them;
  // ⌥↓ and ⌥↑ do too. All of them wrap around at the ends. The listener uses
  // the capture phase, so it runs before the layout's handler for waiting
  // reviews, which then ignores the key.
  useEffect(() => {
    const onWalk = (event: KeyboardEvent) => {
      if (!openId || modalOpen() || hasMod(event)) return;
      const walk = event.altKey && !event.shiftKey && (event.key === "ArrowDown" || event.key === "ArrowUp");
      const step = !event.altKey && !event.shiftKey && (event.key === "j" || event.key === "k");
      if (!walk && !step) return;
      const el = event.target as HTMLElement | null;
      if (step && el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      const list = orderedRef.current;
      if (!list.length) return;
      event.preventDefault();
      const at = list.findIndex((r) => r.id === openId);
      const down = event.key === "ArrowDown" || event.key === "j";
      const next = at < 0 ? 0 : (at + (down ? 1 : list.length - 1)) % list.length;
      if (list[next].id !== openId) openInPanel(list[next].id, { replace: true });
    };
    window.addEventListener("keydown", onWalk, true);
    return () => window.removeEventListener("keydown", onWalk, true);
  }, [openId, openInPanel]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // the inbox's keys are not for a dialog in front of it
      if (modalOpen()) return;
      // the open review handles the other keys: the panel handles discard,
      // and the view may use Enter
      if (openId) {
        if (event.key === "/" && !event.metaKey && !event.ctrlKey && !event.altKey) {
          const el = event.target as HTMLElement | null;
          if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA")) return;
          event.preventDefault();
          search.current?.focus();
        }
        return;
      }
      if (hasMod(event) && !event.altKey && !event.shiftKey && (event.key === "d" || event.key === "D")) {
        // the key must not land in the reason field that opens
        event.preventDefault();
        if (!discarding && shown[focused]) setDiscarding(shown[focused]);
        return;
      }
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const el = event.target as HTMLElement | null;
      const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT");
      if (event.key === "/" && !typing) {
        event.preventDefault();
        search.current?.focus();
        return;
      }
      if (typing) {
        if (event.key === "Escape") (el as HTMLElement).blur();
        return;
      }
      if (discarding) return;
      if (event.key === "j" || event.key === "k") keyboard.current = true;
      // wrap around at the ends, from the last row to the first and back
      if (shown.length && event.key === "j") setFocused((f) => (f + 1) % shown.length);
      if (shown.length && event.key === "k") setFocused((f) => (f - 1 + shown.length) % shown.length);
      if (event.key === "Enter" && shown[focused]) navigate(hrefFor(shown[focused].id));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [shown, focused, navigate, discarding, openId, openInPanel, hrefFor]);

  /** A review's row; `here` is its place in the order J and K walk. */
  const row = (review: Review, here: number) => (
    <Link
      key={review.id}
      to={hrefFor(review.id)}
      onClick={(event) => openFromLink(event, review.id)}
      className={`review-row ${review.kind === "question" ? "is-question" : ""} ${here === focused ? "is-focused" : ""} ${review.id === openId ? "is-open" : ""}`}
      aria-current={review.id === openId ? "true" : undefined}
      onMouseEnter={() => !keyboard.current && setFocused(here)}
      data-testid="review-row"
    >
      {/* questions an agent asked are marked apart from the reviews it sent */}
      {review.kind === "question" ? (
        <MessageCircleQuestion className="review-row-asks" size={15} aria-label="Asks you" />
      ) : (
        <span className="review-row-marker" aria-label="Pending" />
      )}
      <span className="review-row-main">
        <span className="review-row-title">
          {review.title}
          {review.origin.ref ? <span className="review-row-ref">{review.origin.ref}</span> : null}
        </span>
        <span className="review-row-meta">
          {layout === "list" ? (
            <span className="review-row-project" data-testid="review-row-project">
              {review.origin.repo ?? "No project"}
            </span>
          ) : null}
          {layout === "list" && (review.requested_by || review.origin.workflow) ? <span>·</span> : null}
          {review.requested_by ? (
            <span className="requester">
              <AgentIcon requestedBy={review.requested_by} />
              {review.requested_by}
            </span>
          ) : null}
          {review.requested_by && review.origin.workflow ? <span>·</span> : null}
          {review.origin.workflow ? <span>{review.origin.workflow}</span> : null}
          {review.revises ? <span>· New round</span> : null}
          <FilesCount total={review.attachments_total} />
        </span>
      </span>
      {review.kind === "question" ? (
        <span className="review-row-plugin">
          <span className="asks-chip">asks you</span>
        </span>
      ) : (
        <span className="review-row-plugin" title={reviewLabel(review)}>
          <ReviewIcon review={review} size={13} />
          {reviewLabel(review)}
        </span>
      )}
      <span className="review-row-summary">
        <SummaryCounts summary={review.summary} />
      </span>
      <span className="review-row-end">
        <time className="review-row-time" dateTime={review.created_at}>
          {age(review.created_at, now)}
        </time>
        {review.kind === "question" ? null : <StarButton review={review} inRow ofReview />}
        <Tooltip label="Discard: the agent is told to stop" keys={[MOD, "D"]} side="top">
          <button
            type="button"
            className="bar-button"
            aria-label={`Discard ${review.title}`}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setDiscarding(review);
            }}
          >
            <Ban size={14} />
          </button>
        </Tooltip>
      </span>
    </Link>
  );

  const filtered = !!(q || repo || plugin || rounds || starred);
  const now = Date.now();
  let index = -1;

  return (
    <div className={`inbox ${openId ? "has-panel" : ""}`} ref={split}>
      <div className="inbox-side split-side">
        <InboxHead
          tab={rounds === "new" ? "new" : "pending"}
          search={search}
          q={q}
          onSearch={setQ}
          projects={live.projects}
          unassigned={live.unassigned > 0}
          plugins={plugins}
          filtered={!!(q || repo || plugin || starred)}
          onClear={() => setQ("")}
          layout={{ value: layout, set: setLayout }}
        />
        {live.pendingUnloaded > 0 ? (
          <p className="notice" data-testid="inbox-capped">
            The inbox shows the newest {live.pending.length} of {live.pendingCount} pending reviews. Decide or withdraw
            some to see the rest.
          </p>
        ) : null}
        {groups.length === 0 ? (
          <EmptyState
            title={filtered ? "No matching reviews" : "All caught up"}
            icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <CheckCheck size={28} strokeWidth={1.5} />}
            actions={
              filtered ? (
                <button
                  type="button"
                  className="chrome-button"
                  onClick={() => {
                    setQ("");
                    updateParams(clearAll, { replace: true });
                  }}
                >
                  Clear filters
                </button>
              ) : (
                <Link to="/?tab=history" className="chrome-button">
                  View history
                </Link>
              )
            }
          >
            {filtered
              ? "Try a different search or clear the filters."
              : "New reviews appear here when an agent needs you."}
          </EmptyState>
        ) : layout === "list" ? (
          <div className="inbox-list">{shown.map((review, i) => row(review, i))}</div>
        ) : (
          <div className="inbox-groups">
            {groups.map(([repoName, items]) => (
              <details key={repoName || "none"} className="inbox-repo" open data-testid="inbox-group">
                <summary data-testid="inbox-group-heading">
                  <strong data-testid="inbox-group-name">{repoName || "No project"}</strong>
                  <span>{reviews.filter((r) => (r.origin.repo ?? "") === repoName).length}</span>
                </summary>
                {items.map((review) => {
                  index += 1;
                  return row(review, index);
                })}
              </details>
            ))}
          </div>
        )}
        <Pager label="Inbox" page={current} size={size} total={ordered.length} onPage={setPage} onSize={setSize} />
        {discarding ? (
          <DiscardDialog
            review={discarding}
            onClose={() => setDiscarding(null)}
            onDone={(done) => {
              setDiscarding(null);
              // the review open in the panel: on to the next, as after a hand-over
              if (done.id === openId) openNext();
            }}
          />
        ) : null}
        <footer className="inbox-footer">
          {openId ? (
            <>
              <kbd>J</kbd> <kbd>K</kbd> next and previous · <kbd>Esc</kbd> close · <kbd>/</kbd> search
            </>
          ) : (
            <>
              <kbd>J</kbd> <kbd>K</kbd> move · <kbd>↵</kbd> open · <kbd>/</kbd> search
            </>
          )}
        </footer>
      </div>
      {openId ? (
        <SidePanel split={split}>
          <ReviewView id={openId} inPanel linkTo={hrefFor} onEnded={openNext} onClose={closePanel} />
        </SidePanel>
      ) : null}
    </div>
  );
}

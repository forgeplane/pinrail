import { Archive, MoreHorizontal, SearchX, Star, Trash2 } from "lucide-react";
import { type MouseEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, type To } from "react-router";
import { api } from "../api/client";
import type { Review, ReviewListing } from "../api/types";
import { AgentIcon } from "../components/AgentIcon";
import { FilesCount, OutcomeBadge, SummaryCounts, outcomeOf } from "../components/Badges";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { ReviewIcon, reviewLabel } from "../components/ReviewIcon";
import { Tooltip } from "../components/Tooltip";
import { age, settledAt, stamp } from "../lib/format";
import { useUrlParams } from "../lib/url";
import { useLive } from "../state/live";
import { modalOpen } from "../lib/keys";
import { SourceBadge } from "../components/SourceBadge";
import { SidePanel } from "../components/SidePanel";
import { useToggleStar } from "../components/StarButton";
import { DeleteReviewDialog } from "../components/DeleteReviewDialog";
import { PopoverMenu, type MenuEntry } from "../components/PopoverMenu";
import { useToast } from "../state/toasts";
import { InboxHead } from "./InboxHead";
import { ReviewView } from "./ReviewScreen";
import { useSettings } from "../state/settings";

const ENDED = "decided,withdrawn,discarded,expired";
/** How long the search waits for typing to pause before it asks the server. */
const SEARCH_DELAY_MS = 250;

/** The inbox's History tab: every review that has ended, newest first. */
export function History() {
  const live = useLive();
  const [params, updateParams, currentParams] = useUrlParams();
  const navigate = useNavigate();
  const { settings: prefs } = useSettings();
  const [data, setData] = useState<ReviewListing | null>(null);
  const [focused, setFocused] = useState(0);
  const [menu, setMenu] = useState<{ review: Review; anchor: HTMLElement } | null>(null);
  const [deleting, setDeleting] = useState<Review | null>(null);
  // bumped after a delete made here, so the list shows it at once
  const [reloads, setReloads] = useState(0);
  const toggleStar = useToggleStar();
  const toast = useToast();
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
  const status = params.get("status") ?? "";
  const plugin = params.get("plugin") ?? "";
  const repo = params.get("repo") ?? "";
  const project = params.get("project") ?? "";
  const starred = params.get("starred") === "1";
  // the search box keeps its own text; see the inbox for why
  const [q, setQ] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));
  const filtered = !!(q || status || plugin || repo || project || starred);
  // with the side panel, as the inbox opens reviews, ?open= names the review shown beside the list
  const panelMode = prefs.inbox.open_in === "panel";
  const openId = panelMode ? params.get("open") : null;

  // The server filters and pages: the history can be far longer than any
  // one fetch. The search waits for typing to pause.
  const [debounced, setDebounced] = useState(query);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(query), SEARCH_DELAY_MS);
    return () => window.clearTimeout(t);
  }, [query]);
  useEffect(() => {
    let cancelled = false;
    api
      .listReviews({
        status: status || ENDED,
        include_revised: "true",
        q: debounced,
        plugin,
        repo,
        project,
        starred: starred ? "true" : undefined,
        offset: String((page - 1) * size),
        limit: String(size),
        include: "facets",
      })
      .then((p) => !cancelled && setData(p))
      .catch(() => !cancelled && setData(null));
    return () => {
      cancelled = true;
    };
  }, [status, debounced, plugin, repo, project, starred, page, size, live.historyVersion, reloads]);

  const reviews = useMemo(() => data?.reviews ?? [], [data]);
  const total = data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / size));

  // a page past the end, after a filter or a sweep shrank the list: the last page
  useEffect(() => {
    if (data && total > 0 && reviews.length === 0 && page > 1) setPage(Math.ceil(total / size));
  }, [data]); // eslint-disable-line react-hooks/exhaustive-deps -- only when a page arrives, not when the page number changes

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

  const plugins = data?.facets?.plugins ?? [];

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
  /** Opens a review: in the side panel, or on its own page. */
  const open = (id: string) => {
    if (panelMode) openInPanel(id, { replace: currentParams().has("open") });
    else navigate(`/reviews/${id}`, { state: { from: "history" } });
  };
  // the element that holds the list and the side panel
  const split = useRef<HTMLDivElement>(null);

  // The focus follows the review open in the side panel. Opened from the
  // keyboard, the list may scroll under a still pointer, and the row that
  // slides under it must not take the focus back.
  useEffect(() => {
    if (!openId) return;
    const at = reviews.findIndex((r) => r.id === openId);
    if (at < 0) return;
    keyboard.current = true;
    setFocused(at);
  }, [openId, reviews]);

  // With a review open beside the list, J and K open the next and previous
  // review, from the list or from the view, which forwards them, and wrap
  // around at the ends of the page.
  useEffect(() => {
    const onWalk = (event: KeyboardEvent) => {
      if (!openId || modalOpen() || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
      if (event.key !== "j" && event.key !== "k") return;
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (!reviews.length) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const at = reviews.findIndex((r) => r.id === openId);
      const next = at < 0 ? 0 : (at + (event.key === "j" ? 1 : reviews.length - 1)) % reviews.length;
      if (reviews[next].id !== openId) openInPanel(reviews[next].id, { replace: true });
    };
    window.addEventListener("keydown", onWalk, true);
    return () => window.removeEventListener("keydown", onWalk, true);
  }, [openId, reviews, openInPanel]);

  useEffect(() => {
    setFocused((f) => Math.min(f, Math.max(0, reviews.length - 1)));
  }, [reviews.length]);

  // the same keys as the inbox: j/k move, enter opens, / searches
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT");
      if (event.key === "/" && !typing) {
        event.preventDefault();
        search.current?.focus();
        return;
      }
      if (typing) {
        if (event.key === "Escape") el.blur();
        // Enter leaves the field for the results, so J, K and Enter work on them
        if (event.key === "Enter" && el === search.current) {
          event.preventDefault();
          el.blur();
          setFocused(0);
        }
        return;
      }
      if (event.key === "j" || event.key === "k") keyboard.current = true;
      if (event.key === "j") setFocused((f) => Math.min(f + 1, reviews.length - 1));
      if (event.key === "k") setFocused((f) => Math.max(f - 1, 0));
      if (event.key === "Enter" && reviews[focused] && !openId) open(reviews[focused].id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reviews, focused, navigate, openId, panelMode]); // eslint-disable-line react-hooks/exhaustive-deps -- open follows panelMode and openId

  useEffect(() => {
    document.querySelector<HTMLElement>(`[data-history-row="${focused}"]`)?.scrollIntoView({ block: "nearest" });
  }, [focused]);
  const repos = data?.facets?.repos ?? [];
  const now = Date.now();

  return (
    <div className={`inbox history ${openId ? "has-panel" : ""}`} ref={split}>
      <div className="inbox-side split-side">
        <InboxHead
          tab="history"
          search={search}
          q={q}
          onSearch={setQ}
          projects={repos}
          unassigned={!!data?.facets?.unassigned}
          plugins={plugins}
          filtered={filtered}
          onClear={() => setQ("")}
        />
        {!data ? null : total === 0 ? (
          <EmptyState
            title={filtered ? "No matching decisions" : "Your decisions belong here"}
            icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <Archive size={28} strokeWidth={1.5} />}
          >
            {filtered
              ? "Nothing matches these filters."
              : "Decided, withdrawn, discarded and expired reviews appear here, with the view they were decided in."}
          </EmptyState>
        ) : (
          <div className="inbox-list history-list">
            {reviews.map((r, i) => {
              const outcome = outcomeOf(r);
              return (
                <Link
                  key={r.id}
                  to={hrefFor(r.id)}
                  onClick={(event) => openFromLink(event, r.id)}
                  state={panelMode ? undefined : { from: "history" }}
                  className={`review-row ${i === focused ? "is-focused" : ""} ${r.id === openId ? "is-open" : ""}`}
                  aria-current={r.id === openId ? "true" : undefined}
                  data-history-row={i}
                  data-testid="history-row"
                  onMouseEnter={() => !keyboard.current && !menu && setFocused(i)}
                >
                  <span className={`review-row-marker is-ended status-${outcome.tone}`} aria-label={outcome.label} />
                  <span className="review-row-main">
                    <span className="review-row-title">
                      {r.title}
                      {r.origin.ref ? <span className="review-row-ref">{r.origin.ref}</span> : null}
                    </span>
                    <span className="review-row-meta">
                      <span className="review-row-project">{r.origin.repo ?? "No repository"}</span>
                      {r.requested_by ? (
                        <>
                          <span>·</span>
                          <span className="requester">
                            <AgentIcon requestedBy={r.requested_by} />
                            {r.requested_by}
                          </span>
                        </>
                      ) : null}
                      <span>·</span>
                      <EndedBy review={r} />
                      <FilesCount total={r.attachments_total} />
                    </span>
                  </span>
                  <span className="review-row-plugin" title={reviewLabel(r)}>
                    <ReviewIcon review={r} size={13} />
                    {reviewLabel(r)}
                  </span>
                  <span className="review-row-summary history-outcome">
                    <OutcomeBadge review={r} />
                    <SummaryCounts summary={r.decision?.summary} />
                  </span>
                  <span className="review-row-end">
                    <Tooltip label={stamp(settledAt(r))} side="top">
                      <time className="review-row-time" dateTime={settledAt(r) ?? undefined}>
                        {age(settledAt(r), now)}
                      </time>
                    </Tooltip>
                    {/* a starred round shows its star; the space keeps the times in line */}
                    {r.starred_at ? (
                      <Star size={14} className="history-star" fill="currentColor" aria-label="Starred" />
                    ) : (
                      <span className="star-slot" />
                    )}
                    <button
                      type="button"
                      className="bar-button review-row-menu"
                      aria-label={`${r.title} actions`}
                      aria-haspopup="menu"
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        setFocused(i);
                        setMenu({ review: r, anchor: event.currentTarget });
                      }}
                    >
                      <MoreHorizontal size={14} />
                    </button>
                  </span>
                </Link>
              );
            })}
          </div>
        )}
        {data ? (
          <Pager label="History" page={page} size={size} total={total} onPage={setPage} onSize={setSize} />
        ) : null}
      </div>
      {menu ? (
        <PopoverMenu
          anchor={menu.anchor}
          label={menu.review.title}
          align="right"
          onClose={() => setMenu(null)}
          entries={[
            ...(menu.review.kind === "question"
              ? []
              : [
                  {
                    label: menu.review.starred_at ? "Remove star" : "Star",
                    icon: Star,
                    onPick: () => toggleStar(menu.review),
                  } as MenuEntry,
                  { kind: "separator" } as MenuEntry,
                ]),
            {
              label: menu.review.kind === "question" ? "Delete questions…" : "Delete review…",
              icon: Trash2,
              danger: true,
              onPick: () => setDeleting(menu.review),
            },
          ]}
        />
      ) : null}
      {deleting ? (
        <DeleteReviewDialog
          review={deleting}
          onClose={() => setDeleting(null)}
          onDone={() => {
            if (deleting.id === openId) closePanel();
            setDeleting(null);
            setReloads((n) => n + 1);
            toast("Deleted");
          }}
        />
      ) : null}
      {openId ? (
        <SidePanel split={split}>
          <ReviewView id={openId} inPanel linkTo={hrefFor} onClose={closePanel} />
        </SidePanel>
      ) : null}
    </div>
  );
}

/** Who ended a review, and how when it was not in the app. */
function EndedBy({ review: r }: { review: Review }) {
  if (r.decision) {
    return (
      <span>
        {r.decision.decided_by ? `Decided by ${r.decision.decided_by}` : "Decided"}
        <SourceBadge review={r} made={r.decision} command="decide" />
      </span>
    );
  }
  if (r.status === "withdrawn") {
    return (
      <span>
        <Reason text={r.withdrawn_by ? `Withdrawn by ${r.withdrawn_by}` : "Withdrawn"} reason={r.withdrawn_reason} />
        <SourceBadge review={r} made={{ via: r.withdrawn_via, session: r.withdrawn_session }} command="withdraw" />
      </span>
    );
  }
  if (r.status === "discarded") {
    return (
      <span>
        <Reason text={r.discarded_by ? `Discarded by ${r.discarded_by}` : "Discarded"} reason={r.discarded_reason} />
        <SourceBadge review={r} made={{ via: r.discarded_via, session: r.discarded_session }} command="discard" />
      </span>
    );
  }
  return <span>Expired</span>;
}

/** Who withdrew or discarded a review, with the reason they gave in a tooltip. */
function Reason({ text, reason }: { text: string; reason: string | null }) {
  if (!reason) return <>{text}</>;
  return (
    <Tooltip label={reason}>
      <span className="history-reason" tabIndex={0} data-testid="history-reason">
        {text}
      </span>
    </Tooltip>
  );
}

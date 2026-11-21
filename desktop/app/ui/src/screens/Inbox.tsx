import { CheckCheck, SearchX } from "lucide-react";
import { TelemetryNotice } from "../components/TelemetryNotice";
import { Fragment, type MouseEvent, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, type To } from "react-router";
import { api, type NotificationQuery } from "../api/client";
import type { InboxCounts, Notification, NotificationList, NotificationTab, Review } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { Pager, pageOf, pageSizeOf } from "../components/Pager";
import { DiscardDialog } from "../components/DiscardDialog";
import { clearAll, useUrlParams } from "../lib/url";
import { useLive, useNoticeCount } from "../state/live";
import { hasMod, modalOpen, isTyping } from "../lib/keys";
import { useSettings } from "../state/settings";
import { SidePanel } from "../components/SidePanel";
import { History } from "./History";
import { InboxHead, tabOf, type InboxTab } from "./InboxHead";
import { ReviewView } from "./ReviewScreen";
import { InboxRow, type InboxRowActions } from "../components/InboxRow";
import { NotificationEvents } from "../components/NotificationEvents";
import { NotificationRow, notificationPath, unfolds, type NotificationActions } from "../components/NotificationRow";
import { useWindowKey } from "../lib/useWindowKey";
import { useListKeys } from "../lib/useListKeys";
import { useSlashSearch } from "../lib/useSlashSearch";
import { useLatest } from "../lib/useLatest";
import { useDebounced } from "../lib/useDebounced";
import { useLoad } from "../lib/useLoad";

/** How many waiting notifications the All tab shows above the updates: all
 *  of them, up to the core's page. */
const WAITING_SHOWN = 200;
/** How long the search waits for typing to pause before it asks the core. */
const SEARCH_DELAY_MS = 250;

export function Inbox() {
  const [params, updateParams] = useUrlParams();
  // the New rounds tab of earlier versions is a choice of the Kind filter
  const oldRounds = params.get("rounds") === "new";
  useEffect(() => {
    if (!oldRounds) return;
    updateParams(
      (next) => {
        next.delete("rounds");
        next.set("kind", "new_round");
      },
      { replace: true },
    );
  }, [oldRounds, updateParams]);
  const tab = tabOf(params);
  return tab === "history" ? <History /> : <Notifications tab={tab} />;
}

/** Whether a notice can change what the inbox lists. */
const aboutNotifications = (notice: { notification?: unknown; kind: string }) =>
  !!notice.notification || notice.kind === "notification_changed" || notice.kind === "resync";

/** A list of notifications from the core, read again when one changes. */
function useNotifications(query: NotificationQuery | null): NotificationList | null {
  const changed = useNoticeCount(aboutNotifications);
  const key = JSON.stringify(query);
  return useLoad(() => (query ? api.notifications(query) : Promise.resolve(null)), [key, changed]).data;
}

/** The All, Waiting on you and Unread tabs: the notifications the core keeps
 *  for each, with what waits on the person first on All. */
function Notifications({ tab }: { tab: Exclude<InboxTab, "history"> }) {
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
  const query = useDebounced(q, SEARCH_DELAY_MS);
  const list = (key: string) => (params.get(key) ?? "").split(",").filter(Boolean);
  const starred = params.get("starred") === "1";
  const [discarding, setDiscarding] = useState<Review | null>(null);
  // the rows unfolded to list their new events
  const [unfolded, setUnfolded] = useState<ReadonlySet<number>>(new Set());
  const fold = (n: Notification, open: boolean) =>
    setUnfolded((now) => {
      const next = new Set(now);
      if (open) next.add(n.id);
      else next.delete(n.id);
      return next;
    });
  const search = useRef<HTMLInputElement>(null);

  const page = pageOf(params.get("page"));
  const size = pageSizeOf(params.get("per"));
  const filters: NotificationQuery = {
    kind: list("kind"),
    scope: list("scope"),
    plugin: list("plugin"),
    q: query.trim() || undefined,
    done: params.get("done") === "1",
  };
  // All: what waits, whole, above a page of the updates; the other tabs: a
  // page of their own list
  const pagedTab: NotificationTab = tab === "all" ? "updates" : tab;
  const waiting = useNotifications(tab === "all" ? { ...filters, tab: "waiting", limit: WAITING_SHOWN } : null);
  const paged = useNotifications({ ...filters, tab: pagedTab, limit: size, offset: (page - 1) * size });
  const counts: InboxCounts | undefined = paged?.counts;

  // a starred review is the one filter the core does not apply
  const keep = useCallback(
    (n: Notification) => !starred || (n.subject.kind === "review" && !!n.subject.review.starred),
    [starred],
  );
  const waitingRows = useMemo(() => (waiting?.notifications ?? []).filter(keep), [waiting, keep]);
  const pagedRows = useMemo(() => (paged?.notifications ?? []).filter(keep), [paged, keep]);
  const rows = useMemo(() => [...waitingRows, ...pagedRows], [waitingRows, pagedRows]);
  const upTo = Math.max(waiting?.up_to ?? 0, paged?.up_to ?? 0);

  const pages = Math.max(1, Math.ceil((paged?.total ?? 0) / size));
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
  // the reviews in the order drawn, which the side panel walks
  const reviews = useMemo(() => rows.flatMap((n) => (n.subject.kind === "review" ? [n.subject.review] : [])), [rows]);
  const reviewsRef = useLatest(reviews);
  // where the open review was in the list when last seen there: once it has
  // ended, the list may have dropped it before the panel moves on
  const openAt = useRef(-1);
  const seenAt = reviews.findIndex((r) => r.id === openId);
  useLayoutEffect(() => {
    if (seenAt >= 0) openAt.current = seenAt;
  }, [seenAt]);
  /** The review after the one that just ended, or before it at the end of the list. */
  const openNext = useCallback(() => {
    const list = reviewsRef.current;
    const at = list.findIndex((r) => r.id === openId);
    const next = at >= 0 ? (list[at + 1] ?? list[at - 1]) : (list[openAt.current] ?? list[list.length - 1]);
    openInPanel(next && next.id !== openId ? next.id : null, { replace: true });
  }, [openId, openInPanel, reviewsRef]);
  const closePanel = useCallback(() => openInPanel(null), [openInPanel]);
  /** A plain click on a review's link in the panel mode opens it in the side panel. */
  const openFromLink = (event: MouseEvent, id: string) => {
    if (!panelMode || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    openInPanel(id, { replace: currentParams().has("open") });
  };

  const split = useRef<HTMLDivElement>(null);
  const setRead = (n: Notification, read: boolean) => void api.setNotificationRead(n.id, read).catch(() => {});
  const setDone = (n: Notification) => void api.setNotificationDone(n.id, true).catch(() => {});
  const readAll = () => void api.readNotifications({ ...filters, tab }, upTo).catch(() => {});
  /** Opening a notification other than a review marks it read; a review is
   *  read when the person opens it, which the core records. */
  const opened = (n: Notification) => {
    if (n.subject.kind !== "review" && !n.read) setRead(n, true);
  };

  // J, K and Enter over every row, what waits first; the side panel walks
  // the reviews alone
  const walked = useMemo(() => rows.filter((n) => n.subject.kind === "review"), [rows]);
  const openKey = openId
    ? (rows.find((n) => n.subject.kind === "review" && n.subject.review.id === openId)?.id.toString() ?? null)
    : null;
  const { focused, setFocused, hover } = useListKeys<Notification>({
    items: rows,
    walkItems: walked,
    altWalk: true,
    keyOf: (n) => String(n.id),
    openKey,
    onOpen: (n) => {
      opened(n);
      navigate(notificationPath(n, hrefFor));
    },
    onWalk: (n) => {
      if (n.subject.kind === "review") openInPanel(n.subject.review.id, { replace: true });
    },
    rowOf: (i) => document.querySelector<HTMLElement>(`.inbox-side [data-row="${i}"]`),
  });
  const focusedRow = rows[focused];
  const focusedReview = focusedRow?.subject.kind === "review" ? focusedRow.subject.review : null;
  useSlashSearch(search);

  useWindowKey((event) => {
    // the inbox's keys are not for a dialog in front of it
    if (modalOpen()) return;
    // the open review handles the other keys: the panel handles discard,
    // and the view may use Enter
    if (openId) return;
    if (hasMod(event) && !event.altKey && !event.shiftKey && (event.key === "d" || event.key === "D")) {
      // the key must not land in the reason field that opens
      event.preventDefault();
      if (!discarding && focusedReview && focusedRow?.waiting) setDiscarding(focusedReview);
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    // E marks the focused notification done, unless it waits
    if ((event.key === "e" || event.key === "E") && !event.shiftKey && !isTyping(event.target)) {
      if (focusedRow && !focusedRow.waiting) {
        event.preventDefault();
        setDone(focusedRow);
      }
      return;
    }
    // → unfolds the focused row's new events, and ← folds them
    if ((event.key === "ArrowRight" || event.key === "ArrowLeft") && !isTyping(event.target)) {
      if (focusedRow && unfolds(focusedRow)) {
        event.preventDefault();
        fold(focusedRow, event.key === "ArrowRight");
      }
      return;
    }
    // U marks the focused notification read or unread, and ⇧U the list read
    if ((event.key === "u" || event.key === "U") && !isTyping(event.target)) {
      if (event.shiftKey) {
        event.preventDefault();
        readAll();
      } else if (focusedRow) {
        event.preventDefault();
        setRead(focusedRow, !focusedRow.read);
      }
      return;
    }
    const el = event.target as HTMLElement | null;
    if (isTyping(el) && event.key === "Escape") {
      (el as HTMLElement).blur();
      return;
    }
    if (event.key === "Escape" && panelMode && new URLSearchParams(window.location.hash.split("?")[1]).has("open")) {
      openInPanel(null, { replace: true });
    }
  });

  // What a row does, the same functions from one render to the next, so a
  // memoized row draws again only when its own props change
  const latest = useLatest({
    hover: (here: number) => hover(here),
    open: (event: MouseEvent, id: string) => openFromLink(event, id),
    discard: (review: Review) => setDiscarding(review),
    opened,
    setRead,
    setDone,
    toggle: (n: Notification) => fold(n, !unfolded.has(n.id)),
  });
  const reviewActions = useMemo<InboxRowActions>(
    () => ({
      hover: (here) => latest.current.hover(here),
      open: (event, id) => latest.current.open(event, id),
      discard: (review) => latest.current.discard(review),
    }),
    [latest],
  );
  const notificationActions = useMemo<NotificationActions>(
    () => ({
      hover: (here) => latest.current.hover(here),
      open: (n) => latest.current.opened(n),
      setRead: (n, read) => latest.current.setRead(n, read),
      setDone: (n) => latest.current.setDone(n),
      toggle: (n) => latest.current.toggle(n),
    }),
    [latest],
  );

  /** A notification's row; `here` is its place in the order J and K walk. A
   *  review that waits keeps the review's own row, with its star and Discard. */
  const row = (n: Notification, here: number) =>
    n.waiting && n.subject.kind === "review" ? (
      <InboxRow
        key={n.id}
        review={n.subject.review}
        here={here}
        focused={here === focused}
        open={n.subject.review.id === openId}
        showRepo
        hrefFor={hrefFor}
        actions={reviewActions}
      />
    ) : (
      <Fragment key={n.id}>
        <NotificationRow
          notification={n}
          here={here}
          focused={here === focused}
          unfolded={unfolded.has(n.id) && unfolds(n)}
          href={notificationPath(n, hrefFor)}
          actions={notificationActions}
        />
        {unfolded.has(n.id) && unfolds(n) ? (
          <NotificationEvents notification={n} href={notificationPath(n, hrefFor)} onRead={() => setRead(n, true)} />
        ) : null}
      </Fragment>
    );

  const filtered = !!(q || list("kind").length || list("scope").length || list("plugin").length || starred);
  const loaded = !!paged && (tab !== "all" || !!waiting);
  const repos = useMemo(() => {
    const listing = live.projectsListing;
    if (!listing) return [];
    return [...listing.projects.flatMap((p) => p.repos.map((r) => r.repo)), ...listing.loose.map((r) => r.repo)];
  }, [live.projectsListing]);
  const clearFilters = () => {
    setQ("");
    updateParams(
      (next) => {
        const kept = next.get("tab");
        clearAll(next);
        if (kept) next.set("tab", kept);
      },
      { replace: true },
    );
  };

  return (
    <div className={`inbox ${openId ? "has-panel" : ""}`} ref={split}>
      <div className="inbox-side split-side">
        <InboxHead
          tab={tab}
          search={search}
          q={q}
          onSearch={setQ}
          projects={repos}
          unassigned
          plugins={[...live.plugins.keys()].sort()}
          onClear={() => setQ("")}
          counts={counts}
          onReadAll={(counts?.unread ?? 0) > 0 ? readAll : undefined}
        />
        <TelemetryNotice />
        {!loaded ? null : rows.length === 0 ? (
          <EmptyState
            title={
              filtered ? "No matching notifications" : tab === "waiting" ? "Nothing waits on you" : "All caught up"
            }
            icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <CheckCheck size={28} strokeWidth={1.5} />}
            actions={
              filtered ? (
                <button type="button" className="chrome-button" onClick={clearFilters}>
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
              : "Reviews, questions and what your agents do appear here."}
          </EmptyState>
        ) : tab === "all" ? (
          <>
            {waitingRows.length ? (
              <section className="inbox-section" aria-label="Waiting on you">
                <h2 className="inbox-section-title">
                  Waiting on you <span>{waitingRows.length}</span>
                </h2>
                {waitingRows.map((n, i) => row(n, i))}
              </section>
            ) : null}
            {pagedRows.length ? (
              <section className="inbox-section" aria-label="Updates">
                <h2 className="inbox-section-title">
                  Updates <span>{paged?.total ?? pagedRows.length}</span>
                  {counts && counts.unread > 0 ? (
                    <span className="inbox-section-unread">{counts.unread} unread</span>
                  ) : null}
                </h2>
                {pagedRows.map((n, i) => row(n, waitingRows.length + i))}
              </section>
            ) : null}
          </>
        ) : (
          <div className="inbox-list">{pagedRows.map((n, i) => row(n, i))}</div>
        )}
        <Pager
          label="Inbox"
          page={Math.min(page, pages)}
          size={size}
          total={paged?.total ?? 0}
          onPage={setPage}
          onSize={setSize}
        />
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
              <kbd>J</kbd> <kbd>K</kbd> move · <kbd>↵</kbd> open · <kbd>U</kbd> read · <kbd>E</kbd> done · <kbd>/</kbd>{" "}
              search
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

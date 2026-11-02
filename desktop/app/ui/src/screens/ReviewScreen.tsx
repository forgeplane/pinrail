import {
  Ban,
  ClipboardCheck,
  ClipboardX,
  Clock,
  Copy,
  Expand,
  ExternalLink,
  Maximize2,
  MessagesSquare,
  Mic,
  Minimize2,
  PanelRight,
  RefreshCw,
  Send,
  TriangleAlert,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams, type To } from "react-router";
import { ApiError, api } from "../api/client";
import { copyText } from "../lib/clipboard";
import type { Plugin, Review, Violation } from "../api/types";
import { usePluginBridge, type SubmitResult } from "../bridge/usePluginBridge";
import { AgentIcon } from "../components/AgentIcon";
import { AttachmentsChip } from "../components/AttachmentsChip";
import { OutcomeBadge, PluginBadge, SummaryCounts, outcomeOf } from "../components/Badges";
import { DiscardDialog } from "../components/DiscardDialog";
import { FindBar } from "../components/FindBar";
import { SourceBadge } from "../components/SourceBadge";
import { StarButton, useToggleStar } from "../components/StarButton";
import { Thread, sessionOf, sessionPath } from "../components/Thread";
import { WaitMark } from "../components/WaitMark";
import { LinkDialog, type LinkChoice } from "../components/LinkDialog";
import { allowedWithoutAsking, allowing, linkRequest, sourceOf, type LinkRequest } from "../lib/links";
import { openExternal, overlayTitleBar } from "../lib/native";
import { canDictate, useDictation } from "../lib/dictation";
import { Tooltip } from "../components/Tooltip";
import { MOD, SHIFT, hasMod, modalOpen } from "../lib/keys";
import { comboFromEvent, isShadowed } from "../lib/shortcuts";
import { age, stamp } from "../lib/format";
import { useLive } from "../state/live";
import { useToast } from "../state/toasts";
import { useSettings } from "../state/settings";
import { useTopBar } from "../state/topbar";
import type { FromSession } from "../lib/sessions";

const NOTE_PREFIX = "pinrail:draft:";

/** The review page, at /reviews/:id. */
export function ReviewScreen() {
  const { id = "" } = useParams();
  return <ReviewView id={id} />;
}

type ReviewViewProps = {
  id: string;
  /** shown in the inbox's side panel, beside the list, rather than on its own page */
  inPanel?: boolean;
  /** the link target for another round of the review */
  linkTo?: (id: string) => To;
  /** called when the review is decided or discarded here; the page returns to the inbox */
  onEnded?: () => void;
  /** closes the side panel */
  onClose?: () => void;
  /** shown beside its session's thread, which it then does not link to */
  besideThread?: boolean;
};

const pageOf = (id: string): To => `/reviews/${id}`;

/** A review: its strip, its plugin view, and the note sent with a decision. */
export function ReviewView({
  id,
  inPanel = false,
  linkTo = pageOf,
  onEnded,
  onClose,
  besideThread = false,
}: ReviewViewProps) {
  const live = useLive();
  const { settings: prefs, update: updatePrefs } = useSettings();
  const navigate = useNavigate();
  const toast = useToast();
  const [copied, setCopied] = useState<"done" | "failed" | null>(null);
  const [copiedId, setCopiedId] = useState<"done" | "failed" | null>(null);
  // the review as the core renders it in markdown, for a merge request or a thread
  const copyMarkdown = async () => {
    if (!id) return;
    try {
      await copyText(await api.reviewMarkdown(id));
      setCopied("done");
    } catch {
      setCopied("failed");
    }
    window.setTimeout(() => setCopied(null), 3500);
  };
  // the id alone, for a message to the agent or a command line
  const copyId = async () => {
    if (!id) return;
    try {
      await copyText(id);
      setCopiedId("done");
    } catch {
      setCopiedId("failed");
    }
    window.setTimeout(() => setCopiedId(null), 3500);
  };
  const location = useLocation();
  // the way back is where the review was opened from
  const opened = location.state as { from?: string; session?: FromSession["session"] } | null;
  const fromHistory = opened?.from === "history";
  const fromSession = opened?.from === "session" && opened.session ? opened.session : null;
  const back = fromHistory ? { to: "/?tab=history", label: "History" } : { to: "/", label: "Inbox" };
  // where a decision leads: what is still waiting, or the session it came from
  const ended = fromSession ? sessionPath(fromSession) : "/";
  const [review, setReview] = useState<Review | null>(null);
  // the review the shortcuts act on, read when a key is pressed
  const reviewRef = useRef(review);
  reviewRef.current = review;
  const toggleStar = useToggleStar();
  const [rounds, setRounds] = useState<Review[]>([]);
  // the whole review this round revises, with its payload, for the view: the
  // rounds list holds summaries only
  const [revised, setRevised] = useState<Review | null>(null);
  // the other rounds still waiting that have never been opened: what is new
  const [unopened, setUnopened] = useState<Set<string>>(new Set());
  // The review opened: the plugin and its view's URL, resolved together for
  // one plugin and the bundle the review renders with, which opening may
  // move it to; the installed bundle when it was opened; and why the
  // installed version does not take it, when it does not. The screen
  // outlives a change of review, so what was resolved for the last review
  // stays in state until the lookup for this one lands: it counts only
  // when it was resolved for the review on screen.
  const [resolved, setResolved] = useState<{
    key: string;
    plugin: Plugin | null;
    src: string | null;
    bundle: string | null;
    installed: string | null;
    refused: string | null;
  } | null>(null);
  // questions have no plugin: the thread shows them
  const pluginKey = review && review.kind !== "question" ? `${review.plugin}@${review.plugin_bundle}` : null;
  const current = resolved && resolved.key === pluginKey ? resolved : null;
  const plugin: Plugin | null | undefined = current ? current.plugin : undefined;
  const src = current?.src ?? null;
  // the plugin as installed now: another version, or a linked folder
  // changed since it was stored, may have come since the review was opened
  const installedNow = review ? live.plugins.get(review.plugin)?.install : undefined;
  const newerInstalled =
    !!current &&
    review?.status === "pending" &&
    !!installedNow &&
    (installedNow.folder_changed ||
      (installedNow.bundle !== current.installed && installedNow.bundle !== current.bundle));
  // opens the review again, on the version installed now when it takes it
  const reopen = () => setResolved(null);
  const [violations, setViolations] = useState<Violation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [discarding, setDiscarding] = useState(false);
  const [note, setNote] = useState("");
  const noteBox = useRef<HTMLTextAreaElement>(null);
  const [maximized, setMaximized] = useState(false);
  // the find bar, and a count that focuses it each time it is asked for
  const [finding, setFinding] = useState(false);
  const [findFocus, setFindFocus] = useState(0);
  const [findQuery, setFindQuery] = useState("");
  const frame = useRef<HTMLIFrameElement>(null);
  const noteRef = useRef(note);
  noteRef.current = note;

  // the review the route names now: a fetch for one clicked past lands late
  // and is dropped, so the screen never settles on a review nobody picked
  const wanted = useRef(id);
  wanted.current = id;
  /** Loads the review and its rounds; true when the review exists. */
  const load = useCallback(async (): Promise<boolean> => {
    try {
      const [r, rs] = await Promise.all([api.getReview(id), api.rounds(id).catch(() => [] as Review[])]);
      const before = r.revises ? await api.getReview(r.revises).catch(() => null) : null;
      const waiting = rs.filter((round) => round.id !== id && round.status === "pending");
      const opened = await Promise.all(
        waiting.map((round) =>
          api
            .events(round.id)
            .then((events) => events.some((e) => e.kind === "viewed"))
            .catch(() => true),
        ),
      );
      if (wanted.current !== id) return false;
      setReview(r);
      setRounds(rs);
      setRevised(before);
      setUnopened(new Set(waiting.filter((_, i) => !opened[i]).map((round) => round.id)));
      setError(null);
      return true;
    } catch (e) {
      if (wanted.current !== id) return false;
      setError(e instanceof ApiError ? e.message : "The server did not answer.");
      return false;
    }
  }, [id]);

  useEffect(() => {
    // marked viewed only once the server knows the review
    load().then((found) => {
      if (found) api.markViewed(id).catch(() => {});
    });
    try {
      setNote(sessionStorage.getItem(NOTE_PREFIX + id + ":note") ?? "");
    } catch {
      // no storage
    }
  }, [id, load]);

  // Every event about this review refreshes it, and so does a new round of
  // it: that event names the new review, revising this one or a later round.
  const roundIds = useRef(new Set<string>());
  roundIds.current = new Set([id, ...rounds.map((r) => r.id)]);
  useEffect(() => {
    const notice = live.lastNotice;
    if (!notice) return;
    const revises = notice.review?.revises;
    // a star on another round marks this one too
    const round = !!notice.review_id && roundIds.current.has(notice.review_id);
    if (notice.review_id === id || (revises && roundIds.current.has(revises)) || round) load();
  }, [live.lastNotice, id, load]);

  // The plugin the review renders with and the address of its view; null
  // when it is not there, and the review says so.
  useEffect(() => {
    if (!review || !pluginKey) return;
    if (resolved?.key === pluginKey) return;
    let cancelled = false;
    (async () => {
      try {
        // opened, the review may move to the version installed now
        const view = await api.reviewView(review.id);
        if (cancelled) return;
        const key = `${view.review.plugin}@${view.bundle}`;
        // the plugin and its URL arrive in one update, for the key they were looked up for
        setResolved({
          key,
          plugin: view.plugin.usable ? view.plugin : null,
          src: view.url,
          bundle: view.bundle,
          refused: view.refused,
          installed: view.installed,
        });
        if (view.review.plugin_bundle !== review.plugin_bundle) setReview(view.review);
      } catch {
        if (!cancelled)
          setResolved({ key: pluginKey, plugin: null, src: null, bundle: null, installed: null, refused: null });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [pluginKey, resolved]); // eslint-disable-line react-hooks/exhaustive-deps -- the key names the plugin and version; the review changes on every event

  const previous = useMemo(() => {
    if (!review?.revises) return null;
    if (revised?.id === review.revises) return revised;
    return rounds.find((r) => r.id === review.revises) ?? null;
  }, [review, rounds, revised]);
  const revisedBy = useMemo(() => rounds.find((r) => r.revises === review?.id) ?? null, [rounds, review]);
  // the newest round, when it is waiting and has never been opened
  const waiting = useMemo(() => {
    const last = rounds[rounds.length - 1];
    return last && unopened.has(last.id) ? { round: last, number: rounds.length } : null;
  }, [rounds, unopened]);

  const readonly = !review || review.status !== "pending";

  const onSubmit = useCallback(
    async (data: unknown): Promise<SubmitResult> => {
      try {
        const decided = await api.decide(id, data, noteRef.current, current?.bundle ?? undefined);
        try {
          sessionStorage.removeItem(NOTE_PREFIX + id + ":note");
        } catch {
          // ignore
        }
        // the person moved on while it was on its way: the screen is
        // another review's now, and the decision stays with its own
        if (wanted.current !== id) return { ok: true, decision: decided.decision! };
        // the review is done with: back to what is still waiting, once the
        // view has been told submitted, which the bridge sends on return
        toast("Decision recorded");
        setTimeout(() => (onEnded ? onEnded() : navigate(ended)), 0);
        return { ok: true, decision: decided.decision! };
      } catch (e) {
        if (wanted.current !== id) return { ok: false, violations: [] };
        if (e instanceof ApiError && e.kind === "invalid") {
          setViolations(e.violations);
          return { ok: false, violations: e.violations };
        }
        if (e instanceof ApiError && e.kind === "conflict") {
          // the review moved to another version since it was opened: it
          // opens again with that one, and the person decides there
          setFlash("This review was opened again with the plugin's version installed now. Nothing was decided.");
          load();
          reopen();
          return { ok: false, violations: [] };
        }
        setFlash(e instanceof Error ? e.message : "The decision was not recorded.");
        load();
        return { ok: false, violations: [] };
      }
    },
    [id, load, navigate, toast, current?.bundle, onEnded, ended],
  );

  // the plugin's own settings as they stand: its defaults under what was set
  const stored = plugin ? prefs.plugins[plugin.name] : undefined;
  const pluginSettings = useMemo(() => {
    if (!plugin?.settings_schema) return null;
    const out: Record<string, unknown> = {};
    for (const [key, property] of Object.entries(plugin.settings_schema.properties))
      out[key] = stored && key in stored ? stored[key] : property.default;
    return out;
  }, [plugin, stored]);
  const onSetSetting = useCallback(
    async (patch: Record<string, unknown>): Promise<Violation[]> => {
      if (!plugin) return [];
      try {
        await api.patchSettings({ plugins: { [plugin.name]: patch } });
        return [];
      } catch (e) {
        return e instanceof ApiError
          ? e.violations
          : [{ path: "", message: e instanceof Error ? e.message : "The setting was not kept" }];
      }
    },
    [plugin],
  );

  // A link the view asked to open: opened when its origin is allowed for this
  // plugin, and otherwise asked about, one request at a time; any that
  // arrive while the person is being asked are dropped.
  const [linkAsk, setLinkAsk] = useState<LinkRequest | null>(null);
  const asking = useRef(false);
  const prefsNow = useRef(prefs);
  prefsNow.current = prefs;
  const onOpen = useCallback(
    (url: string) => {
      if (!plugin || asking.current) return;
      const request = linkRequest(url);
      if (!request) return;
      if (
        allowedWithoutAsking(
          request,
          prefsNow.current.links[plugin.name],
          sourceOf(live.plugins.get(plugin.name) ?? plugin),
        )
      ) {
        openExternal(url);
        return;
      }
      asking.current = true;
      setLinkAsk(request);
    },
    [plugin, live.plugins],
  );
  const onLinkChoice = useCallback(
    (choice: LinkChoice) => {
      const request = linkAsk;
      asking.current = false;
      setLinkAsk(null);
      if (!request || !plugin || choice === "cancel") return;
      if (choice === "always" && request.origin) {
        updatePrefs({
          links: {
            [plugin.name]: allowing(
              prefsNow.current.links[plugin.name],
              sourceOf(live.plugins.get(plugin.name) ?? plugin),
              request.origin,
            ),
          },
        });
      }
      openExternal(request.url);
    },
    [linkAsk, plugin, updatePrefs, live.plugins],
  );

  const bridge = usePluginBridge({
    frame,
    reviewId: review?.id ?? null,
    review,
    previous: previous ? { ...previous } : null,
    readonly,
    src,
    connected: live.connected,
    onSubmit,
    settings: pluginSettings,
    onSetSetting,
    onOpen,
    onDictationError: (message) => toast(message, "danger"),
    canCopy: !!plugin?.capabilities?.includes("clipboard"),
    onCopied: (what) => toast("text" in what ? "Copied the text" : `Copied ${what.image}`),
  });

  // closing the find bar clears what it highlighted, and the keys go back
  // to the view
  const findInView = bridge.find;
  const closeFind = useCallback(() => {
    setFinding(false);
    findInView("");
    frame.current?.focus();
  }, [findInView]);

  // only a review that is still waiting can be discarded
  const pending = review?.status === "pending";

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // a dialog in front of the review has the keys
      if (modalOpen()) return;
      if (event.key === "Escape" && maximized) {
        setMaximized(false);
        return;
      }
      // Escape closes the side panel. A view that handles the key keeps it,
      // and forwards only an Escape it did not handle
      if (event.key === "Escape" && onClose && !finding && !modalOpen()) {
        const target = event.target as HTMLElement | null;
        if (!target || (target.tagName !== "INPUT" && target.tagName !== "TEXTAREA")) {
          onClose();
          return;
        }
      }
      if (hasMod(event) && event.shiftKey && !event.altKey && (event.key === "s" || event.key === "S")) {
        event.preventDefault();
        const shown = reviewRef.current;
        if (shown && shown.kind !== "question") toggleStar(shown);
        return;
      }
      if (hasMod(event) && event.shiftKey && !event.altKey && (event.key === "m" || event.key === "M")) {
        event.preventDefault();
        if (plugin) setMaximized((m) => !m);
        return;
      }
      if (hasMod(event) && !event.shiftKey && !event.altKey && (event.key === "d" || event.key === "D")) {
        // the key must not land in the reason field that opens
        event.preventDefault();
        if (pending) setDiscarding(true);
        return;
      }
      if (hasMod(event) && !event.shiftKey && !event.altKey && (event.key === "f" || event.key === "F")) {
        event.preventDefault();
        if (plugin) {
          setFinding(true);
          setFindFocus((n) => n + 1);
        }
        return;
      }
      const el = event.target as HTMLElement | null;
      if (
        el &&
        (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
      )
        return;
      // [ and ] alone move between rounds; with ⌘ or Ctrl they are Back and
      // Forward, which the layout handles
      const plain = !event.metaKey && !event.ctrlKey && !event.altKey;
      if (plain && event.key === "[" && previous) navigate(linkTo(previous.id));
      if (plain && event.key === "]" && revisedBy) navigate(linkTo(revisedBy.id));
      // one of the plugin's declared keys, pressed with the shell in focus:
      // it goes to the view as if typed there
      const combo = comboFromEvent(event);
      if (combo && !isShadowed(combo) && plugin?.shortcuts?.some((s) => s.keys === combo)) {
        event.preventDefault();
        // focus the frame before forwarding the shortcut, so a field the
        // shortcut opens, such as a code review's note on c, accepts typing at once
        frame.current?.focus();
        frame.current?.contentWindow?.postMessage(
          {
            pinrail: 1,
            type: "key",
            key: event.key,
            code: event.code,
            metaKey: event.metaKey,
            ctrlKey: event.ctrlKey,
            altKey: event.altKey,
            shiftKey: event.shiftKey,
          },
          "*",
        );
      }
    };
    const onCommand = (event: Event) => {
      if ((event as CustomEvent<string>).detail === "maximize-view" && plugin) setMaximized((m) => !m);
    };
    // the palette's "Discard this review"
    const onDiscard = () => setDiscarding(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:command", onCommand);
    window.addEventListener("pinrail:discard", onDiscard);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:command", onCommand);
      window.removeEventListener("pinrail:discard", onDiscard);
    };
  }, [previous, revisedBy, navigate, maximized, plugin, pending, onClose, finding, linkTo, toggleStar]);

  // the view leaves with the review
  // what was said and opened for the last review stays with it: the screen
  // outlives a change of review
  useEffect(() => {
    setMaximized(false);
    setFinding(false);
    setFlash(null);
    setViolations([]);
    setDiscarding(false);
    setCopied(null);
    setCopiedId(null);
  }, [id]);

  // the bar: where this came from, the title, its origin link. The review's
  // outcome leads the page's own strip, and what can be done with it ends it.
  const originUrl = review?.origin.url ?? null;
  const crumb = useMemo(
    () =>
      review ? (
        // it stretches across the bar, so its own empty space moves the window
        <span className="crumb" data-tauri-drag-region>
          {fromSession ? (
            <>
              <Link to="/sessions" className="crumb-root">
                Sessions
              </Link>
              <span className="crumb-sep">›</span>
              <Link
                to={sessionPath(fromSession)}
                className="crumb-root crumb-session"
                title={fromSession.title ?? fromSession.agent}
              >
                {fromSession.title ?? fromSession.agent}
              </Link>
            </>
          ) : (
            <Link to={back.to} state={location.state} className="crumb-root">
              {back.label}
            </Link>
          )}
          <span className="crumb-sep">›</span>
          <span className="crumb-title copyable" title={review.title} data-testid="review-title">
            {review.title}
          </span>
          {originUrl ? (
            <Tooltip label={`Open ${review.origin.ref ?? "the origin"} in the browser`}>
              <a
                href={originUrl}
                target="_blank"
                rel="noreferrer"
                className="bar-button crumb-link"
                aria-label="Open the origin"
              >
                <ExternalLink size={13} />
              </a>
            </Tooltip>
          ) : null}
        </span>
      ) : null,
    [review?.title, review?.origin.ref, originUrl, back.to, back.label, location.state, fromSession], // eslint-disable-line react-hooks/exhaustive-deps -- the fields the crumb shows, not the whole review
  );
  // a page opened from the inbox can return to its side panel
  const showInPanel = !inPanel && !fromHistory && !fromSession && prefs.inbox.open_in === "panel" && pending;
  const actions = useMemo(
    () =>
      review ? (
        <>
          {review.status === "pending" ? (
            <Tooltip label="Discard: the agent is told to stop" keys={[MOD, "D"]} side="bottom">
              <button
                type="button"
                className="bar-button"
                onClick={() => setDiscarding(true)}
                aria-label="Discard this review"
              >
                <Ban size={15} />
              </button>
            </Tooltip>
          ) : null}
          <Tooltip
            label={copied === "done" ? "Copied" : copied === "failed" ? "Could not copy" : "Copy decision as markdown"}
            side="bottom"
          >
            <button
              type="button"
              className={`bar-button copy-button ${copied === "done" ? "ok" : copied === "failed" ? "danger" : ""}`}
              onClick={copyMarkdown}
              aria-label="Copy the decision as markdown"
            >
              {copied === "done" ? (
                <ClipboardCheck size={15} className="copy-done" />
              ) : copied === "failed" ? (
                <ClipboardX size={15} />
              ) : (
                <Copy size={15} />
              )}
            </button>
          </Tooltip>
          {review.kind === "question" ? null : <StarButton review={review} />}
          {besideThread ? null : <SessionLink review={review} />}
          {plugin ? (
            <Tooltip label="Maximize the view" keys={[MOD, SHIFT, "M"]} side="bottom">
              <button
                type="button"
                className="bar-button"
                onClick={() => setMaximized(true)}
                aria-label="Maximize the view"
              >
                <Maximize2 size={15} />
              </button>
            </Tooltip>
          ) : null}
          {inPanel ? (
            <Tooltip label="Open as a page" side="bottom">
              <button
                type="button"
                className="bar-button"
                onClick={() => navigate(`/reviews/${id}`)}
                aria-label="Open as a page"
              >
                <Expand size={15} />
              </button>
            </Tooltip>
          ) : showInPanel ? (
            <Tooltip label="Show in the inbox's panel" side="bottom">
              <button
                type="button"
                className="bar-button"
                onClick={() => navigate(`/?open=${encodeURIComponent(id)}`)}
                aria-label="Show in the inbox's panel"
              >
                <PanelRight size={15} />
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
        </>
      ) : null,
    // the id too: the screen outlives a change of review, and Copy acts on it
    // eslint-disable-next-line react-hooks/exhaustive-deps -- copyMarkdown reads only the id
    [
      id,
      review?.status,
      review?.requested_by,
      review?.session,
      review?.starred,
      review?.starred_at,
      plugin,
      copied,
      inPanel,
      showInPanel,
      onClose,
      besideThread,
    ],
  );
  const forPlugin = useMemo(
    () =>
      plugin
        ? {
            name: plugin.name,
            title: plugin.title || plugin.name,
            icon: plugin.icon,
            shortcuts: plugin.shortcuts ?? [],
          }
        : undefined,
    [plugin],
  );
  // in the side panel, the inbox shows its own bar and title: the panel
  // gives only its plugin, whose keys the shortcuts dialog lists
  const topbar = useMemo(
    () => (review ? (inPanel ? { plugin: forPlugin } : { crumb, plugin: forPlugin }) : null),
    [review, crumb, forPlugin, inPanel],
  );
  useTopBar(topbar);

  const onNote = (value: string) => {
    setNote(value);
    try {
      sessionStorage.setItem(NOTE_PREFIX + id + ":note", value);
    } catch {
      // ignore
    }
  };
  // dictation writes recognized text into the note, and the field grows with it
  const dictation = useDictation(onNote, (message) => toast(message, "danger"));
  const listening = dictation.listening;
  // Esc stops the active dictation, in the note or the view, before other
  // handlers see it. The panel stays open, and the next Esc goes to the screen
  const viewDictating = bridge.dictating;
  const stopViewDictation = bridge.stopDictation;
  const stopNoteDictation = dictation.stop;
  useEffect(() => {
    if (!listening && !viewDictating) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing || modalOpen()) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      if (listening) stopNoteDictation();
      if (viewDictating) stopViewDictation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening, viewDictating, stopViewDictation, stopNoteDictation]);
  useEffect(() => {
    const el = noteBox.current;
    if (!el || !dictation.listening) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [note, dictation.listening]);

  if (error) {
    return (
      <div className="review-page">
        <p className="notice notice-danger">{error}</p>
      </div>
    );
  }
  if (!review) return <div className="review-page" />;
  if (review.kind === "question")
    return (
      <div className={`review-page thread-page ${inPanel ? "in-panel" : ""}`}>
        <Thread key={review.id} focus={review} inPanel={inPanel} onClose={onClose} />
      </div>
    );

  const origin = review.origin;
  const originText = [origin.repo, origin.workflow].filter(Boolean).join(" / ");

  return (
    <div className={`review-page ${inPanel ? "in-panel" : ""}`}>
      {inPanel ? (
        <div className="panel-head" data-testid="panel-head">
          <span className="panel-title copyable" title={review.title} data-testid="review-title">
            {review.title}
          </span>
          {originUrl ? (
            <Tooltip label={`Open ${review.origin.ref ?? "the origin"} in the browser`}>
              <a
                href={originUrl}
                target="_blank"
                rel="noreferrer"
                className="bar-button crumb-link"
                aria-label="Open the origin"
              >
                <ExternalLink size={13} />
              </a>
            </Tooltip>
          ) : null}
          <span className="strip-actions">{actions}</span>
        </div>
      ) : null}
      <div className="review-strip" data-testid="review-strip">
        <div className="strip-info">
          <OutcomeBadge review={review} />
          {/* what it asks while it waits, what was decided once it is */}
          <SummaryCounts summary={review.status === "pending" ? review.summary : review.decision?.summary} />
          <PluginBadge name={review.plugin} version={review.plugin_version} />
          {review.attachments?.length ? (
            <AttachmentsChip reviewId={review.id} attachments={review.attachments} />
          ) : null}
          {originText ? (
            <span title={origin.folder ?? undefined}>
              {originText}
              {origin.ref ? <span className="mono faint"> #{origin.ref}</span> : null}
            </span>
          ) : null}
          {review.requested_by ? (
            <span className="with-icon">
              <AgentIcon requestedBy={review.requested_by} /> {review.requested_by}
            </span>
          ) : null}
          <span title={stamp(review.created_at)}>{age(review.created_at)} ago</span>
          {review.expires_at ? (
            <span className="with-icon" title={stamp(review.expires_at)}>
              <Clock size={13} /> expires {stamp(review.expires_at)}
            </span>
          ) : null}
          <span className="strip-id">
            <span className="mono faint copyable" data-testid="review-id">
              {review.id}
            </span>
            <Tooltip
              label={copiedId === "done" ? "Copied" : copiedId === "failed" ? "Could not copy" : "Copy the id"}
              side="bottom"
            >
              <button
                type="button"
                className={`id-copy ${copiedId === "done" ? "ok" : copiedId === "failed" ? "danger" : ""}`}
                onClick={copyId}
                aria-label="Copy the review id"
              >
                {copiedId === "done" ? (
                  <ClipboardCheck size={14} />
                ) : copiedId === "failed" ? (
                  <ClipboardX size={14} />
                ) : (
                  <Copy size={14} />
                )}
              </button>
            </Tooltip>
          </span>
          <span className="strip-spacer" />
          {rounds.length > 1 ? (
            <span className="rounds" role="navigation" aria-label="Rounds">
              <span className="rounds-cap">rounds</span>
              {rounds.map((r, i) => {
                // each round in the colour of its outcome, as the status badge shows it
                const outcome = outcomeOf(r.id === review.id ? review : r);
                const current = r.id === review.id;
                return (
                  <Tooltip key={r.id} label={`${r.title} · ${outcome.label}`} side="bottom">
                    <Link
                      to={linkTo(r.id)}
                      state={inPanel ? undefined : location.state}
                      className={`round-pill round-${outcome.tone}${current ? " is-current" : unopened.has(r.id) ? " is-waiting" : ""}`}
                      aria-current={current ? "page" : undefined}
                    >
                      <span>{i + 1}</span>
                    </Link>
                  </Tooltip>
                );
              })}
            </span>
          ) : null}
        </div>
        {inPanel ? null : <span className="strip-actions">{actions}</span>}
      </div>

      {flash ? <p className="notice">{flash}</p> : null}
      {newerInstalled ? (
        <div className="notice notice-warning" role="status">
          <TriangleAlert size={16} aria-hidden />
          <span>The plugin changed since this review was opened. Reload to see the review with the new version.</span>
          <button type="button" className="chrome-button" onClick={reopen}>
            <RefreshCw size={13} /> Reload
          </button>
        </div>
      ) : null}
      {current?.refused && review.status === "pending" ? (
        <p className="notice">
          The installed version of this plugin does not take this review, so it opens with the version it was submitted
          to. {current.refused}
        </p>
      ) : null}
      {review.status === "withdrawn" ? (
        <p className="notice">
          {review.withdrawn_by ? (
            <>
              <span className="text">{review.withdrawn_by}</span> withdrew this review
            </>
          ) : (
            "This review was withdrawn"
          )}{" "}
          {age(review.withdrawn_at)} ago
          {review.withdrawn_reason ? `: ${review.withdrawn_reason}.` : "."} Nothing was decided.{" "}
          <SourceBadge
            review={review}
            made={{ via: review.withdrawn_via, session: review.withdrawn_session }}
            command="withdraw"
          />
        </p>
      ) : null}
      {waiting ? (
        <p className="notice notice-round">
          Round {waiting.number} is waiting for you.{" "}
          <Link to={linkTo(waiting.round.id)} state={inPanel ? undefined : location.state}>
            Open it
          </Link>
        </p>
      ) : null}
      {review.status === "expired" ? (
        <p className="notice notice-danger">This review expired at {stamp(review.expires_at)} without a decision.</p>
      ) : null}
      {review.status === "discarded" ? (
        <p className="notice">
          {review.discarded_by ? (
            <>
              <span className="text">{review.discarded_by}</span> discarded this review
            </>
          ) : (
            "This review was discarded"
          )}{" "}
          {age(review.discarded_at)} ago
          {review.discarded_reason ? `: ${review.discarded_reason}` : "."} The agent was told to stop; nothing was
          decided.{" "}
          <SourceBadge
            review={review}
            made={{ via: review.discarded_via, session: review.discarded_session }}
            command="discard"
          />
        </p>
      ) : null}
      {linkAsk && plugin ? (
        <LinkDialog plugin={plugin.title || plugin.name} request={linkAsk} onClose={onLinkChoice} />
      ) : null}
      {discarding && review.status === "pending" ? (
        <DiscardDialog
          review={review}
          onClose={() => setDiscarding(false)}
          onDone={() => {
            // the review has ended: back to what is still waiting
            setDiscarding(false);
            toast("Discarded. The agent was told to stop.");
            if (onEnded) onEnded();
            else navigate(ended);
          }}
        />
      ) : null}
      {review.decision ? (
        <section className="decision-box">
          <div className="dim">
            {review.decision.decided_by ? (
              <>
                Decided by <span className="text">{review.decision.decided_by}</span>
              </>
            ) : (
              "Decided"
            )}{" "}
            <span title={stamp(review.decision.decided_at)}>{age(review.decision.decided_at)} ago</span>{" "}
            <SourceBadge review={review} made={review.decision} command="decide" />
          </div>
          {review.agent_note ? (
            <div className="agent-note">
              <div className="label">note to the agent</div>
              <p>{review.agent_note}</p>
            </div>
          ) : null}
          <details>
            <summary>decision data</summary>
            <pre>{JSON.stringify(review.decision.data)}</pre>
          </details>
        </section>
      ) : null}
      {plugin === null ? (
        <div className="notice notice-danger plugin-missing">
          <p>
            <b>
              {review.plugin} {review.plugin_version}
            </b>{" "}
            is not installed, so this review has no view. What was decided is still on record.
          </p>
          <button
            type="button"
            className="chrome-button"
            onClick={() => navigate("/", { state: { settings: "plugins" } })}
          >
            Open Plugins…
          </button>
        </div>
      ) : null}

      {plugin ? (
        <div
          className={`plugin-frame-wrap ${maximized ? "is-maximized" : ""} ${overlayTitleBar ? "has-overlay-bar" : ""}`}
        >
          {maximized ? (
            <div className="frame-bar" data-tauri-drag-region>
              <span className="frame-bar-title copyable">{review.title}</span>
              <Tooltip label="Restore the view" keys={[MOD, SHIFT, "M"]} side="bottom">
                <button
                  type="button"
                  className="bar-button"
                  onClick={() => setMaximized(false)}
                  aria-label="Restore the view"
                >
                  <Minimize2 size={15} />
                </button>
              </Tooltip>
            </div>
          ) : null}
          {bridge.left ? (
            <div className="notice notice-danger view-left" role="alert">
              <span>This plugin's view went to another page, so Pinrail stopped exchanging messages with it.</span>
              <button type="button" className="chrome-button" onClick={bridge.reload}>
                Reload the view
              </button>
            </div>
          ) : !bridge.loaded ? (
            <div className="plugin-loading" role="status">
              <WaitMark />
              <span>Loading the view…</span>
            </div>
          ) : null}
          {finding && bridge.loaded && !bridge.left ? (
            <FindBar
              query={findQuery}
              onQuery={setFindQuery}
              found={bridge.found}
              onFind={bridge.find}
              onClose={closeFind}
              focus={findFocus}
            />
          ) : null}
          <iframe
            key={`${review.id}:${bridge.reloads}`}
            ref={frame}
            id="plugin-frame"
            sandbox="allow-scripts"
            referrerPolicy="no-referrer"
            title={review.title}
            className="plugin-frame"
            style={{ visibility: bridge.left ? "hidden" : undefined }}
          />
        </div>
      ) : null}

      {violations.length > 0 ? (
        <ul className="violations" aria-label="Problems with the decision">
          {violations.map((v, i) => (
            <li key={i}>
              <span className="mono">{v.path || "/"}</span>: {v.message}
            </li>
          ))}
        </ul>
      ) : null}

      {plugin && !readonly ? (
        <div className={`composer ${bridge.submitting ? "is-busy" : ""}`}>
          <textarea
            ref={noteBox}
            rows={1}
            value={note}
            aria-label="Note to the agent"
            placeholder="Add a note for the agent…"
            onChange={(e) => onNote(e.target.value)}
            onInput={(e) => {
              const el = e.currentTarget;
              el.style.height = "auto";
              el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
            }}
          />
          <div className="composer-bar">
            {bridge.dictating ? (
              // the app runs speech recognition for the view and shows that it is listening
              <span className="composer-hint composer-listening" role="status">
                <span className="listening-dot" aria-hidden="true" />
                Listening for the view
                <span aria-hidden="true">·</span>
                <button type="button" className="listening-stop" onClick={bridge.stopDictation}>
                  Stop
                </button>
              </span>
            ) : (
              <span className="composer-hint">
                {live.connected
                  ? "The note travels with your decision"
                  : "Reconnecting — hand-over resumes when the server is back"}
              </span>
            )}
            {canDictate ? (
              <Tooltip label={listening ? "Stop dictating" : "Dictate the note"} side="top">
                <button
                  type="button"
                  className={`bar-button composer-dictate ${listening ? "is-listening" : ""}`}
                  aria-label={listening ? "Stop dictating" : "Dictate the note"}
                  aria-pressed={listening}
                  onClick={() => {
                    if (dictation.listening) dictation.stop();
                    else {
                      // only one dictation runs at a time, so stop the view's first
                      if (bridge.dictating) bridge.stopDictation();
                      dictation.start(note);
                    }
                    noteBox.current?.focus();
                  }}
                >
                  <Mic size={15} />
                </button>
              </Tooltip>
            ) : null}
            <Tooltip
              label={live.connected ? "Hand over to the agent" : "Reconnect to hand over"}
              keys={[MOD, "Enter"]}
              side="top"
            >
              <button
                type="button"
                className="handover-button with-icon"
                data-testid="handover"
                disabled={bridge.submitting || !live.connected}
                onClick={bridge.collect}
              >
                {bridge.handoverLabel}
                <Send size={13} />
                <span className="handover-keys" aria-hidden="true">
                  <kbd>{MOD}</kbd>
                  <kbd>↵</kbd>
                </span>
              </button>
            </Tooltip>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** A link to the thread of the session the review was sent from, if the
 *  agent said which. */
function SessionLink({ review }: { review: Review }) {
  const session = sessionOf(review);
  if (!session) return null;
  return (
    <Tooltip label="Open the session's thread" side="bottom">
      <Link to={sessionPath(session)} className="bar-button" aria-label="Open the session's thread">
        <MessagesSquare size={15} />
      </Link>
    </Tooltip>
  );
}

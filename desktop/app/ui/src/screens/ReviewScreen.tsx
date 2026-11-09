import { Clock, Mic, Minimize2, RefreshCw, Send, TriangleAlert } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams, type To } from "react-router";
import { ApiError, api } from "../api/client";
import type { Review, Violation } from "../api/types";
import { usePluginBridge, type SubmitResult } from "../bridge/usePluginBridge";
import { AgentIcon } from "../components/AgentIcon";
import { AttachmentsChip } from "../components/AttachmentsChip";
import { OutcomeBadge, PluginBadge, SummaryCounts, outcomeOf } from "../components/Badges";
import { DiscardDialog } from "../components/DiscardDialog";
import { DeleteReviewDialog } from "../components/DeleteReviewDialog";
import { FindBar, useFindBar } from "../components/FindBar";
import { SourceBadge } from "../components/SourceBadge";
import { useToggleStar } from "../components/StarButton";
import { ReviewItem } from "../components/ReviewItem";
import { Thread, sessionPath } from "../components/Thread";
import { WaitMark } from "../components/WaitMark";
import { LinkDialog } from "../components/LinkDialog";
import { overlayTitleBar } from "../lib/native";
import { canDictate, useDictation } from "../lib/dictation";
import { Tooltip } from "../components/Tooltip";
import { MOD, SHIFT, hasMod, modalOpen, isTyping } from "../lib/keys";
import { comboFromEvent, isShadowed } from "../lib/shortcuts";
import { age, stamp } from "../lib/format";
import { useConnected } from "../state/live";
import { useToast } from "../state/toasts";
import { useSettings } from "../state/settings";
import { useTopBar } from "../state/topbar";
import type { FromSession } from "../lib/sessions";
import { usePageRepo } from "../lib/pageRepo";
import { CopyButton } from "../components/CopyButton";
import { useReviewRounds } from "./review/useReviewRounds";
import { usePluginView } from "./review/usePluginView";
import { useLinkRequests } from "./review/useLinkRequests";
import { OriginLink, ReviewActions, ReviewCrumb } from "./review/ReviewBar";

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

/** A review: its strip, its plugin view, and the note sent with a decision.
 *  The review on screen stays until the next one the route names has
 *  loaded; what was said and opened for it goes with it. */
export const ReviewView = memo(function ReviewView(props: ReviewViewProps) {
  const rounds = useReviewRounds(props.id);
  const view = usePluginView(rounds.review, rounds.setReview);
  usePageRepo(rounds.review?.origin.repo);
  if (rounds.error) {
    return (
      <div className="review-page">
        <p className="notice notice-danger">{rounds.error}</p>
      </div>
    );
  }
  if (!rounds.review) return <div className="review-page" />;
  return <ReviewBody key={rounds.review.id} {...props} review={rounds.review} rounds={rounds} view={view} />;
});

/** The review on screen, with the state that belongs to it alone. */
function ReviewBody({
  review,
  rounds: { rounds, unopened, previous, revisedBy, waiting, load, wanted },
  view: { plugin, src, bundle, refused, newerInstalled, reopen },
  inPanel = false,
  linkTo = pageOf,
  onEnded,
  onClose,
  besideThread = false,
}: ReviewViewProps & {
  review: Review;
  rounds: ReturnType<typeof useReviewRounds>;
  view: ReturnType<typeof usePluginView>;
}) {
  // the review on screen, which is the route's once it has loaded
  const id = review.id;
  const connected = useConnected();
  const { settings: prefs } = useSettings();
  const navigate = useNavigate();
  const toast = useToast();
  const location = useLocation();
  // the way back is where the review was opened from
  const opened = location.state as { from?: string; session?: FromSession["session"] } | null;
  const fromHistory = opened?.from === "history";
  const fromSession = opened?.from === "session" && opened.session ? opened.session : null;
  const back = fromHistory ? { to: "/?tab=history", label: "History" } : { to: "/", label: "Inbox" };
  // where a decision leads: what is still waiting, or the session it came from
  const ended = fromSession ? sessionPath(fromSession) : "/";
  const toggleStar = useToggleStar();
  const [violations, setViolations] = useState<Violation[]>([]);
  const [flash, setFlash] = useState<string | null>(null);
  const [discarding, setDiscarding] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [note, setNote] = useState(() => {
    try {
      return sessionStorage.getItem(NOTE_PREFIX + id + ":note") ?? "";
    } catch {
      return "";
    }
  });
  const noteBox = useRef<HTMLTextAreaElement>(null);
  const [maximized, setMaximized] = useState(false);
  // the find bar, and a count that focuses it each time it is asked for
  const {
    open: finding,
    focus: findFocus,
    query: findQuery,
    setQuery: setFindQuery,
    show: showFind,
    hide: hideFind,
  } = useFindBar();
  const frame = useRef<HTMLIFrameElement>(null);
  const noteRef = useRef(note);
  useEffect(() => {
    noteRef.current = note;
  }, [note]);

  const readonly = review.status !== "pending";
  // only a review that is still waiting can be discarded
  const pending = review.status === "pending";

  const onSubmit = useCallback(
    async (data: unknown): Promise<SubmitResult> => {
      try {
        const decided = await api.decide(id, data, noteRef.current, bundle ?? undefined);
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
    [id, load, navigate, toast, bundle, onEnded, ended, wanted, reopen],
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

  const links = useLinkRequests(plugin);

  const bridge = usePluginBridge({
    frame,
    reviewId: id,
    review,
    previous: previous ? { ...previous } : null,
    readonly,
    src,
    connected: connected,
    onSubmit,
    settings: pluginSettings,
    onSetSetting,
    onOpen: links.onOpen,
    onDictationError: (message) => toast(message, "danger"),
    canCopy: !!plugin?.capabilities?.includes("clipboard"),
    onCopied: (what) => toast("text" in what ? "Copied the text" : `Copied ${what.image}`),
  });

  // closing the find bar clears what it highlighted, and the keys go back
  // to the view
  const findInView = bridge.find;
  const closeFind = useCallback(() => {
    hideFind();
    findInView("");
    frame.current?.focus();
  }, [findInView, hideFind]);

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
        if (review.kind !== "question") toggleStar(review);
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
          showFind();
        }
        return;
      }
      const el = event.target as HTMLElement | null;
      if (isTyping(el)) return;
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
    // and its "Delete this review…"
    const onDelete = () => setDeleting(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:command", onCommand);
    window.addEventListener("pinrail:discard", onDiscard);
    window.addEventListener("pinrail:delete-review", onDelete);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:command", onCommand);
      window.removeEventListener("pinrail:discard", onDiscard);
      window.removeEventListener("pinrail:delete-review", onDelete);
    };
  }, [
    review,
    previous,
    revisedBy,
    navigate,
    maximized,
    plugin,
    pending,
    onClose,
    finding,
    linkTo,
    toggleStar,
    showFind,
  ]);

  // the bar: where this came from, the title, its origin link. The review's
  // outcome leads the page's own strip, and what can be done with it ends it.
  const originUrl = review.origin.url ?? null;
  const crumb = useMemo(
    () => (
      <ReviewCrumb
        title={review.title}
        originRef={review.origin.ref ?? null}
        originUrl={originUrl}
        back={back}
        backState={location.state}
        fromSession={fromSession}
        sessionsShown={prefs.labs.sessions}
      />
    ),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- back is made from fromHistory, which location.state holds
    [review.title, review.origin.ref, originUrl, location.state, fromSession, prefs.labs.sessions],
  );
  // a page opened from the inbox can return to its side panel
  const showInPanel = !inPanel && !fromHistory && !fromSession && prefs.inbox.open_in === "panel" && pending;
  const actions = (
    <ReviewActions
      review={review}
      hasView={!!plugin}
      inPanel={inPanel}
      showInPanel={showInPanel}
      sessionLink={!besideThread && prefs.labs.sessions}
      onDiscard={() => setDiscarding(true)}
      onDelete={() => setDeleting(true)}
      onMaximize={() => setMaximized(true)}
      onOpenPage={() => navigate(`/reviews/${id}`)}
      onShowInPanel={() => navigate(`/?open=${encodeURIComponent(id)}`)}
      onClose={onClose}
    />
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
    () => (inPanel ? { plugin: forPlugin } : { crumb, plugin: forPlugin }),
    [crumb, forPlugin, inPanel],
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
          {originUrl ? <OriginLink url={originUrl} originRef={review.origin.ref ?? null} /> : null}
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
          <ReviewItem review={review} onChanged={() => void load()} />
          {review.expires_at ? (
            <span className="with-icon" title={stamp(review.expires_at)}>
              <Clock size={13} /> expires {stamp(review.expires_at)}
            </span>
          ) : null}
          <span className="strip-id">
            <span className="mono faint copyable" data-testid="review-id">
              {review.id}
            </span>
            <CopyButton key={review.id} text={review.id} label="Copy the id" ariaLabel="Copy the review id" />
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
      {refused && review.status === "pending" ? (
        <p className="notice">
          The installed version of this plugin does not take this review, so it opens with the version it was submitted
          to. {refused}
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
      {links.asked && plugin ? (
        <LinkDialog plugin={plugin.title || plugin.name} request={links.asked} onClose={links.onChoice} />
      ) : null}
      {deleting ? (
        <DeleteReviewDialog
          review={review}
          onClose={() => setDeleting(false)}
          onDone={() => {
            setDeleting(false);
            toast("Deleted");
            // nothing is left to show: back to the list it came from
            if (onClose) onClose();
            else navigate(fromSession ? ended : back.to);
          }}
        />
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
                {connected
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
              label={connected ? "Hand over to the agent" : "Reconnect to hand over"}
              keys={[MOD, "Enter"]}
              side="top"
            >
              <button
                type="button"
                className="handover-button with-icon"
                data-testid="handover"
                disabled={bridge.submitting || !connected}
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

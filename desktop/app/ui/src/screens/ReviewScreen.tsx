import { Ban, Bot, Clock, ExternalLink, Maximize2, Minimize2, Send } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";
import { ApiError, api } from "../api/client";
import type { Plugin, Review, Violation } from "../api/types";
import { usePluginBridge, type SubmitResult } from "../bridge/usePluginBridge";
import { OutcomeBadge, PluginBadge } from "../components/Badges";
import { DiscardDialog } from "../components/DiscardDialog";
import { Tooltip } from "../components/Tooltip";
import { MOD, hasMod } from "../lib/keys";
import { comboFromEvent, isShadowed } from "../lib/shortcuts";
import { overlayTitleBar } from "../lib/native";
import { age, stamp } from "../lib/format";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useTopBar } from "../state/topbar";

const NOTE_PREFIX = "wicket:draft:";

export function ReviewScreen() {
  const { id = "" } = useParams();
  const live = useLive();
  const { settings: prefs } = useSettings();
  const navigate = useNavigate();
  const location = useLocation();
  // the way back is where the review was opened from
  const fromHistory = (location.state as { from?: string } | null)?.from === "history";
  const back = fromHistory ? { to: "/history", label: "History" } : { to: "/", label: "Inbox" };
  const [review, setReview] = useState<Review | null>(null);
  const [rounds, setRounds] = useState<Review[]>([]);
  const [plugin, setPlugin] = useState<Plugin | null | undefined>(undefined);
  const [src, setSrc] = useState<string | null>(null);
  const [violations, setViolations] = useState<Violation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [discarding, setDiscarding] = useState(false);
  const [note, setNote] = useState("");
  const [maximized, setMaximized] = useState(false);
  const frame = useRef<HTMLIFrameElement>(null);
  const noteRef = useRef(note);
  noteRef.current = note;

  const load = useCallback(async () => {
    try {
      const [r, rs] = await Promise.all([api.getReview(id), api.rounds(id).catch(() => [] as Review[])]);
      setReview(r);
      setRounds(rs);
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "The server did not answer.");
    }
  }, [id]);

  useEffect(() => {
    load();
    api.markViewed(id).catch(() => {});
    try {
      setNote(sessionStorage.getItem(NOTE_PREFIX + id + ":note") ?? "");
    } catch {
      // no storage
    }
  }, [id, load]);

  // Every event about this review refreshes it.
  useEffect(() => {
    if (live.lastNotice?.review_id === id) load();
  }, [live.lastNotice, id, load]);

  // The plugin at the review's version: the current one when it matches,
  // otherwise the snapshot, whose entry we assume is index.html.
  useEffect(() => {
    if (!review) return;
    let cancelled = false;
    api
      .plugins()
      .then(({ plugins }) => {
        if (cancelled) return;
        const current = plugins.find((p) => p.name === review.plugin);
        const resolved =
          current && current.version === review.plugin_version && current.usable
            ? current
            : { name: review.plugin, version: review.plugin_version, title: review.plugin, path: "", entry: "index.html", min_height: 400, dev: false, editorial: false, icon: null, usable: true, error: null, settings_schema: null, settings_error: null, settings: null, shortcuts: [], shortcuts_error: null };
        setPlugin(resolved);
        return api.bundleUrl(review, resolved.entry).then((url) => !cancelled && setSrc(url));
      })
      .catch(() => !cancelled && setPlugin(null));
    return () => {
      cancelled = true;
    };
  }, [review?.plugin, review?.plugin_version]); // eslint-disable-line react-hooks/exhaustive-deps

  const previous = useMemo(() => {
    if (!review?.revises) return null;
    return rounds.find((r) => r.id === review.revises) ?? null;
  }, [review, rounds]);
  const revisedBy = useMemo(() => rounds.find((r) => r.revises === review?.id) ?? null, [rounds, review]);

  const readonly = !review || review.status !== "pending";

  const onSubmit = useCallback(
    async (data: unknown): Promise<SubmitResult> => {
      try {
        const decided = await api.decide(id, data, noteRef.current);
        setReview(decided);
        setViolations([]);
        setFlash("Decision recorded");
        try {
          sessionStorage.removeItem(NOTE_PREFIX + id + ":note");
        } catch {
          // ignore
        }
        return { ok: true, decision: decided.decision! };
      } catch (e) {
        if (e instanceof ApiError && e.kind === "invalid") {
          setViolations(e.violations);
          return { ok: false, violations: e.violations };
        }
        setFlash(e instanceof Error ? e.message : "The decision was not recorded.");
        load();
        return { ok: false, violations: [] };
      }
    },
    [id, load],
  );

  // the plugin's own settings as they stand: its defaults under what was set
  const stored = plugin ? prefs.plugins[plugin.name] : undefined;
  const pluginSettings = useMemo(() => {
    if (!plugin?.settings_schema) return null;
    const out: Record<string, unknown> = {};
    for (const [key, property] of Object.entries(plugin.settings_schema.properties)) out[key] = stored && key in stored ? stored[key] : property.default;
    return out;
  }, [plugin, stored]);
  const onSetSetting = useCallback(
    async (patch: Record<string, unknown>): Promise<Violation[]> => {
      if (!plugin) return [];
      try {
        await api.patchSettings({ plugins: { [plugin.name]: patch } });
        return [];
      } catch (e) {
        return e instanceof ApiError ? e.violations : [{ path: "", message: e instanceof Error ? e.message : "The setting was not kept" }];
      }
    },
    [plugin],
  );

  const bridge = usePluginBridge({
    frame,
    reviewId: review?.id ?? null,
    review,
    previous: previous ? { ...previous } : null,
    readonly,
    minHeight: plugin?.min_height ?? 400,
    src,
    connected: live.connected,
    onSubmit,
    settings: pluginSettings,
    onSetSetting,
  });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && maximized) {
        setMaximized(false);
        return;
      }
      if (hasMod(event) && event.shiftKey && !event.altKey && (event.key === "m" || event.key === "M")) {
        event.preventDefault();
        if (plugin) setMaximized((m) => !m);
        return;
      }
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)) return;
      if (event.key === "[" && previous) navigate(`/reviews/${previous.id}`);
      if (event.key === "]" && revisedBy) navigate(`/reviews/${revisedBy.id}`);
      // one of the plugin's declared keys, pressed with the shell in focus:
      // it goes to the view as if typed there
      const combo = comboFromEvent(event);
      if (combo && !isShadowed(combo) && plugin?.shortcuts?.some((s) => s.keys === combo)) {
        event.preventDefault();
        frame.current?.contentWindow?.postMessage(
          { wicket: 1, type: "key", key: event.key, code: event.code, metaKey: event.metaKey, ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey },
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
    window.addEventListener("wicket:command", onCommand);
    window.addEventListener("wicket:discard", onDiscard);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wicket:command", onCommand);
      window.removeEventListener("wicket:discard", onDiscard);
    };
  }, [previous, revisedBy, navigate, maximized, plugin]);

  // the view leaves with the review
  useEffect(() => setMaximized(false), [id]);

  // the bar: where this came from, the title, its origin link; status and
  // maximize at the right
  const originUrl = review?.origin.url ?? null;
  const crumb = useMemo(
    () =>
      review ? (
        <span className="crumb">
          <Link to={back.to} state={location.state} className="crumb-root">
            {back.label}
          </Link>
          <span className="crumb-sep">›</span>
          <span className="crumb-title" title={review.title}>
            {review.title}
          </span>
          {originUrl ? (
            <Tooltip label={`Open ${review.origin.ref ?? "the origin"} in the browser`}>
              <a href={originUrl} target="_blank" rel="noreferrer" className="bar-button crumb-link" aria-label="Open the origin">
                <ExternalLink size={13} />
              </a>
            </Tooltip>
          ) : null}
        </span>
      ) : null,
    [review?.title, review?.origin.ref, originUrl, back.to, back.label, location.state], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const actions = useMemo(
    () =>
      review ? (
        <>
          <OutcomeBadge review={review} />
          {review.status === "pending" ? (
            <Tooltip label="Discard: the agent is told to stop" side="bottom">
              <button type="button" className="bar-button" onClick={() => setDiscarding(true)} aria-label="Discard this review" data-discard>
                <Ban size={15} />
              </button>
            </Tooltip>
          ) : null}
          {plugin ? (
            <Tooltip label="Maximize the view" keys={[MOD, "⇧", "M"]} side="bottom">
              <button type="button" className="bar-button" onClick={() => setMaximized(true)} aria-label="Maximize the view" data-maximize>
                <Maximize2 size={15} />
              </button>
            </Tooltip>
          ) : null}
        </>
      ) : null,
    [review?.status, plugin], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const forPlugin = useMemo(
    () => (plugin ? { name: plugin.name, title: plugin.title || plugin.name, icon: plugin.icon, shortcuts: plugin.shortcuts ?? [] } : undefined),
    [plugin],
  );
  const topbar = useMemo(() => (review ? { crumb, actions, plugin: forPlugin } : null), [review, crumb, actions, forPlugin]);
  useTopBar(topbar);

  const onNote = (value: string) => {
    setNote(value);
    try {
      sessionStorage.setItem(NOTE_PREFIX + id + ":note", value);
    } catch {
      // ignore
    }
  };

  if (error) {
    return (
      <div className="review-page">
        <p className="notice notice-danger">{error}</p>
      </div>
    );
  }
  if (!review) return <div className="review-page" />;

  const origin = review.origin;
  const originText = [origin.repo, origin.workflow].filter(Boolean).join(" / ");

  return (
    <div className="review-page">
      <div className="review-strip">
        <PluginBadge name={review.plugin} version={review.plugin_version} />
        {originText ? (
          <span>
            {originText}
            {origin.ref ? <span className="mono faint"> #{origin.ref}</span> : null}
          </span>
        ) : null}
        {review.requested_by ? (
          <span className="with-icon">
            <Bot size={13} /> {review.requested_by}
          </span>
        ) : null}
        <span title={stamp(review.created_at)}>{age(review.created_at)} ago</span>
        {review.expires_at ? (
          <span className="with-icon" title={stamp(review.expires_at)}>
            <Clock size={13} /> expires {stamp(review.expires_at)}
          </span>
        ) : null}
        <span className="mono faint">{review.id}</span>
        <span className="strip-spacer" />
        {rounds.length > 1 ? (
          <span className="rounds" role="navigation" aria-label="Rounds">
            <span className="rounds-cap">rounds</span>
            {rounds.map((r, i) => (
              <Tooltip key={r.id} label={r.title} side="bottom">
                <Link to={`/reviews/${r.id}`} state={location.state} className={`round-pill ${r.id === review.id ? "is-current" : ""}`} aria-current={r.id === review.id ? "page" : undefined}>
                  {i + 1}
                </Link>
              </Tooltip>
            ))}
          </span>
        ) : null}
      </div>

      {flash ? (
        <p className="notice" onAnimationEnd={() => setFlash(null)}>
          {flash}
        </p>
      ) : null}
      {review.status === "withdrawn" ? (
        <p className="notice">
          The requester withdrew this review {age(review.withdrawn_at)} ago
          {review.withdrawn_reason ? `: ${review.withdrawn_reason}` : "."} Nothing was decided.
        </p>
      ) : null}
      {review.status === "expired" ? <p className="notice notice-danger">This review expired at {stamp(review.expires_at)} without a decision.</p> : null}
      {review.status === "discarded" ? (
        <p className="notice">
          <span className="text">{review.discarded_by}</span> discarded this review {age(review.discarded_at)} ago
          {review.discarded_reason ? `: ${review.discarded_reason}` : "."} The agent was told to stop; nothing was decided.
        </p>
      ) : null}
      {discarding && review.status === "pending" ? (
        <DiscardDialog
          review={review}
          onClose={() => setDiscarding(false)}
          onDone={() => {
            setDiscarding(false);
            setFlash("Discarded. The agent was told to stop.");
            load();
          }}
        />
      ) : null}
      {review.decision ? (
        <section className="decision-box">
          <div className="dim">
            Decided by <span className="text">{review.decision.decided_by}</span>{" "}
            <span title={stamp(review.decision.decided_at)}>{age(review.decision.decided_at)} ago</span>
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
        <p className="notice notice-danger">
          The view for {review.plugin} v{review.plugin_version} is not available, so this review cannot be rendered. Recorded decisions remain.
        </p>
      ) : null}

      {plugin ? (
        <div className={`plugin-frame-wrap ${bridge.fill ? "is-fill" : ""} ${maximized ? "is-maximized" : ""} ${overlayTitleBar ? "has-overlay-bar" : ""}`}>
          {maximized ? (
            <div className="frame-bar" data-tauri-drag-region>
              <span className="frame-bar-title">{review.title}</span>
              <Tooltip label="Restore the view" keys={[MOD, "⇧", "M"]} side="bottom">
                <button type="button" className="bar-button" onClick={() => setMaximized(false)} aria-label="Restore the view" data-restore>
                  <Minimize2 size={15} />
                </button>
              </Tooltip>
            </div>
          ) : null}
          {!bridge.loaded ? (
            <div className="plugin-loading" role="status">
              <span className="spinner" aria-hidden="true" />
              <span>Loading the view…</span>
            </div>
          ) : null}
          <iframe
            key={review.id}
            ref={frame}
            id="plugin-frame"
            sandbox="allow-scripts"
            referrerPolicy="no-referrer"
            title={review.title}
            className="plugin-frame"
            style={{ height: `${plugin.min_height}px` }}
          />
        </div>
      ) : null}

      {violations.length > 0 ? (
        <ul className="violations">
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
            <span className="composer-hint">
              {live.connected ? "The note travels with your decision" : "Reconnecting — hand-over resumes when the server is back"}
            </span>
            <Tooltip label={live.connected ? "Hand over to the agent" : "Reconnect to hand over"} keys={[MOD, "Enter"]} side="top">
              <button type="button" className="handover-button with-icon" data-handover disabled={bridge.submitting || !live.connected} onClick={bridge.collect}>
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

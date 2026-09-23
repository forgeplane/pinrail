import { Ban, Bot, ClipboardCheck, ClipboardX, Clock, Copy, ExternalLink, Maximize2, Minimize2, Send } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";
import { ApiError, api } from "../api/client";
import { copyText } from "../lib/clipboard";
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
  const fromHistory = (location.state as { from?: string } | null)?.from === "history";
  const back = fromHistory ? { to: "/history", label: "History" } : { to: "/", label: "Inbox" };
  const [review, setReview] = useState<Review | null>(null);
  const [rounds, setRounds] = useState<Review[]>([]);
  // the other rounds still waiting that have never been opened: what is new
  const [unopened, setUnopened] = useState<Set<string>>(new Set());
  // The plugin and its bundle URL, resolved together for one plugin at one
  // version. The screen outlives a change of review, so what was resolved
  // for the last review stays in state until the lookup for this one lands:
  // it counts only when it was resolved for the review on screen.
  const [resolved, setResolved] = useState<{ key: string; plugin: Plugin | null; src: string | null } | null>(null);
  const pluginKey = review ? `${review.plugin}@${review.plugin_version}` : null;
  const current = resolved && resolved.key === pluginKey ? resolved : null;
  const plugin: Plugin | null | undefined = current ? current.plugin : undefined;
  const src = current?.src ?? null;
  const [violations, setViolations] = useState<Violation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [discarding, setDiscarding] = useState(false);
  const [note, setNote] = useState("");
  const [maximized, setMaximized] = useState(false);
  const frame = useRef<HTMLIFrameElement>(null);
  const noteRef = useRef(note);
  noteRef.current = note;

  // the review the route names now: a fetch for one clicked past lands late
  // and is dropped, so the screen never settles on a review nobody picked
  const wanted = useRef(id);
  wanted.current = id;
  const load = useCallback(async () => {
    try {
      const [r, rs] = await Promise.all([api.getReview(id), api.rounds(id).catch(() => [] as Review[])]);
      const waiting = rs.filter((round) => round.id !== id && round.status === "pending");
      const opened = await Promise.all(
        waiting.map((round) =>
          api
            .events(round.id)
            .then((events) => events.some((e) => e.kind === "viewed"))
            .catch(() => true),
        ),
      );
      if (wanted.current !== id) return;
      setReview(r);
      setRounds(rs);
      setUnopened(new Set(waiting.filter((_, i) => !opened[i]).map((round) => round.id)));
      setError(null);
    } catch (e) {
      if (wanted.current !== id) return;
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

  // Every event about this review refreshes it, and so does a new round of
  // it: that event names the new review, revising this one or a later round.
  const roundIds = useRef(new Set<string>());
  roundIds.current = new Set([id, ...rounds.map((r) => r.id)]);
  useEffect(() => {
    const notice = live.lastNotice;
    if (!notice) return;
    const revises = notice.review?.revises;
    if (notice.review_id === id || (revises && roundIds.current.has(revises))) load();
  }, [live.lastNotice, id, load]);

  // The plugin at the review's version: the current one when it matches,
  // otherwise the store entry kept for it, whose entry we assume is
  // index.html; null when neither is there, and the review says so.
  useEffect(() => {
    if (!review || !pluginKey) return;
    if (resolved?.key === pluginKey) return;
    let cancelled = false;
    (async () => {
      try {
        const { plugins } = await api.plugins();
        if (cancelled) return;
        const installed = plugins.find((p) => p.name === review.plugin);
        let found: Plugin | null = null;
        if (installed && installed.version === review.plugin_version && installed.usable) {
          found = installed;
        } else {
          const kept = await api.pluginVersions(review.plugin).catch(() => null);
          if (kept?.versions.includes(review.plugin_version)) {
            found = { name: review.plugin, version: review.plugin_version, title: installed?.title ?? review.plugin, path: "", entry: "index.html", min_height: 400, dev: false, editorial: false, icon: installed?.icon ?? null, usable: true, error: null, settings_schema: null, settings_error: null, settings: null, shortcuts: [], shortcuts_error: null, install: null };
          }
        }
        const url = found ? await api.bundleUrl(review, found.entry) : null;
        // the plugin and its URL arrive in one update, for the key they were looked up for
        if (!cancelled) setResolved({ key: pluginKey, plugin: found, src: url });
      } catch {
        if (!cancelled) setResolved({ key: pluginKey, plugin: null, src: null });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [pluginKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const previous = useMemo(() => {
    if (!review?.revises) return null;
    return rounds.find((r) => r.id === review.revises) ?? null;
  }, [review, rounds]);
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

  // the bar: where this came from, the title, its origin link; what can be
  // done with the review at the right. Its outcome leads the page's own strip.
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
          {review.status === "pending" ? (
            <Tooltip label="Discard: the agent is told to stop" side="bottom">
              <button type="button" className="bar-button" onClick={() => setDiscarding(true)} aria-label="Discard this review" data-discard>
                <Ban size={15} />
              </button>
            </Tooltip>
          ) : null}
          <Tooltip label={copied === "done" ? "Copied" : copied === "failed" ? "Could not copy" : "Copy as markdown"} side="bottom">
            <button type="button" className={`bar-button copy-button ${copied === "done" ? "ok" : copied === "failed" ? "danger" : ""}`} onClick={copyMarkdown} aria-label="Copy the review as markdown" data-copy-markdown>
              {copied === "done" ? <ClipboardCheck size={15} className="copy-done" /> : copied === "failed" ? <ClipboardX size={15} /> : <Copy size={15} />}
            </button>
          </Tooltip>
          {plugin ? (
            <Tooltip label="Maximize the view" keys={[MOD, "⇧", "M"]} side="bottom">
              <button type="button" className="bar-button" onClick={() => setMaximized(true)} aria-label="Maximize the view" data-maximize>
                <Maximize2 size={15} />
              </button>
            </Tooltip>
          ) : null}
        </>
      ) : null,
    [review?.status, plugin, copied], // eslint-disable-line react-hooks/exhaustive-deps
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
        <OutcomeBadge review={review} />
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
        <span className="strip-id">
          <span className="mono faint">{review.id}</span>
          <Tooltip label={copiedId === "done" ? "Copied" : copiedId === "failed" ? "Could not copy" : "Copy the id"} side="bottom">
            <button
              type="button"
              className={`id-copy ${copiedId === "done" ? "ok" : copiedId === "failed" ? "danger" : ""}`}
              onClick={copyId}
              aria-label="Copy the review id"
              data-copy-id
            >
              {copiedId === "done" ? <ClipboardCheck size={14} /> : copiedId === "failed" ? <ClipboardX size={14} /> : <Copy size={14} />}
            </button>
          </Tooltip>
        </span>
        <span className="strip-spacer" />
        {rounds.length > 1 ? (
          <span className="rounds" role="navigation" aria-label="Rounds">
            <span className="rounds-cap">rounds</span>
            {rounds.map((r, i) => (
              <Tooltip key={r.id} label={r.title} side="bottom">
                <Link
                  to={`/reviews/${r.id}`}
                  state={location.state}
                  className={`round-pill ${r.id === review.id ? "is-current" : unopened.has(r.id) ? "is-waiting" : ""}`}
                  aria-current={r.id === review.id ? "page" : undefined}
                >
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
      {waiting ? (
        <p className="notice notice-round" data-new-round>
          Round {waiting.number} is waiting for you.{" "}
          <Link to={`/reviews/${waiting.round.id}`} state={location.state}>
            Open it
          </Link>
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
        <div className="notice notice-danger plugin-missing" data-plugin-missing>
          <p>
            <b>{review.plugin} v{review.plugin_version}</b> is not installed, so this review has no view. What was decided is still on record.
          </p>
          <button type="button" className="chrome-button" onClick={() => navigate("/", { state: { settings: "plugins" } })}>
            Open Plugins…
          </button>
        </div>
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

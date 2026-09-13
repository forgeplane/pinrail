import { ArrowLeft, Bot, Clock, Send } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { ApiError, api } from "../api/client";
import type { Plugin, Review, Violation } from "../api/types";
import { usePluginBridge, type SubmitResult } from "../bridge/usePluginBridge";
import { OriginLine, PluginBadge, StatusBadge } from "../components/Badges";
import { Tooltip } from "../components/Tooltip";
import { MOD } from "../lib/keys";
import { age, stamp } from "../lib/format";
import { useLive } from "../state/live";

const NOTE_PREFIX = "wicket:draft:";

export function ReviewScreen() {
  const { id = "" } = useParams();
  const live = useLive();
  const navigate = useNavigate();
  const [review, setReview] = useState<Review | null>(null);
  const [rounds, setRounds] = useState<Review[]>([]);
  const [plugin, setPlugin] = useState<Plugin | null | undefined>(undefined);
  const [src, setSrc] = useState<string | null>(null);
  const [violations, setViolations] = useState<Violation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [note, setNote] = useState("");
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
            : { name: review.plugin, version: review.plugin_version, title: review.plugin, path: "", entry: "index.html", min_height: 400, dev: false, editorial: false, icon: null, usable: true, error: null };
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
  const earlier = useMemo(() => {
    if (!review) return [];
    const at = rounds.findIndex((r) => r.id === review.id);
    return at > 0 ? rounds.slice(0, at).reverse() : [];
  }, [rounds, review]);

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

  const bridge = usePluginBridge({
    frame,
    review,
    previous: previous ? { ...previous } : null,
    readonly,
    minHeight: plugin?.min_height ?? 400,
    src,
    connected: live.connected,
    onSubmit,
  });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA")) return;
      if (event.key === "[" && previous) navigate(`/reviews/${previous.id}`);
      if (event.key === "]" && revisedBy) navigate(`/reviews/${revisedBy.id}`);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [previous, revisedBy, navigate]);

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
        <Link to="/" className="back-link with-icon">
          <ArrowLeft size={14} /> Back to inbox
        </Link>
        <p className="notice notice-danger">{error}</p>
      </div>
    );
  }
  if (!review) return <div className="review-page" />;

  return (
    <div className="review-page">
      <Link to="/" className="back-link with-icon">
        <ArrowLeft size={14} /> Back to inbox
      </Link>
      <header className="review-head">
        <div className="badges">
          <StatusBadge status={review.status} />
          <PluginBadge name={review.plugin} version={review.plugin_version} />
        </div>
        <h1>{review.title}</h1>
        <div className="review-meta">
          <span className="mono faint">{review.id}</span>
          <OriginLine origin={review.origin} />
          {review.requested_by ? (
            <span className="with-icon">
              <Bot size={13} /> {review.requested_by}
            </span>
          ) : null}
          <span title={stamp(review.created_at)}>submitted {age(review.created_at)} ago</span>
          {review.expires_at ? (
            <span className="with-icon" title={stamp(review.expires_at)}>
              <Clock size={13} /> expires {stamp(review.expires_at)}
            </span>
          ) : null}
        </div>
        {earlier.length > 0 || revisedBy ? (
          <div className="review-rounds">
            {revisedBy ? (
              <span>
                revised by <Link to={`/reviews/${revisedBy.id}`}>{revisedBy.title}</Link>
              </span>
            ) : null}
            {earlier.length > 0 ? (
              <span>
                previous rounds:
                {earlier.map((r) => (
                  <Link key={r.id} to={`/reviews/${r.id}`} className="round-link">
                    {r.title}
                  </Link>
                ))}
              </span>
            ) : null}
          </div>
        ) : null}
      </header>

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
        <div className={`plugin-frame-wrap ${bridge.fill ? "is-fill" : ""}`}>
          {!bridge.loaded ? (
            <div className="plugin-loading" role="status">
              <span className="spinner" aria-hidden="true" />
              <span>Loading the view…</span>
            </div>
          ) : null}
          <iframe
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

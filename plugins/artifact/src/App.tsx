// The review: the artifact on a stage, a toolbar above it, the comments
// beside it. In select mode the pointer picks an element; a comment hangs
// on it by a selector, shown as a numbered pin. The verdict follows the
// comments unless the reviewer says otherwise.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { boxIn, commentable, markupOf, mount, selectorFor, snippetOf, type Box } from "./artifact";
import { CommentPopover } from "./components/CommentPopover";
import { CommentsPanel } from "./components/CommentsPanel";
import { Toolbar } from "./components/Toolbar";
import {
  VIEWPORTS,
  newId,
  type Comment,
  type Decision,
  type Kind,
  type Payload,
  type Verdict,
  type Viewport,
} from "./types";
import type { Init, Plugin, Violation } from "./pinrail";

type Editing = {
  id: string | null;
  selector: string;
  tag: string;
  snippet: string;
  html: string;
  text: string;
  kind: Kind;
  box: Box;
};

/** What the connection in main.tsx asks of the view on screen. */
export const view = {
  collect: (): Decision | undefined => undefined,
  violations: (_errors: Violation[]) => {},
  submitted: (_decided: { data: Record<string, unknown> } | null) => {},
};

/** Where the review starts: its decision when it ended, otherwise the draft,
 *  whose shape an earlier release may have written differently. */
function startOf(init: Init): { decision: Decision | null; comments: Comment[]; verdict: Verdict | null } {
  const data = init.review.decision?.data as Decision | undefined;
  if (init.readonly && data) return { decision: data, comments: data.comments ?? [], verdict: data.verdict };
  const draft = init.draft;
  if (draft && Array.isArray(draft.comments)) {
    const verdict = draft.verdict === "approve" || draft.verdict === "revise" ? draft.verdict : null;
    return { decision: null, comments: draft.comments as Comment[], verdict };
  }
  return { decision: null, comments: [], verdict: null };
}

export function App({ plugin, init }: { plugin: Plugin; init: Init }) {
  const [start] = useState(() => startOf(init));
  const [readonly, setReadonly] = useState(init.readonly);
  const [decision, setDecision] = useState<Decision | null>(start.decision);
  const [comments, setComments] = useState<Comment[]>(start.comments);
  const [verdict, setVerdict] = useState<Verdict | null>(start.verdict);
  const [errors, setErrors] = useState<Violation[]>([]);
  const [selecting, setSelecting] = useState(false);
  const [viewport, setViewport] = useState<Viewport>(() => {
    const wanted = (init.review.payload as Payload).viewport;
    return wanted && VIEWPORTS.some((v) => v.key === wanted) ? wanted : "desktop";
  });
  const [panel, setPanel] = useState(true);
  const [hover, setHover] = useState<{ box: Box; label: string } | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [focused, setFocused] = useState<string | null>(null);
  const [pins, setPins] = useState<Map<string, Box>>(new Map());
  const [layoutTick, setLayoutTick] = useState(0);
  // the document: the payload's html, or its file once read
  const [html, setHtml] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const mounted = useRef<{ root: ShadowRoot; body: HTMLElement } | null>(null);
  const latest = useRef({ comments, verdict, readonly });
  latest.current = { comments, verdict, readonly };

  const payload = init.review.payload as Payload;
  const effectiveVerdict: Verdict = verdict ?? (comments.length > 0 ? "revise" : "approve");

  view.violations = setErrors;
  view.submitted = (decided) => {
    setReadonly(true);
    setSelecting(false);
    setEditing(null);
    if (decided?.data) setDecision(decided.data as Decision);
  };
  view.collect = () => {
    const { comments, verdict, readonly } = latest.current;
    if (readonly) return;
    return { verdict: verdict ?? (comments.length > 0 ? "revise" : "approve"), comments };
  };

  // the document, inline or from the file the review carries
  useEffect(() => {
    if (typeof payload.html === "string") {
      setHtml(payload.html);
      return;
    }
    const name = window.Pinrail.attachmentName(payload.file);
    if (!name) return;
    let gone = false;
    plugin
      .attachment(name)
      .then((bytes) => !gone && setHtml(new TextDecoder().decode(bytes)))
      .catch(
        (e: unknown) =>
          !gone && setLoadError(`${name} could not be read: ${e instanceof Error ? e.message : String(e)}`),
      );
    return () => {
      gone = true;
    };
  }, [payload, plugin]);

  // the artifact goes into its shadow root once the document is here
  useEffect(() => {
    if (!host.current || html === null) return;
    mounted.current = mount(host.current, html);
    const observer = new ResizeObserver(() => setLayoutTick((t) => t + 1));
    observer.observe(mounted.current.body);
    setLayoutTick((t) => t + 1);
    return () => observer.disconnect();
  }, [html]);

  // the shell hears what the button should say, and keeps the draft
  useEffect(() => {
    if (readonly) return;
    const n = comments.length;
    plugin.handOverLabel(effectiveVerdict === "approve" ? "Approve" : `Request changes${n ? ` (${n})` : ""}`);
    plugin.draft({ comments, verdict });
  }, [plugin, comments, verdict, effectiveVerdict, readonly]);

  // pins follow their elements
  useEffect(() => {
    const m = mounted.current;
    const l = layer.current;
    if (!m || !l) return;
    const next = new Map<string, Box>();
    for (const c of comments) {
      let el: Element | null;
      try {
        el = m.root.querySelector(c.selector);
      } catch {
        el = null;
      }
      if (el) next.set(c.id, boxIn(el, l));
    }
    setPins(next);
  }, [comments, layoutTick, viewport]);

  useEffect(() => {
    const onResize = () => setLayoutTick((t) => t + 1);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const elementAt = useCallback((event: React.MouseEvent): Element | null => {
    const m = mounted.current;
    if (!m) return null;
    const hit = m.root.elementFromPoint(event.clientX, event.clientY);
    return commentable(hit, m.body);
  }, []);

  const onStageMove = (event: React.MouseEvent) => {
    if (!selecting || editing) return;
    const el = elementAt(event);
    if (!el || !layer.current) {
      setHover(null);
      return;
    }
    setHover({ box: boxIn(el, layer.current), label: describe(el) });
  };

  const onStageClick = (event: React.MouseEvent) => {
    if (!selecting || editing || readonly) return;
    const m = mounted.current;
    const el = elementAt(event);
    if (!el || !m || !layer.current) return;
    event.preventDefault();
    event.stopPropagation();
    setHover(null);
    setEditing({
      id: null,
      selector: selectorFor(el, m.root, m.body),
      tag: el.localName,
      snippet: snippetOf(el),
      html: markupOf(el),
      text: "",
      kind: "change",
      box: boxIn(el, layer.current),
    });
  };

  const editComment = (id: string) => {
    const c = comments.find((x) => x.id === id);
    const box = pins.get(id);
    if (!c || !box || readonly) return;
    setEditing({
      id,
      selector: c.selector,
      tag: c.tag,
      snippet: c.snippet ?? "",
      html: c.html ?? "",
      text: c.text,
      kind: c.kind,
      box,
    });
    setFocused(id);
  };

  const saveEditing = (text: string, kind: Kind) => {
    if (!editing) return;
    const body = text.trim();
    if (!body) return;
    setComments((list) => {
      const entry: Comment = {
        id: editing.id ?? newId(),
        selector: editing.selector,
        tag: editing.tag,
        kind,
        text: body,
        snippet: editing.snippet || undefined,
        html: editing.html || undefined,
      };
      return editing.id ? list.map((c) => (c.id === editing.id ? entry : c)) : [...list, entry];
    });
    setEditing(null);
    setErrors([]);
  };

  const removeComment = (id: string) => {
    setComments((list) => list.filter((c) => c.id !== id));
    if (editing?.id === id) setEditing(null);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (editing) setEditing(null);
        else if (selecting) setSelecting(false);
      }
      if (event.key === "s" && !event.metaKey && !event.ctrlKey && !readonly && !isTyping(event.target)) {
        setSelecting((s) => !s);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [editing, selecting, readonly]);

  const previous = useMemo(() => {
    const data = init.previous?.decision?.data as Decision | undefined;
    return data?.comments ?? [];
  }, [init]);

  const width = VIEWPORTS.find((v) => v.key === viewport)?.width ?? null;

  return (
    <div className={`app ${selecting ? "is-selecting" : ""} ${readonly ? "is-readonly" : ""}`}>
      <Toolbar
        title={payload.title ?? init.review.title}
        selecting={selecting}
        onSelecting={() => setSelecting((s) => !s)}
        viewport={viewport}
        onViewport={setViewport}
        verdict={effectiveVerdict}
        verdictChosen={verdict !== null}
        onVerdict={(v) => setVerdict(v === effectiveVerdict && verdict !== null ? null : v)}
        count={comments.length}
        panel={panel}
        onPanel={() => setPanel((p) => !p)}
        readonly={readonly}
        decided={decision}
      />
      {payload.notes ? (
        <div className="notes" dangerouslySetInnerHTML={{ __html: window.Pinrail.markdown(payload.notes) }} />
      ) : null}
      {loadError ? (
        <div className="pinrail-errors errors" data-load-error>
          {loadError}
        </div>
      ) : null}
      {errors.length > 0 ? (
        <div className="pinrail-errors errors">
          {errors.map((e, i) => (
            <div key={i}>
              {e.path || "/"}: {e.message}
            </div>
          ))}
        </div>
      ) : null}
      <div className="main">
        <div
          className="stage"
          onMouseMove={onStageMove}
          onMouseLeave={() => setHover(null)}
          onClickCapture={onStageClick}
          data-stage
        >
          <div className="frame" style={{ width: width ? `${width}px` : "100%" }} data-viewport={viewport}>
            <div ref={host} className="host" data-artifact />
            <div ref={layer} className="layer" aria-hidden={!editing}>
              {hover ? (
                <div
                  className="hover-box"
                  style={{ left: hover.box.x, top: hover.box.y, width: hover.box.w, height: hover.box.h }}
                >
                  <span className="hover-label">{hover.label}</span>
                </div>
              ) : null}
              {comments.map((c, i) => {
                const box = pins.get(c.id);
                if (!box) return null;
                const active = focused === c.id || editing?.id === c.id;
                return (
                  <div
                    key={c.id}
                    className={`pin-box ${active ? "is-active" : ""} kind-${c.kind}`}
                    style={{ left: box.x, top: box.y, width: box.w, height: box.h }}
                  >
                    <button
                      type="button"
                      className="pin"
                      data-pin={c.id}
                      title={c.text}
                      onClick={(e) => {
                        e.stopPropagation();
                        setFocused(c.id);
                        if (!readonly) editComment(c.id);
                      }}
                    >
                      {i + 1}
                    </button>
                  </div>
                );
              })}
              {editing && editing.id === null ? (
                // the element a new comment is about, held while it is written
                <div
                  className="target-box"
                  style={{ left: editing.box.x, top: editing.box.y, width: editing.box.w, height: editing.box.h }}
                />
              ) : null}
              {editing ? (
                <CommentPopover
                  key={editing.id ?? "new"}
                  box={editing.box}
                  selector={editing.selector}
                  tag={editing.tag}
                  text={editing.text}
                  kind={editing.kind}
                  isNew={editing.id === null}
                  onSave={saveEditing}
                  onCancel={() => setEditing(null)}
                  onRemove={editing.id ? () => removeComment(editing.id!) : undefined}
                />
              ) : null}
            </div>
          </div>
        </div>
        {panel ? (
          <CommentsPanel
            comments={comments}
            pins={pins}
            focused={focused}
            readonly={readonly}
            previous={previous}
            onFocus={(id) => {
              setFocused(id);
              const el = document.querySelector<HTMLElement>(`[data-pin="${id}"]`);
              el?.scrollIntoView({ block: "center", behavior: "smooth" });
            }}
            onEdit={editComment}
            onRemove={removeComment}
          />
        ) : null}
      </div>
    </div>
  );
}

function describe(el: Element): string {
  const id = el.id ? `#${el.id}` : "";
  const cls = el.classList.length ? `.${Array.from(el.classList).slice(0, 2).join(".")}` : "";
  return `${el.localName}${id}${cls}`;
}

function isTyping(target: EventTarget | null) {
  const el = target as HTMLElement | null;
  return (
    !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
  );
}

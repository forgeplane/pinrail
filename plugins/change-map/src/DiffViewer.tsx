// A file's diff in a drawer over the bottom of the map, which stays in
// view above it: its hunks with both line numbers, highlighted
// in the file's language, with the selected part's lines marked. A click on
// a line's comment button comments on that line, and a drag from it on the
// lines it passes over. Dragging the top edge resizes the drawer, and Esc or
// the close button closes it.
import {
  Bird,
  Cat,
  ChevronLeft,
  ChevronRight,
  Coffee,
  Ghost,
  Leaf,
  MessageSquarePlus,
  Moon,
  Telescope,
  Wind,
  X,
  type LucideIcon,
} from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import {
  countLines,
  hunkLines,
  firstLine,
  isLine,
  sameChange,
  takesIn,
  type Comment,
  type FileChange,
  type LineTarget,
  type Payload,
  type Target,
} from "./types";
import { CommentItem } from "./CommentItem";
import type { Unmapped } from "./lines";
import { Compose } from "./Compose";

const { Pinrail } = window;

type Line = {
  kind: "context" | "add" | "del" | "hunk";
  text: string;
  old?: number;
  new?: number;
  /** the line highlighted, as HTML */
  html?: string;
};

/** The lines of a unified diff, numbered on each side, and highlighted by
 *  the whole of each side so that a construct over several lines colours
 *  right. */
export function parseDiff(diff: string, path: string): Line[] {
  const out: Line[] = [];
  let o = 0;
  let n = 0;
  for (const raw of hunkLines(diff)) {
    const hunk = raw.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/);
    if (hunk) {
      o = Number(hunk[1]);
      n = Number(hunk[2]);
      out.push({ kind: "hunk", text: raw });
      continue;
    }
    if (raw.startsWith("+")) out.push({ kind: "add", text: raw.slice(1), new: n++ });
    else if (raw.startsWith("-")) out.push({ kind: "del", text: raw.slice(1), old: o++ });
    else out.push({ kind: "context", text: raw.slice(1), old: o++, new: n++ });
  }
  const language = Pinrail.languageOf(path);
  const side = (keep: (l: Line) => boolean) => {
    const lines = out.filter(keep);
    const html = Pinrail.highlightLines(lines.map((l) => l.text).join("\n"), language);
    lines.forEach((l, i) => (l.html = html[i] ?? Pinrail.escape(l.text)));
  };
  side((l) => l.kind === "context" || l.kind === "add");
  side((l) => l.kind === "del");
  return out;
}

/** The drawer's height, as a share of the map's, stored on this machine. */
const HEIGHT_KEY = "change-map.diff-height";
export const savedHeight = () => {
  try {
    const h = Number(localStorage.getItem(HEIGHT_KEY));
    return h >= 0.2 && h <= 0.95 ? h : 0.55;
  } catch {
    return 0.55;
  }
};

/** Resizes a drawer by its top edge, as a share of the map's height, and
 *  keeps the height on this machine. */
export function resizeDrawer(
  event: React.PointerEvent<HTMLElement>,
  drawer: HTMLElement | null,
  setHeight: (height: number) => void,
) {
  event.preventDefault();
  const stage = drawer?.parentElement?.getBoundingClientRect();
  if (!stage) return;
  const share = (y: number) => Math.min(0.95, Math.max(0.2, (stage.bottom - y) / stage.height));
  const onMove = (e: PointerEvent) => setHeight(share(e.clientY));
  const onUp = (e: PointerEvent) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    document.body.classList.remove("cm-resizing");
    try {
      localStorage.setItem(HEIGHT_KEY, String(share(e.clientY)));
    } catch {
      // without storage, the height lasts until the view reloads
    }
  };
  document.body.classList.add("cm-resizing");
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

/** A row's side and line: a removed line is numbered on the old side, the others on the new. */
const rowOf = (l: Line): { side: "old" | "new"; line: number } =>
  l.kind === "del" ? { side: "old", line: l.old! } : { side: "new", line: l.new! };
/** Whether a row is the last line of a target, under which its comments go. */
const endsAt = (t: Target, path: string, l: Line) =>
  isLine(t) && t.file === path && (t.side ?? "new") === rowOf(l).side && t.line === rowOf(l).line;
/** The icons and lines an empty drawer picks from, to make an empty drawer
 *  less of a dead end. */
const EMPTY_ICONS: LucideIcon[] = [Ghost, Telescope, Coffee, Cat, Bird, Wind, Leaf, Moon];
const EMPTY_LINES = [
  "Ask your agent to be more thorough next time.",
  "The agent left this one to your imagination.",
  "Not a single line of code behind this one.",
  "Nothing to read here. Enjoy the quiet.",
  "This one came without its code.",
  "The diff for this one went for a walk.",
  "Some things are better left unwritten. This was not one of them.",
  "The agent drew the arrow and forgot the code.",
  "Plenty of room here for the lines that should have been.",
];

/** A number from a name, so that the same selection always picks the same
 *  icon and line. */
function pick(name: string, size: number): number {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h % size;
}

/** The drawer for a selection with no files in the change: an icon, a light
 *  line, and what is missing. */
function NoFiles({ owner, ownerAsCode }: { owner: string; ownerAsCode: boolean }) {
  const Icon = EMPTY_ICONS[pick(owner, EMPTY_ICONS.length)];
  return (
    <div className="cm-diff-empty" data-testid="no-files">
      <Icon className="cm-empty-icon" aria-hidden="true" />
      <p className="cm-empty-quip">{EMPTY_LINES[pick(`${owner}.`, EMPTY_LINES.length)]}</p>
      <p className="pinrail-faint">
        No files of {ownerAsCode ? <span className="mono">{owner}</span> : owner} are in this change.
      </p>
    </div>
  );
}

/** Whether a target takes in a row. */
const holds = (t: Target | null | undefined, path: string, l: Line) =>
  !!t && isLine(t) && t.file === path && takesIn(t, rowOf(l).side, rowOf(l).line);

export function DiffViewer({
  payload,
  path,
  siblings,
  unmapped,
  owner,
  ownerAsCode = false,
  step,
  change,
  at,
  height,
  setHeight,
  readonly,
  comments,
  addComment,
  removeComment,
  editComment,
  onClose,
}: {
  payload: Payload;
  /** the file shown, or null when the selection has none to show */
  path: string | null;
  /** the file's changed lines that nothing on the map covers */
  unmapped?: Unmapped;
  /** the selection's file changes, to step through */
  siblings: FileChange[];
  /** the selection's title */
  owner: string;
  /** the title is an interaction's, a message's or a transition's label, in monospace */
  ownerAsCode?: boolean;
  step(change: FileChange): void;
  change?: FileChange;
  /** a line to show first, such as a comment's */
  at?: LineTarget;
  /** the drawer's height, as a share of the map's */
  height: number;
  setHeight(height: number): void;
  readonly: boolean;
  comments: Comment[];
  addComment(target: Target, body: string): void;
  removeComment(comment: Comment): void;
  editComment(comment: Comment, body: string): void;
  onClose(): void;
}) {
  const file = path ? payload.files.find((f) => f.path === path) : undefined;
  const index = siblings.findIndex((c) => sameChange(c, change) || (!change && c.file === path));
  const lines = useMemo(() => (file ? parseDiff(file.diff, file.path) : []), [file]);
  const counts = file ? countLines(file.diff) : { added: 0, removed: 0 };
  const [writing, setWriting] = useState<LineTarget | null>(null);
  // lines being picked by dragging from a comment button, on one side
  const [drag, setDrag] = useState<{ side: "old" | "new"; from: number; to: number } | null>(null);
  const comment = (side: "old" | "new", from: number, to: number) => {
    const [lo, hi] = from <= to ? [from, to] : [to, from];
    setWriting({ file: path!, side, line: hi, ...(lo < hi ? { start_line: lo } : {}) });
    setText("");
  };
  /** Starts picking lines at a row's comment button; the release opens the
   *  comment field under the last line picked. */
  const startPick = (event: React.PointerEvent<HTMLButtonElement>, l: Line) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const { side, line } = rowOf(l);
    let to = line;
    setDrag({ side, from: line, to });
    const onMove = (e: PointerEvent) => {
      const row = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("tr[data-line]");
      const [s, n] = (row?.dataset.line ?? "").split(":");
      if (s !== side || !n) return;
      to = Number(n);
      setDrag({ side, from: line, to });
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      document.body.classList.remove("cm-picking");
      setDrag(null);
      comment(side, line, to);
    };
    document.body.classList.add("cm-picking");
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const [text, setText] = useState("");
  const root = useRef<HTMLDivElement>(null);

  const startResize = (event: React.PointerEvent<HTMLDivElement>) => resizeDrawer(event, root.current, setHeight);
  // a changed line that no part, interaction or diagram covers
  const offMap = (l: Line) =>
    !!unmapped && (l.kind === "add" ? unmapped.added.has(l.new!) : l.kind === "del" && unmapped.removed.has(l.old!));
  const off = unmapped ? unmapped.added.size + unmapped.removed.size : 0;
  const inRange = (l: Line) =>
    l.new !== undefined && l.kind !== "del" && !!change?.lines?.some(([a, b]) => l.new! >= a && l.new! <= b);

  // Esc cancels the comment being written, then closes the drawer. It is
  // the drawer's while the drawer is open: the capture phase runs this
  // before the SDK's own listener, which would pass an Esc nobody kept up
  // to the app, and the app would close its side panel. A comment being
  // edited keeps its Esc, which cancels the edit.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (writing) {
        e.preventDefault();
        setWriting(null);
        return;
      }
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "TEXTAREA" || el.tagName === "INPUT")) return;
      e.preventDefault();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [writing, onClose]);

  // The line asked for comes into view, or else the first marked line: when
  // the file opens, and when another part or interaction marks other lines
  // of the same file
  const ranges = JSON.stringify(change?.lines ?? []);
  useEffect(() => {
    const line = at && root.current?.querySelector(`[data-line="${at.side ?? "new"}:${firstLine(at)}"]`);
    (line || root.current?.querySelector(".cm-in-range"))?.scrollIntoView({ block: "center" });
    // the overview's files keep the focus, for their arrow keys
    if (!document.activeElement?.closest(".cm-finder")) root.current?.focus();
  }, [path, at, ranges]);

  const save = () => {
    if (!writing || !text.trim()) return;
    addComment(writing, text.trim());
    setText("");
    setWriting(null);
  };

  return (
    <div
      className="cm-diff"
      role="region"
      aria-label={path ? `Diff of ${path}` : "Diff"}
      ref={root}
      tabIndex={-1}
      data-testid="diff"
      style={{ height: `${height * 100}%` }}
    >
      <div className="cm-diff-resize" onPointerDown={startResize} aria-hidden="true" />
      <div className="cm-diff-head">
        {index >= 0 && siblings.length > 1 ? (
          <span className="cm-stepper">
            <button
              type="button"
              className="pinrail-btn pinrail-btn-ghost"
              aria-label="Previous file"
              title="Previous file"
              disabled={index === 0}
              onClick={() => step(siblings[index - 1])}
            >
              <ChevronLeft />
            </button>
            <span className="pinrail-faint" data-testid="file-position">
              {index + 1} of {siblings.length}
            </span>
            <button
              type="button"
              className="pinrail-btn pinrail-btn-ghost"
              aria-label="Next file"
              title="Next file"
              disabled={index === siblings.length - 1}
              onClick={() => step(siblings[index + 1])}
            >
              <ChevronRight />
            </button>
          </span>
        ) : null}
        {path ? (
          <span className="cm-diff-path mono" title={path}>
            {path}
          </span>
        ) : (
          // left to right, unlike a path, so a label's closing brackets stay at its end
          <span className={`cm-diff-owner${ownerAsCode ? " mono" : ""}`} title={owner}>
            {owner}
          </span>
        )}
        {path ? (
          <span className="cm-counts mono">
            <span className="cm-add">+{counts.added}</span> <span className="cm-del">−{counts.removed}</span>
          </span>
        ) : null}
        {path && off ? (
          <span className="cm-unmapped-count" data-testid="unmapped-count">
            {off} not on the map
          </span>
        ) : null}
        <button
          type="button"
          className="pinrail-btn pinrail-btn-ghost"
          onClick={onClose}
          title="Close the diff (Esc)"
          aria-label="Close the diff"
        >
          <X />
        </button>
      </div>
      {path ? null : <NoFiles owner={owner} ownerAsCode={ownerAsCode} />}
      <div className="cm-diff-body" hidden={!path}>
        <table className="cm-diff-table">
          <tbody>
            {lines.map((l, i) => {
              if (l.kind === "hunk")
                return (
                  <tr key={i} className="cm-hunk">
                    <td colSpan={4} className="mono">
                      {l.text}
                    </td>
                  </tr>
                );
              const { side, line } = rowOf(l);
              const here = comments.filter((c) => endsAt(c.target, path!, l));
              const open = !!writing && endsAt(writing, path!, l);
              const picked =
                !!drag &&
                drag.side === side &&
                line >= Math.min(drag.from, drag.to) &&
                line <= Math.max(drag.from, drag.to);
              const commented = comments.some((c) => holds(c.target, path!, l));
              const loose = offMap(l);
              return (
                <Fragment key={i}>
                  <tr
                    className={`cm-line cm-line-${l.kind}${inRange(l) ? " cm-in-range" : ""}${holds(at, path!, l) ? " cm-at" : ""}${commented ? " cm-commented" : ""}${picked || holds(writing, path!, l) ? " cm-picked" : ""}${drag && drag.side === side && drag.from === line ? " cm-anchor" : ""}${loose ? " cm-unmapped" : ""}`}
                    data-line={l.kind === "del" ? `old:${l.old}` : `new:${l.new}`}
                    title={loose ? "Not on the map" : undefined}
                  >
                    <td className="cm-num mono">{l.old ?? ""}</td>
                    <td className="cm-num mono">{l.new ?? ""}</td>
                    <td className="cm-gut">
                      {readonly ? null : (
                        <button
                          type="button"
                          className="cm-add-comment"
                          aria-label={`Comment on line ${line}`}
                          title="Comment on this line, or drag to comment on several"
                          onPointerDown={(e) => startPick(e, l)}
                          // the keyboard's Enter or Space: this line alone
                          onClick={(e) => e.detail === 0 && comment(side, line, line)}
                        >
                          <MessageSquarePlus />
                        </button>
                      )}
                    </td>
                    <td className="cm-code-line mono">
                      <span className="cm-sign">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : " "}</span>
                      <span dangerouslySetInnerHTML={{ __html: l.html ?? "" }} />
                    </td>
                  </tr>
                  {here.length || open ? (
                    <tr className="cm-line-comments">
                      <td colSpan={3} />
                      <td>
                        {here.map((c, j) => (
                          <CommentItem
                            key={j}
                            comment={c}
                            readonly={readonly}
                            language={Pinrail.languageOf(path!)}
                            onEdit={(body) => editComment(c, body)}
                            onDelete={() => removeComment(c)}
                          />
                        ))}
                        {open ? (
                          <Compose
                            id="line-comment"
                            autoFocus
                            placeholder={
                              writing?.start_line
                                ? `Comment on lines ${firstLine(writing)}–${writing.line}`
                                : "Comment on this line"
                            }
                            value={text}
                            onChange={setText}
                            onSubmit={save}
                            onCancel={() => setWriting(null)}
                          />
                        ) : null}
                      </td>
                    </tr>
                  ) : null}
                </Fragment>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

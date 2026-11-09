// The panel beside the diagram: the change as a whole while nothing is
// selected, and the selected part or interaction otherwise, with its
// screenshots, its files, its comments and the reviewed mark.
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  ExternalLink,
  FileCode,
  Film,
  GitBranch,
  GitFork,
  Image as ImageIcon,
  ListOrdered,
  MessageCircle,
  Play,
  Search,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { kindIcon, kindLabel, statusLabel, type Marks } from "./Diagram";
import {
  countLines,
  covers,
  diagramComments,
  edgeStatus,
  findItem,
  isItem,
  isLine,
  itemTitle,
  markOf,
  selectionOf,
  isScreen,
  keyOf,
  linesText,
  nodeStatus,
  sameChange,
  type LineTarget,
  type Comment,
  type ExtraDiagram,
  type FileChange,
  type Found,
  type Link,
  type Part,
  type Payload,
  type Selection,
  type Target,
} from "./types";
import { clock, screenComments } from "./ScreenViewer";
import { routine, type Unmapped } from "./lines";
import { CommentItem } from "./CommentItem";
import { Compose } from "./Compose";
import { dedent, highlightLines, markdown } from "./markdown";

/** The most lines of code a line comment shows in the panel. */
const SNIPPET = 12;

function Markdown({ text, className, language }: { text?: string; className?: string; language?: string | null }) {
  if (!text?.trim()) return null;
  return <div className={`cm-md ${className ?? ""}`} dangerouslySetInnerHTML={{ __html: markdown(text, language) }} />;
}

export type PanelProps = {
  payload: Payload;
  selection: Selection;
  readonly: boolean;
  comments: Comment[];
  reviewed: Set<string>;
  images: Map<string, string>;
  select(selection: Selection): void;
  /** selects a part and zooms the map in on it */
  zoomTo(node: string): void;
  /** each part's and interaction's comments and reviewed mark, as the map shows them */
  marks: Marks;
  /** how many of the change's lines are on the map, of how many */
  coverage: { total: number; mapped: number; unmapped: Map<string, Unmapped> };
  /** the file chosen in the overview's files, shown on the map, or null */
  chosenFile: string | null;
  /** chooses a file to show on the map, or none */
  chooseFile(path: string | null): void;
  /** the view the panel showed before, to go back to */
  back: { to: string; go(): void } | null;
  /** the file in the diff drawer, marked in the lists */
  current: FileChange | null;
  /** the selected part's screen in the drawer, marked in its screenshots */
  screen: number | null;
  /** opens a part's screen, a recording at a moment when given */
  openScreen(index: number, time?: number): void;
  /** opens a file's diff, at a line when one is given */
  openFile(path: string, change?: FileChange, at?: LineTarget): void;
  openLink(url: string): void;
  /** shows an extra diagram on the canvas */
  openDiagram(id: string): void;
  addComment(target: Target, body: string): void;
  removeComment(comment: Comment): void;
  editComment(comment: Comment, body: string): void;
  toggleReviewed(id: string): void;
  /** a count that focuses the comment field each time it goes up */
  composeAt: number;
  /** the panel's footer, where the comment field goes; in place without one */
  footer: HTMLElement | null;
};

export function Panel(props: PanelProps) {
  const { back } = props;
  return (
    <>
      {back ? (
        <button type="button" className="cm-back" onClick={back.go} title={`Back to ${back.to}`}>
          <ArrowLeft /> {back.to}
        </button>
      ) : null}
      <View {...props} />
    </>
  );
}

function View(props: PanelProps) {
  const { payload, selection } = props;
  if (selection && "node" in selection) {
    const part = payload.nodes.find((n) => n.id === selection.node);
    if (part) return <PartDetail {...props} part={part} />;
  }
  if (selection && "edge" in selection) {
    const link = payload.edges?.find((e) => e.id === selection.edge);
    if (link) return <LinkDetail {...props} link={link} />;
  }
  if (selection && isItem(selection)) {
    const found = findItem(payload, selection);
    if (found) return <ItemDetail {...props} found={found} />;
  }
  return <Overview {...props} />;
}

function Overview(props: PanelProps) {
  const { payload, zoomTo, openLink, comments } = props;
  const change = payload.change;
  const claimed = new Set([
    ...payload.nodes.flatMap((n) => (n.changes ?? []).map((c) => c.file)),
    ...(payload.edges ?? []).flatMap((e) => (e.changes ?? []).map((c) => c.file)),
  ]);
  const others = payload.files.filter((f) => !claimed.has(f.path));
  const changed = payload.nodes.filter((n) => nodeStatus(n) !== "context");
  // the comments on each part, its lines and its screenshots, as the map counts them
  const counted = (id: string) => props.marks.get(id)?.comments ?? 0;
  const isReviewed = (id: string) => props.marks.get(id)?.reviewed ?? false;
  return (
    <div className="cm-detail" data-testid="overview">
      {change?.source || change?.ref ? (
        <p className="cm-branch pinrail-dim">
          {change.ref ? <span className="cm-ref">{change.ref}</span> : null}
          {change.source ? (
            <>
              <GitBranch />
              <span className="mono">{change.source}</span>
              {change.target ? (
                <>
                  <ArrowRight />
                  <span className="mono">{change.target}</span>
                </>
              ) : null}
            </>
          ) : null}
          {change.url ? (
            <button
              type="button"
              className="pinrail-btn pinrail-btn-ghost cm-open"
              onClick={() => openLink(change.url!)}
            >
              <ExternalLink /> Open
            </button>
          ) : null}
        </p>
      ) : null}
      <Markdown text={payload.summary ?? change?.description} />
      {props.coverage.total ? (
        <p
          className={`cm-coverage${props.coverage.mapped < props.coverage.total ? " cm-coverage-short" : ""}`}
          data-testid="coverage"
        >
          {props.coverage.mapped === props.coverage.total
            ? "Every changed line is on the map."
            : `${props.coverage.mapped} of ${props.coverage.total} changed lines are on the map.`}
        </p>
      ) : null}
      <h3 className="pinrail-eyebrow">Parts changed</h3>
      <ul className="cm-parts cm-marked-rows">
        {changed.map((n) => {
          const Icon = kindIcon[n.kind ?? "module"];
          const status = nodeStatus(n);
          return (
            <li key={n.id}>
              <button
                type="button"
                className="cm-row"
                onClick={() => zoomTo(n.id)}
                aria-label={`${n.title}, ${statusLabel[status]}${isReviewed(n.id) ? ", reviewed" : ""}`}
              >
                {/* a reviewed part has a check on its icon's corner */}
                <span className="cm-row-icon">
                  <Icon />
                  {isReviewed(n.id) ? (
                    <span className="cm-icon-reviewed" title="Reviewed">
                      <Check />
                    </span>
                  ) : null}
                </span>
                <span className="cm-row-title">{n.title}</span>
                {/* the comments beside the name; the status keeps its column */}
                {counted(n.id) ? (
                  <span
                    className="cm-mark cm-mark-comment"
                    title={`${counted(n.id)} ${counted(n.id) === 1 ? "comment" : "comments"}`}
                  >
                    <MessageCircle />
                    {counted(n.id)}
                  </span>
                ) : null}
                <span className={`cm-status cm-status-${status} cm-row-end`}>{statusLabel[status]}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {payload.diagrams?.length ? (
        <>
          <h3 className="pinrail-eyebrow">Diagrams</h3>
          <DiagramList
            payload={payload}
            diagrams={payload.diagrams}
            openDiagram={props.openDiagram}
            comments={comments}
          />
        </>
      ) : null}
      {others.length ? (
        <>
          <h3 className="pinrail-eyebrow">Other files</h3>
          <Files
            files={others.map((f) => ({ file: f.path }))}
            payload={payload}
            openFile={props.openFile}
            comments={comments}
            current={props.current}
          />
          <LineComments {...props} changes={others.map((f) => ({ file: f.path }))} />
        </>
      ) : null}
      {props.footer ? createPortal(<FileFinder {...props} claimed={claimed} />, props.footer) : null}
    </div>
  );
}

/** The change's files, a drawer at the bottom of the overview: closed, a
 *  bar that names the chosen file; open, each file with its line counts and
 *  how much of it is off the map, filtered as one types. A file chosen
 *  opens in the drawer under the map, and the map shows what touches it. */
function FileFinder(props: PanelProps & { claimed: Set<string> }) {
  const { payload, chosenFile: found, chooseFile, claimed } = props;
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState(false);
  const shown = payload.files.filter((f) => f.path.toLowerCase().includes(filter.trim().toLowerCase()));
  // what on the map touches the chosen file, named as the map names it
  const owners = found
    ? [
        ...payload.nodes.filter((n) => n.changes?.some((c) => c.file === found)).map((n) => n.title),
        ...(payload.edges ?? [])
          .filter((e) => e.changes?.some((c) => c.file === found))
          .map((e) => e.label ?? e.kind ?? "call"),
      ]
    : [];
  const listed = owners.length > 1 ? `${owners.slice(0, -1).join(", ")} and ${owners[owners.length - 1]}` : owners[0];
  return (
    <section className={`cm-finder${open ? " cm-finder-open" : ""}`} data-testid="files">
      <button type="button" className="cm-finder-toggle" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="pinrail-eyebrow">Files · {payload.files.length}</span>
        {!open && found ? <span className="cm-finder-chosen mono">{found}</span> : null}
        {open ? <ChevronDown /> : <ChevronUp />}
      </button>
      {open ? (
        <>
          <label className="cm-finder-search">
            <Search aria-hidden="true" />
            <input
              className="pinrail-field cm-finder-filter"
              type="search"
              placeholder="Filter files"
              aria-label="Filter files"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </label>
          <ul className="cm-parts cm-finder-list">
            {shown.map((f) => {
              const { added, removed } = countLines(f.diff);
              const missing = props.coverage.unmapped.get(f.path);
              const off = missing ? missing.added.size + missing.removed.size : 0;
              const nowhere = !claimed.has(f.path) && !routine(f.path);
              return (
                <li key={f.path}>
                  <button
                    type="button"
                    className={`cm-row cm-file${found === f.path ? " cm-current" : ""}`}
                    aria-pressed={found === f.path}
                    onClick={() => chooseFile(found === f.path ? null : f.path)}
                    title={f.path}
                  >
                    <FileCode />
                    <span className="cm-row-title mono">{f.path}</span>
                    {nowhere ? (
                      <span className="cm-file-off">not on the map</span>
                    ) : off ? (
                      <span className="cm-file-off">{off} off map</span>
                    ) : null}
                    <span className="cm-counts mono">
                      <span className="cm-add">+{added}</span> <span className="cm-del">−{removed}</span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          {found && owners.length ? (
            <p className="cm-finder-owners" data-testid="file-owners">
              On the map: {listed}
            </p>
          ) : null}
        </>
      ) : null}
    </section>
  );
}

const diagramIcon = (d: ExtraDiagram) => (d.kind === "sequence" ? ListOrdered : GitFork);
const diagramKind = (d: ExtraDiagram) => (d.kind === "sequence" ? "Sequence" : "State diagram");

/** A row per extra diagram, which shows it on the canvas. */
function DiagramList({
  payload,
  diagrams,
  openDiagram,
  comments,
}: {
  payload: Payload;
  diagrams: ExtraDiagram[];
  openDiagram: PanelProps["openDiagram"];
  comments: Comment[];
}) {
  return (
    <ul className="cm-parts cm-marked-rows">
      {diagrams.map((d) => {
        const Icon = diagramIcon(d);
        const count = diagramComments(payload, d, comments);
        return (
          <li key={d.id}>
            <button type="button" className="cm-row" onClick={() => openDiagram(d.id)} data-diagram-row={d.id}>
              <Icon />
              <span className="cm-row-title">{d.title}</span>
              {count ? (
                <span className="cm-mark cm-mark-comment" title={`${count} ${count === 1 ? "comment" : "comments"}`}>
                  <MessageCircle />
                  {count}
                </span>
              ) : null}
              <span className="pinrail-faint cm-row-end">{diagramKind(d)}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** The extra diagrams a part or an interaction of the map appears in. */
function AlsoIn({
  payload,
  node,
  edge,
  openDiagram,
}: Pick<PanelProps, "payload" | "openDiagram"> & { node?: string; edge?: string }) {
  const found = (payload.diagrams ?? []).filter(
    (d) => (node && d.participants?.some((p) => p.node === node)) || (edge && d.messages?.some((m) => m.edge === edge)),
  );
  if (!found.length) return null;
  return (
    <>
      <h3 className="pinrail-eyebrow">Also in</h3>
      <DiagramList payload={payload} diagrams={found} openDiagram={openDiagram} comments={[]} />
    </>
  );
}

/** Whether a file row is the file in the diff drawer: the same file and
 *  ranges, or the same file when the drawer opened it without ranges. */
const isCurrent = (current: FileChange | null, c: FileChange) =>
  !!current && (sameChange(current, c) || (!current.lines?.length && current.file === c.file));

function Files({
  files,
  payload,
  openFile,
  comments,
  current,
}: {
  files: FileChange[];
  payload: Payload;
  openFile: PanelProps["openFile"];
  comments: Comment[];
  current: FileChange | null;
}) {
  return (
    <ul className="cm-files">
      {files.map((c) => {
        const file = payload.files.find((f) => f.path === c.file);
        const { added, removed } = file ? countLines(file.diff) : { added: 0, removed: 0 };
        const commented = comments.filter((x) => isLine(x.target) && x.target.file === c.file).length;
        return (
          <li key={c.file + JSON.stringify(c.lines ?? [])}>
            <button
              type="button"
              className={`cm-row cm-file${isCurrent(current, c) ? " cm-current" : ""}`}
              aria-current={isCurrent(current, c) ? "true" : undefined}
              onClick={() => openFile(c.file, c)}
              disabled={!file}
              title={c.file}
            >
              <FileCode />
              {/* the path, and under it the line ranges, so that the ranges
                  never take the path's room */}
              <span className="cm-file-name">
                <span className="cm-row-title mono">{c.file}</span>
                {c.lines?.length ? (
                  <span className="pinrail-faint mono cm-file-lines">
                    {c.lines.map(([a, b]) => (a === b ? `${a}` : `${a}–${b}`)).join(", ")}
                  </span>
                ) : null}
              </span>
              {commented ? (
                <span
                  className="cm-mark cm-mark-comment"
                  title={`${commented} ${commented === 1 ? "comment" : "comments"} on its lines`}
                >
                  <MessageCircle />
                  {commented}
                </span>
              ) : null}
              <span className="cm-counts mono">
                <span className="cm-add">+{added}</span> <span className="cm-del">−{removed}</span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** A part's screens, one at a time: previous and next step through them,
 *  and follow the viewer while it shows one of them. */
function Screens({
  part,
  images,
  screen,
  openScreen,
  comments,
}: { part: Part } & Pick<PanelProps, "images" | "screen" | "openScreen" | "comments">) {
  const [shown, setShown] = useState(0);
  const all = part.screens ?? [];
  if (!all.length) return null;
  const i = Math.min(screen ?? shown, all.length - 1);
  const s = all[i];
  const go = (to: number) => {
    setShown(to);
    // the viewer shows the same screen
    if (screen !== null) openScreen(to);
  };
  const count = screenComments(comments, part.id, i).length;
  return (
    <section className="cm-screens">
      <div className="cm-screens-head">
        <span className="cm-screens-title">{s.title}</span>
        {count ? (
          <span className="cm-mark cm-mark-comment" title="Comments on areas of this screen">
            <MessageCircle />
            {count}
          </span>
        ) : null}
        {all.length > 1 ? (
          <span className="cm-screens-nav">
            <button
              type="button"
              className="pinrail-btn pinrail-btn-ghost"
              aria-label="Previous screen"
              title="Previous screen"
              disabled={i === 0}
              onClick={() => go(i - 1)}
            >
              <ChevronLeft />
            </button>
            <span className="pinrail-faint" data-testid="panel-screen-position">
              {i + 1} of {all.length}
            </span>
            <button
              type="button"
              className="pinrail-btn pinrail-btn-ghost"
              aria-label="Next screen"
              title="Next screen"
              disabled={i === all.length - 1}
              onClick={() => go(i + 1)}
            >
              <ChevronRight />
            </button>
          </span>
        ) : null}
      </div>
      <figure
        className={`cm-screen${screen === i ? " cm-current" : ""}`}
        role="button"
        tabIndex={0}
        aria-current={screen === i ? "true" : undefined}
        aria-label={`Show ${s.title ?? "the screen"} larger`}
        title="Show it larger"
        onClick={() => openScreen(i)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            openScreen(i);
          }
        }}
      >
        {s.video ? (
          // a recording: its first frame, with a play mark
          <div className="cm-screen-pair cm-single">
            <div>
              <span className="pinrail-eyebrow">Recording</span>
              <span className="cm-screen-frame">
                {images.get(s.video.$attachment) ? (
                  <video
                    src={`${images.get(s.video.$attachment)}#t=0.1`}
                    aria-label={`${s.title ?? part.title}, recording`}
                    muted
                    playsInline
                    preload="metadata"
                  />
                ) : null}
                <span className="cm-screen-play" aria-hidden="true">
                  <Play />
                </span>
              </span>
            </div>
          </div>
        ) : null}
        <div className={`cm-screen-pair${s.before && s.after ? "" : " cm-single"}`} hidden={!!s.video}>
          {s.before ? (
            <div>
              <span className="pinrail-eyebrow">Before</span>
              {images.get(s.before.$attachment) ? (
                <img src={images.get(s.before.$attachment)} alt={`${s.title ?? part.title}, before`} />
              ) : null}
            </div>
          ) : null}
          {s.after ? (
            <div>
              <span className="pinrail-eyebrow">After</span>
              {images.get(s.after.$attachment) ? (
                <img src={images.get(s.after.$attachment)} alt={`${s.title ?? part.title}, after`} />
              ) : null}
            </div>
          ) : null}
        </div>
      </figure>
    </section>
  );
}

/** The comments on the lines of some files, each with its line, which opens the diff there. */
function LineComments({
  changes,
  payload,
  comments,
  readonly,
  openFile,
  removeComment,
  editComment,
}: Pick<PanelProps, "payload" | "comments" | "readonly" | "openFile" | "removeComment" | "editComment"> & {
  changes: FileChange[];
}) {
  const on = comments.filter(
    (c): c is Comment & { target: LineTarget } => isLine(c.target) && covers(changes, c.target),
  );
  if (!on.length) return null;
  return (
    <div className="cm-line-list" data-testid="line-comments">
      {on.map((c, i) => {
        const text = linesText(payload, c.target);
        const name = c.target.file.split("/").pop();
        return (
          <CommentItem
            key={i}
            className="cm-comment-line"
            comment={c}
            readonly={readonly}
            language={window.Pinrail.languageOf(c.target.file)}
            onEdit={(body) => editComment(c, body)}
            onDelete={() => removeComment(c)}
            head={
              // the line opens the diff there; the code is a block of its own
              // beside the button, since WebKit leaves a scrolling block out of
              // a button's height
              <div
                className="cm-line-head"
                onClick={(e) => {
                  e.stopPropagation();
                  openFile(c.target.file, undefined, c.target);
                }}
              >
                <button type="button" className="cm-line-ref" title={`Open ${c.target.file} at line ${c.target.line}`}>
                  <span className="mono">
                    {name}:{c.target.start_line ? `${c.target.start_line}–${c.target.line}` : c.target.line}
                    {(c.target.side ?? "new") === "old" ? " (old)" : ""}
                  </span>
                </button>
                {text.length ? (
                  <pre className="cm-line-text">
                    {highlightLines(dedent(text.slice(0, SNIPPET)), c.target.file).map((html, k) => (
                      <code key={k} dangerouslySetInnerHTML={{ __html: html || " " }} />
                    ))}
                    {text.length > SNIPPET ? (
                      <span className="pinrail-faint cm-more">and {text.length - SNIPPET} more lines</span>
                    ) : null}
                  </pre>
                ) : null}
              </div>
            }
          />
        );
      })}
    </div>
  );
}

/** The comments on a target, and a field to add one. */
export function Comments({
  target,
  comments,
  readonly,
  addComment,
  removeComment,
  editComment,
  composeAt,
  footer,
  payload,
  openFile,
  openScreen,
  changes,
  part,
}: Pick<
  PanelProps,
  | "comments"
  | "readonly"
  | "addComment"
  | "removeComment"
  | "editComment"
  | "composeAt"
  | "footer"
  | "payload"
  | "openFile"
  | "openScreen"
> & {
  target: Target;
  /** the files, or their ranges, whose line comments show here too */
  changes?: FileChange[];
  /** the part whose screens' area comments show here too */
  part?: Part;
}) {
  const dock = (field: ReactNode) => (footer ? createPortal(field, footer) : field);
  const key = keyOf(target);
  const mine = comments.filter((c) => keyOf(c.target) === key);
  const [text, setText] = useState("");
  const id = `comment-${key.replace(/[^A-Za-z0-9_-]/g, "-")}`;
  const add = () => {
    if (!text.trim()) return;
    addComment(target, text.trim());
    setText("");
  };
  return (
    <section className="cm-comments">
      <h3 className="pinrail-eyebrow">Comments</h3>
      {mine.map((c, i) => (
        <CommentItem
          key={i}
          comment={c}
          readonly={readonly}
          onEdit={(body) => editComment(c, body)}
          onDelete={() => removeComment(c)}
        />
      ))}
      {part
        ? (part.screens ?? []).flatMap((sc, index) =>
            screenComments(comments, part.id, index).map((c, n) => (
              <CommentItem
                key={`${index}-${n}`}
                className="cm-comment-line"
                comment={c}
                readonly={readonly}
                onEdit={(body) => editComment(c, body)}
                onDelete={() => removeComment(c)}
                head={
                  <button
                    type="button"
                    className="cm-line-ref cm-screen-ref"
                    onClick={(e) => {
                      e.stopPropagation();
                      openScreen(index, c.target.time);
                    }}
                    aria-label={
                      c.target.screen.image === "video"
                        ? "Show the recording at this moment"
                        : "Show the screenshot with this area"
                    }
                    title={
                      c.target.screen.image === "video"
                        ? "Show the recording at this moment"
                        : "Show the screenshot with this area"
                    }
                  >
                    {/* the number of the area's box on the screenshot */}
                    <span className="cm-area-n">{n + 1}</span>
                    {c.target.screen.image === "video" ? (
                      <>
                        <Film aria-hidden="true" />
                        <span>
                          Recording · {sc.title ?? part.title} · {clock(c.target.time ?? 0)}
                        </span>
                      </>
                    ) : (
                      <>
                        <ImageIcon aria-hidden="true" />
                        <span>
                          Screenshot · {sc.title ?? part.title} ·{" "}
                          {c.target.screen.image === "before" ? "Before" : "After"}
                        </span>
                      </>
                    )}
                  </button>
                }
              />
            )),
          )
        : null}
      <LineComments
        changes={changes ?? []}
        payload={payload}
        comments={comments}
        readonly={readonly}
        openFile={openFile}
        removeComment={removeComment}
        editComment={editComment}
      />
      {readonly ? (
        mine.length ||
        comments.some((c) => isLine(c.target) && covers(changes, c.target)) ||
        (part && comments.some((c) => isScreen(c.target) && c.target.screen.node === part.id)) ? null : (
          <p className="pinrail-faint">No comments.</p>
        )
      ) : (
        // in the panel's footer, under the part that scrolls
        dock(
          <Compose
            key={composeAt}
            id={id}
            value={text}
            onChange={setText}
            onSubmit={add}
            placeholder="Add a comment"
            inputRef={(el) => {
              if (el && composeAt) el.focus();
            }}
          />,
        )
      )}
    </section>
  );
}

function ReviewedToggle({
  id,
  reviewed,
  readonly,
  toggleReviewed,
}: { id: string } & Pick<PanelProps, "readonly" | "toggleReviewed"> & { reviewed: Set<string> }) {
  const on = reviewed.has(id);
  if (readonly)
    return on ? (
      <span className="cm-reviewed-tag">
        <Check /> Reviewed
      </span>
    ) : null;
  return (
    <button
      type="button"
      className="pinrail-btn"
      aria-pressed={on}
      onClick={() => toggleReviewed(id)}
      title="Mark as reviewed (R)"
    >
      <Check /> {on ? "Reviewed" : "Mark as reviewed"}
    </button>
  );
}

function PartDetail(props: PanelProps & { part: Part }) {
  const { part, payload, select, openFile, images, reviewed } = props;
  const Icon = kindIcon[part.kind ?? "module"];
  const status = nodeStatus(part);
  const links = (payload.edges ?? []).filter((e) => e.from === part.id || e.to === part.id);
  return (
    <div className="cm-detail" data-testid="part-detail">
      <div className="cm-detail-head">
        <h2 className="cm-detail-name">
          <span className="cm-part-icon">
            <Icon />
          </span>
          <span>{part.title}</span>
        </h2>
        <div className="cm-detail-meta">
          <span className="pinrail-faint">
            {kindLabel[part.kind ?? "module"]} ·{" "}
            <span className={`cm-status cm-status-${status}`}>{statusLabel[status]}</span>
          </span>
          <ReviewedToggle
            id={part.id}
            reviewed={reviewed}
            readonly={props.readonly}
            toggleReviewed={props.toggleReviewed}
          />
        </div>
      </div>
      <Markdown text={part.summary} />
      <Screens
        part={part}
        images={images}
        screen={props.screen}
        openScreen={props.openScreen}
        comments={props.comments}
      />
      {part.changes?.length ? (
        <>
          <h3 className="pinrail-eyebrow">Files</h3>
          <Files
            files={part.changes}
            payload={payload}
            openFile={openFile}
            comments={props.comments}
            current={props.current}
          />
        </>
      ) : null}
      {links.length ? (
        <>
          <h3 className="pinrail-eyebrow">Interactions</h3>
          <ul className="cm-parts">
            {links.map((e) => {
              const other = payload.nodes.find((n) => n.id === (e.from === part.id ? e.to : e.from));
              return (
                <li key={e.id}>
                  <button type="button" className="cm-row" onClick={() => select({ edge: e.id })}>
                    <span className="cm-row-title">
                      {e.from === part.id ? "→ " : "← "}
                      {other?.title ?? "?"}
                    </span>
                    <span className="pinrail-faint">{e.label ?? e.kind}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      ) : null}
      <AlsoIn payload={payload} node={part.id} openDiagram={props.openDiagram} />
      <Comments
        key={keyOf({ node: part.id })}
        {...props}
        target={{ node: part.id }}
        changes={part.changes}
        part={part}
      />
    </div>
  );
}

function LinkDetail(props: PanelProps & { link: Link }) {
  const { link, payload, select, openFile, reviewed } = props;
  const from = payload.nodes.find((n) => n.id === link.from);
  const to = payload.nodes.find((n) => n.id === link.to);
  const status = edgeStatus(link);
  return (
    <div className="cm-detail" data-testid="link-detail">
      <div className="cm-detail-head">
        <h2 className="cm-detail-name">
          <span>{link.label ?? "Interaction"}</span>
        </h2>
        <div className="cm-detail-meta">
          <span className="pinrail-faint">
            {link.kind ?? "call"} · <span className={`cm-status cm-status-${status}`}>{statusLabel[status]}</span>
          </span>
          <ReviewedToggle
            id={link.id}
            reviewed={reviewed}
            readonly={props.readonly}
            toggleReviewed={props.toggleReviewed}
          />
        </div>
      </div>
      <p className="cm-ends">
        <button type="button" className="pinrail-btn pinrail-btn-ghost" onClick={() => select({ node: link.from })}>
          {from?.title ?? link.from}
        </button>
        <ArrowRight />
        <button type="button" className="pinrail-btn pinrail-btn-ghost" onClick={() => select({ node: link.to })}>
          {to?.title ?? link.to}
        </button>
      </p>
      <Markdown text={link.summary} />
      {link.detail?.request ? (
        <>
          <h3 className="pinrail-eyebrow">Request</h3>
          <pre className="cm-code">{link.detail.request}</pre>
        </>
      ) : null}
      {link.detail?.response ? (
        <>
          <h3 className="pinrail-eyebrow">Response</h3>
          <pre className="cm-code">{link.detail.response}</pre>
        </>
      ) : null}
      {link.changes?.length ? (
        <>
          <h3 className="pinrail-eyebrow">Files</h3>
          <Files
            files={link.changes}
            payload={payload}
            openFile={openFile}
            comments={props.comments}
            current={props.current}
          />
        </>
      ) : null}
      <AlsoIn payload={payload} edge={link.id} openDiagram={props.openDiagram} />
      <Comments key={keyOf({ edge: link.id })} {...props} target={{ edge: link.id }} changes={link.changes} />
    </div>
  );
}

const itemKind: Record<Found["kind"], string> = {
  participant: "Participant",
  message: "Message",
  state: "State",
  transition: "Transition",
};

/** An entity of an extra diagram that has no part or interaction of the map. */
function ItemDetail(props: PanelProps & { found: Found }) {
  const { found, payload, select, openFile, reviewed } = props;
  const { diagram, item } = found;
  const sel = { diagram: diagram.id, item: item.id };
  const raw = item.status ?? "changed";
  const status = found.kind === "message" || found.kind === "transition" ? raw : raw === "unchanged" ? "context" : raw;
  // a message's or a transition's two ends, which select them
  const ends =
    found.kind === "message" || found.kind === "transition"
      ? [found.item.from, found.item.to].map((id) => {
          const end =
            found.kind === "message"
              ? diagram.participants?.find((p) => p.id === id)
              : diagram.states?.find((s) => s.id === id);
          return end
            ? {
                to: selectionOf(payload, diagram.id, end),
                title: itemTitle(
                  found.kind === "message"
                    ? { diagram, kind: "participant", item: end as NonNullable<ExtraDiagram["participants"]>[number] }
                    : { diagram, kind: "state", item: end as NonNullable<ExtraDiagram["states"]>[number] },
                ),
              }
            : { to: null, title: id };
        })
      : null;
  const Icon = diagramIcon(diagram);
  return (
    <div className="cm-detail" data-testid="item-detail">
      <div className="cm-detail-head">
        <h2 className="cm-detail-name">
          <span>{itemTitle(found)}</span>
        </h2>
        <div className="cm-detail-meta">
          <span className="pinrail-faint">
            {itemKind[found.kind]}
            {found.kind === "message" && found.item.kind && found.item.kind !== "call"
              ? ` (${found.item.kind})`
              : ""} · <span className={`cm-status cm-status-${status}`}>{statusLabel[status]}</span>
          </span>
          <ReviewedToggle
            id={markOf(sel)}
            reviewed={reviewed}
            readonly={props.readonly}
            toggleReviewed={props.toggleReviewed}
          />
        </div>
      </div>
      <button type="button" className="cm-in-diagram" onClick={() => props.openDiagram(diagram.id)}>
        <Icon /> {diagramKind(diagram)}: {diagram.title}
      </button>
      {ends ? (
        <p className="cm-ends">
          {ends.map((end, i) => (
            <span key={i} className="cm-end">
              {i ? <ArrowRight /> : null}
              <button
                type="button"
                className="pinrail-btn pinrail-btn-ghost"
                disabled={!end.to}
                onClick={() => end.to && select(end.to)}
              >
                {end.title}
              </button>
            </span>
          ))}
        </p>
      ) : null}
      <Markdown text={item.summary} />
      {item.changes?.length ? (
        <>
          <h3 className="pinrail-eyebrow">Files</h3>
          <Files
            files={item.changes}
            payload={payload}
            openFile={openFile}
            comments={props.comments}
            current={props.current}
          />
        </>
      ) : null}
      <Comments key={keyOf(sel)} {...props} target={sel} changes={item.changes} />
    </div>
  );
}

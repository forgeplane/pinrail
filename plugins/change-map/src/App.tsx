// The change map: the diagram, and the panel beside it. The verdict is the
// person's choice in the header; until they make one, comments ask for
// changes and none approve. main.tsx connects to the app and draws this
// with each init.
import { GitFork, ListOrdered, Network, PanelBottom } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Init, Plugin, Settings } from "../pinrail-plugin";
import { Diagram, type Marks } from "./Diagram";
import { submitOpenFields } from "./Compose";
import { DiffViewer, savedHeight } from "./DiffViewer";
import type { CardStyle, Direction } from "./layout";
import { coverage } from "./lines";
import { ScreenViewer, type ScreenMode } from "./ScreenViewer";
import { Panel } from "./Panel";
import { SequenceDiagram } from "./SequenceDiagram";
import { StateDiagram } from "./StateDiagram";
import {
  covers,
  isLine,
  isItem,
  isScreen,
  changesOf,
  namedAsCode,
  diagramComments,
  edgeStatus,
  markOf,
  nodeStatus,
  narrowest,
  ownItems,
  sameSelection,
  selectionOf,
  problemsOf,
  titleOf,
  type Comment,
  type Decision,
  type Draft,
  type FileChange,
  type LineTarget,
  type Payload,
  type Selection,
  type Target,
  type Verdict,
} from "./types";

export type { Decision, Payload } from "./types";

/** What the connection in main.tsx asks of the view on screen. */
export const view = {
  collect: (): Decision | undefined => undefined,
  /** the plugin's settings changed, in the app or through the view */
  settings: (_settings: Settings): void => undefined,
};

/** The side panel's width at its narrowest, which it opens at by default. */
const PANEL_MIN = 340;
/** The room the diagram keeps beside a wide side panel. */
const CANVAS_MIN = 360;

/** A draft kept by an earlier release may have another shape: use only what reads as this one's. */
function draftOf(kept: unknown): Draft {
  const d = (kept && typeof kept === "object" ? kept : {}) as Partial<Draft>;
  return {
    verdict: d.verdict === "approve" || d.verdict === "request_changes" ? d.verdict : null,
    comments: Array.isArray(d.comments) ? d.comments : [],
    reviewed: Array.isArray(d.reviewed) ? d.reviewed : [],
  };
}

const verdictOf = (draft: Draft): Verdict => draft.verdict ?? (draft.comments.length ? "request_changes" : "approve");

export function App({ plugin, init }: { plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision> }) {
  const { review, readonly } = init;
  const payload = review.payload;
  const decided = review.decision?.data;
  const [draft, setDraft] = useState<Draft>(() =>
    readonly && decided
      ? { verdict: decided.verdict, comments: decided.comments ?? [], reviewed: decided.reviewed ?? [] }
      : draftOf(init.draft),
  );
  const [selection, setSelection] = useState<Selection>(null);
  // the diagram on the canvas: the map, or one of the extra diagrams by its id
  const [tab, setTab] = useState<string | null>(null);
  // a moment of a recording the panel asked to show
  const [seek, setSeek] = useState<{ time: number; n: number } | null>(null);
  // a part to zoom the map in on, counted so that the same part again zooms again
  const [zoom, setZoom] = useState<{ node: string; n: number } | null>(null);
  const diagram = payload.diagrams?.find((d) => d.id === tab) ?? null;
  // the file in the diff drawer; null path: the selection has no files to show
  // it opens as the person left it, which the show_diff setting keeps
  // the drawer: a file of the selection, or one of the selected part's screens
  const [file, setFile] = useState<{
    path: string | null;
    change?: FileChange;
    at?: LineTarget;
    /** a screen of the selected part, shown in place of a file */
    screen?: number;
  } | null>(() => (init.settings?.show_diff === true && payload.files[0] ? { path: payload.files[0].path } : null));
  const shown = !!file;
  const kept = useRef(init.settings?.show_diff === true);
  useEffect(() => {
    if (shown === kept.current) return;
    kept.current = shown;
    plugin.setSetting("show_diff", shown).catch(() => undefined);
  }, [shown, plugin]);
  // the side panel's width, as the person dragged it last
  const widthOf = (settings?: Settings) =>
    typeof settings?.panel_width === "number" ? Math.max(PANEL_MIN, settings.panel_width) : PANEL_MIN;
  const [panelWidth, setPanelWidth] = useState(() => widthOf(init.settings));
  // the panel's footer, where the comment field goes
  const [footer, setFooter] = useState<HTMLDivElement | null>(null);
  /** Resizes the side panel by dragging its left edge, and keeps the width
   *  as a setting when the person lets go. The diagram keeps at least
   *  CANVAS_MIN pixels. */
  const resizePanel = (event: React.PointerEvent<HTMLElement>) => {
    event.preventDefault();
    const start = event.clientX;
    const from = panelWidth;
    const widest = Math.max(PANEL_MIN, window.innerWidth - CANVAS_MIN);
    const at = (x: number) => Math.round(Math.min(widest, Math.max(PANEL_MIN, from + start - x)));
    const onMove = (e: PointerEvent) => setPanelWidth(at(e.clientX));
    const onUp = (e: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      document.body.classList.remove("cm-resizing-panel");
      const width = at(e.clientX);
      setPanelWidth(width);
      plugin.setSetting("panel_width", width).catch(() => undefined);
    };
    document.body.classList.add("cm-resizing-panel");
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  // how screenshots compare, as the person chose last
  const modeOf = (settings?: Settings): ScreenMode =>
    settings?.screen_mode === "wipe" || settings?.screen_mode === "after" ? settings.screen_mode : "side";
  const [screenMode, setScreenModeState] = useState<ScreenMode>(() => modeOf(init.settings));
  const setScreenMode = (m: ScreenMode) => {
    setScreenModeState(m);
    plugin.setSetting("screen_mode", m).catch(() => undefined);
  };
  // how the map draws its parts, detailed unless the setting says compact
  const cardsOf = (settings?: Settings): CardStyle => (settings?.card_style === "compact" ? "compact" : "detailed");
  const [cards, setCardsState] = useState<CardStyle>(() => cardsOf(init.settings));
  const setCards = (style: CardStyle) => {
    setCardsState(style);
    plugin.setSetting("card_style", style).catch(() => undefined);
  };
  // which way the state diagrams run, left to right unless the setting says otherwise
  const directionOf = (settings?: Settings): Direction => (settings?.state_direction === "TB" ? "TB" : "LR");
  const [direction, setDirectionState] = useState<Direction>(() => directionOf(init.settings));
  const setDirection = (d: Direction) => {
    setDirectionState(d);
    plugin.setSetting("state_direction", d).catch(() => undefined);
  };
  // a change in the app's settings opens or closes the drawer, and sets the comparison
  view.settings = (settings) => {
    setScreenModeState(modeOf(settings));
    setCardsState(cardsOf(settings));
    setDirectionState(directionOf(settings));
    setPanelWidth(widthOf(settings));
    const want = settings.show_diff === true;
    kept.current = want;
    setFile((open) => (want ? (open ?? (payload.files[0] ? { path: payload.files[0].path } : null)) : null));
  };
  // With the drawer open, selecting a part or an interaction shows its
  // first file, or says it has none
  // the selection on screen, read when a new one is chosen
  const shownRef = useRef(selection);
  shownRef.current = selection;
  // what the panel showed before, for its Back link
  const [history, setHistory] = useState<Selection[]>([]);
  /** Shows a selection, without adding to the history. */
  const show = useCallback(
    (next: Selection) => {
      setSelection(next);
      if (!next) return;
      // an entity of an extra diagram shows on its diagram
      if (isItem(next)) setTab(next.diagram);
      setFile((open) => {
        if (!open) return open;
        const first = changesOf(payload, next)[0];
        return first ? { path: first.file, change: first } : { path: null };
      });
    },
    [payload],
  );
  const select = useCallback(
    (next: Selection) => {
      const now = shownRef.current;
      if (!sameSelection(now, next)) setHistory((h) => [...h, now].slice(-50));
      show(next);
    },
    [show],
  );
  const back = useCallback(() => {
    const previous = history[history.length - 1];
    if (previous === undefined) return;
    setHistory((h) => h.slice(0, -1));
    show(previous);
  }, [history, show]);
  const selectedPart =
    selection && "node" in selection ? payload.nodes.find((n) => n.id === selection.node) : undefined;
  const siblings = useMemo(() => changesOf(payload, selection), [payload, selection]);
  // how much of the change the map covers, and the lines it leaves out
  const covered = useMemo(() => coverage(payload), [payload]);
  // the file chosen in the overview's files, and what on the map touches it;
  // choosing a part or an interaction leaves it
  const [found, setFound] = useState<string | null>(null);
  useEffect(() => {
    if (selection) setFound(null);
  }, [selection]);
  const findFile = (path: string | null) => {
    setFound(path);
    if (path) setFile({ path });
  };
  const lit = useMemo(
    () =>
      found
        ? {
            nodes: new Set(payload.nodes.filter((n) => n.changes?.some((c) => c.file === found)).map((n) => n.id)),
            edges: new Set(
              (payload.edges ?? []).filter((e) => e.changes?.some((c) => c.file === found)).map((e) => e.id),
            ),
            // the diagrams' own: those standing for a part or an interaction follow it
            items: new Set(
              (payload.diagrams ?? []).flatMap((d) =>
                [...(d.participants ?? []), ...(d.messages ?? []), ...(d.states ?? []), ...(d.transitions ?? [])]
                  .filter((x) => x.changes?.some((c) => c.file === found))
                  .map((x) => `${d.id}:${x.id}`),
              ),
            ),
          }
        : null,
    [found, payload],
  );
  /** Opens the diff drawer on the selection's first file, or the change's
   *  first file when nothing is selected; closes it when it is open. */
  const toggleDiff = () =>
    setFile((open) => {
      if (open) return null;
      if (selection) {
        const first = changesOf(payload, selection)[0];
        return first ? { path: first.file, change: first } : { path: null };
      }
      return payload.files[0] ? { path: payload.files[0].path } : null;
    });
  const [fit, setFit] = useState(0);
  const [drawer, setDrawer] = useState(savedHeight);
  // a count that the C key raises, which focuses the selection's comment field
  const [composeAt, setComposeAt] = useState(0);
  const problems = useMemo(() => problemsOf(payload), [payload]);
  const verdict = verdictOf(draft);

  // the draft survives a reload, and the hand-over says what it will do.
  // The latest draft is also kept in a ref, which the hand-over reads right
  // after the open comment fields add their text.
  const latest = useRef(draft);
  const change = useCallback(
    (next: (d: Draft) => Draft) => {
      const out = next(latest.current);
      latest.current = out;
      setDraft(out);
      if (!readonly) plugin.draft(out);
    },
    [plugin, readonly],
  );
  const commented = draft.comments.length > 0;
  useEffect(() => {
    if (readonly) return;
    plugin.handOverLabel(
      verdict === "request_changes" ? "Request changes" : commented ? "Approve with comments" : "Approve",
    );
  }, [plugin, readonly, verdict, commented]);
  view.collect = () => {
    submitOpenFields();
    const d = latest.current;
    return { verdict: verdictOf(d), comments: d.comments, ...(d.reviewed.length ? { reviewed: d.reviewed } : {}) };
  };

  // the screens' addresses, screenshots and recordings, once the app has them
  const [images, setImages] = useState<Map<string, string>>(new Map());
  useEffect(() => {
    let cancelled = false;
    const names = new Set(
      payload.nodes.flatMap((n) =>
        (n.screens ?? []).flatMap((s) => [s.before, s.after, s.video].filter(Boolean).map((a) => a!.$attachment)),
      ),
    );
    Promise.all(
      [...names].map((name) =>
        plugin
          .attachmentUrl(name)
          .then((url) => [name, url] as const)
          .catch(() => null),
      ),
    ).then((pairs) => {
      if (!cancelled) setImages(new Map(pairs.filter((p): p is readonly [string, string] => !!p)));
    });
    return () => {
      cancelled = true;
    };
  }, [payload, plugin]);

  const reviewed = useMemo(() => new Set(draft.reviewed), [draft.reviewed]);
  // what there is to review: the parts and the interactions the change
  // adds, changes or removes, the parts first, in the order N walks them
  const toReview = useMemo<NonNullable<Selection>[]>(
    () => [
      ...payload.nodes.filter((n) => nodeStatus(n) !== "context").map((n) => ({ node: n.id })),
      ...(payload.edges ?? []).filter((e) => edgeStatus(e) !== "unchanged").map((e) => ({ edge: e.id })),
    ],
    [payload],
  );
  const done = toReview.filter((s) => reviewed.has(markOf(s))).length;
  /** Selects the next part or interaction not yet reviewed, after the
   *  selection, on the map. */
  const nextToReview = () => {
    const at = toReview.findIndex((s) => sameSelection(s, selection));
    const order = [...toReview.slice(at + 1), ...toReview.slice(0, at + 1)];
    const next = order.find((s) => !reviewed.has(markOf(s)));
    if (!next) return;
    select(next);
    setTab(null);
    if ("node" in next) setZoom((z) => ({ node: next.node, n: (z?.n ?? 0) + 1 }));
  };
  const nextRef = useRef(nextToReview);
  nextRef.current = nextToReview;
  /** The badges. `marks` counts a line comment on every part and
   *  interaction whose files cover it, for the overview's list of parts.
   *  `canvasMarks` counts it once, on the entity of the diagram on screen
   *  that `narrowest` picks, so that a diagram's badges add up to its tab's
   *  count. */
  const marksFor = useCallback(
    (owners: { id: string; changes?: FileChange[] }[], every: boolean): Marks => {
      const out: Marks = new Map();
      const at = (id: string) => out.get(id) ?? out.set(id, { comments: 0, reviewed: false }).get(id)!;
      for (const c of draft.comments) {
        if ("node" in c.target) at(c.target.node).comments++;
        else if ("edge" in c.target) at(c.target.edge).comments++;
        else if (isItem(c.target)) at(markOf(c.target)).comments++;
        else if (isLine(c.target)) {
          if (every) {
            for (const x of owners) if (covers(x.changes, c.target)) at(x.id).comments++;
          } else {
            const id = narrowest(owners, c.target);
            if (id) at(id).comments++;
          }
        } else if (isScreen(c.target)) at(c.target.screen.node).comments++;
      }
      for (const id of draft.reviewed) at(id).reviewed = true;
      return out;
    },
    [draft],
  );
  const mapOwners = useMemo(() => [...payload.nodes, ...(payload.edges ?? [])], [payload]);
  const marks = useMemo(() => marksFor([...mapOwners, ...ownItems(payload)], true), [marksFor, mapOwners, payload]);
  const canvasMarks = useMemo(() => {
    if (!diagram) return marksFor(mapOwners, false);
    // the diagram's entities, as their selections name them, each once
    const owners = new Map<string, { id: string; changes?: FileChange[] }>();
    for (const item of [
      ...(diagram.participants ?? []),
      ...(diagram.messages ?? []),
      ...(diagram.states ?? []),
      ...(diagram.transitions ?? []),
    ]) {
      const sel = selectionOf(payload, diagram.id, item);
      const id = markOf(sel!);
      if (!owners.has(id)) owners.set(id, { id, changes: changesOf(payload, sel) });
    }
    return marksFor([...owners.values()], false);
  }, [diagram, marksFor, mapOwners, payload]);

  const addComment = useCallback(
    (target: Target, body: string) => change((d) => ({ ...d, comments: [...d.comments, { target, body }] })),
    [change],
  );
  const editComment = useCallback(
    (comment: Comment, body: string) =>
      change((d) => ({ ...d, comments: d.comments.map((c) => (c === comment ? { ...c, body } : c)) })),
    [change],
  );
  const removeComment = useCallback(
    (comment: Comment) => change((d) => ({ ...d, comments: d.comments.filter((c) => c !== comment) })),
    [change],
  );
  const toggleReviewed = useCallback(
    (id: string) =>
      change((d) => ({
        ...d,
        reviewed: d.reviewed.includes(id) ? d.reviewed.filter((r) => r !== id) : [...d.reviewed, id],
      })),
    [change],
  );
  const setVerdict = (v: Verdict) => change((d) => ({ ...d, verdict: v }));

  // The view's keys, whether the view has the keyboard or the app forwards
  // them: N and P walk the parts, Enter opens the selection's first file,
  // C comments on it, R marks it reviewed, and F fits the diagram. Esc is
  // the drawer's while one is open, and the app's otherwise.
  const keys = useRef({ selection, payload, readonly, diagram });
  keys.current = { selection, payload, readonly, diagram };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      const { selection: now, payload: p, readonly: ro, diagram: d } = keys.current;
      const id = now ? markOf(now) : null;
      if (e.key === "n" || e.key === "p") {
        // the map's parts, a sequence's messages in order, or a state diagram's states
        const parts: Selection[] = !d
          ? p.nodes.map((n) => ({ node: n.id }))
          : d.kind === "sequence"
            ? (d.messages ?? []).map((m) => selectionOf(p, d.id, m))
            : (d.states ?? []).filter((s) => s.kind !== "initial").map((s) => selectionOf(p, d.id, s));
        if (!parts.length) return;
        const at = parts.findIndex((x) => sameSelection(x, now));
        const next =
          at < 0
            ? e.key === "n"
              ? 0
              : parts.length - 1
            : (at + (e.key === "n" ? 1 : parts.length - 1)) % parts.length;
        select(parts[next]);
      } else if (e.key === "Enter" && now) {
        const first = changesOf(p, now)[0];
        if (!first) return;
        setFile({ path: first.file, change: first });
      } else if (e.key === "c" && now && !ro) {
        setComposeAt((n) => n + 1);
      } else if (e.key === "r" && id && !ro) {
        toggleReviewed(id);
      } else if (e.key === "f") {
        setFit((n) => n + 1);
      } else if (e.key === "u") {
        nextRef.current();
      } else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [select, toggleReviewed]);

  const canvas = {
    payload,
    selection,
    marks: canvasMarks,
    images,
    inset: file ? drawer : 0,
    fit,
    onFit: () => setFit((n) => n + 1),
    select,
    lit,
  };

  return (
    <div className={`cm-app${readonly ? " cm-readonly" : ""}`}>
      <header className="cm-header">
        <div className="cm-legend" aria-label="Legend">
          <span className="cm-status cm-status-added">new</span>
          <span className="cm-status cm-status-changed">changed</span>
          <span className="cm-status cm-status-removed">removed</span>
          <span className="cm-status cm-status-context">unchanged</span>
        </div>
        {toReview.length ? (
          <button
            type="button"
            className="pinrail-btn pinrail-btn-ghost cm-progress"
            data-testid="progress"
            onClick={nextToReview}
            disabled={done === toReview.length}
            title="Go to the next part or interaction to review (U)"
          >
            {done === toReview.length ? `All ${done} reviewed` : `${done} of ${toReview.length} reviewed`}
            <span className="cm-progress-bar" aria-hidden="true">
              <span style={{ width: `${(done / toReview.length) * 100}%` }} />
            </span>
          </button>
        ) : null}
        <span className="cm-spacer" />
        <button
          type="button"
          className="pinrail-btn pinrail-btn-ghost cm-toggle"
          aria-pressed={!!file}
          aria-label={file ? "Hide the diff" : "Show the diff"}
          title={file ? "Hide the diff" : "Show the diff"}
          onClick={toggleDiff}
        >
          <PanelBottom />
        </button>
        {readonly ? (
          <span
            className={`pinrail-tone pinrail-tone-${verdict === "approve" ? "success" : "warning"}`}
            data-testid="verdict"
          >
            {verdict === "approve" ? "Approved" : "Changes requested"}
          </span>
        ) : (
          <div className="cm-verdict" role="radiogroup" aria-label="Verdict">
            <button
              type="button"
              role="radio"
              className="pinrail-btn"
              aria-checked={verdict === "approve"}
              aria-pressed={verdict === "approve"}
              onClick={() => setVerdict("approve")}
            >
              Approve
            </button>
            <button
              type="button"
              role="radio"
              className="pinrail-btn"
              aria-checked={verdict === "request_changes"}
              aria-pressed={verdict === "request_changes"}
              onClick={() => setVerdict("request_changes")}
            >
              Request changes
            </button>
          </div>
        )}
      </header>
      {problems.length ? (
        <div className="pinrail-notice pinrail-notice-warning cm-problems" role="status">
          {problems.map((p) => (
            <div key={p}>{p}</div>
          ))}
        </div>
      ) : null}
      <div className="cm-body">
        <div className="cm-canvas">
          {payload.diagrams?.length ? (
            <div className="cm-tabs" role="tablist" aria-label="Diagrams">
              <button
                type="button"
                role="tab"
                className="cm-tab"
                aria-selected={!diagram}
                onClick={() => setTab(null)}
                data-tab="map"
              >
                <Network /> Map
              </button>
              {payload.diagrams.map((d) => {
                const Icon = d.kind === "sequence" ? ListOrdered : GitFork;
                const count = diagramComments(payload, d, draft.comments);
                return (
                  <button
                    key={d.id}
                    type="button"
                    role="tab"
                    className="cm-tab"
                    aria-selected={diagram?.id === d.id}
                    onClick={() => setTab(d.id)}
                    title={`${d.kind === "sequence" ? "Sequence" : "State diagram"}: ${d.title}`}
                    data-tab={d.id}
                  >
                    <Icon /> {d.title}
                    {count ? <span className="cm-tab-count">{count}</span> : null}
                  </button>
                );
              })}
            </div>
          ) : null}
          {diagram?.kind === "sequence" ? (
            <SequenceDiagram key={diagram.id} diagram={diagram} {...canvas} />
          ) : diagram?.kind === "state" ? (
            <StateDiagram
              key={diagram.id}
              diagram={diagram}
              {...canvas}
              direction={direction}
              setDirection={setDirection}
            />
          ) : (
            <Diagram {...canvas} zoom={zoom} cards={cards} setCards={setCards} />
          )}
          {file?.screen !== undefined && selectedPart?.screens?.[file.screen] ? (
            <ScreenViewer
              part={selectedPart}
              index={file.screen}
              images={images}
              step={(i) => setFile({ path: null, screen: i })}
              height={drawer}
              setHeight={setDrawer}
              readonly={readonly}
              comments={draft.comments}
              addComment={addComment}
              onClose={() => setFile(null)}
              mode={screenMode}
              setMode={setScreenMode}
              seek={seek}
            />
          ) : file ? (
            <DiffViewer
              key={file.path ?? ""}
              payload={payload}
              path={file.path}
              siblings={siblings}
              unmapped={file.path ? covered.unmapped.get(file.path) : undefined}
              owner={titleOf(payload, selection)}
              ownerAsCode={namedAsCode(payload, selection)}
              step={(c) => setFile({ path: c.file, change: c })}
              change={file.change}
              at={file.at}
              height={drawer}
              setHeight={setDrawer}
              readonly={readonly}
              comments={draft.comments}
              addComment={addComment}
              removeComment={removeComment}
              editComment={editComment}
              onClose={() => setFile(null)}
            />
          ) : null}
        </div>
        <div
          className="cm-panel-resize"
          onPointerDown={resizePanel}
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize the side panel"
          title="Drag to resize"
        />
        <aside className="cm-panel" aria-label="Details" style={{ width: panelWidth }}>
          <div className="cm-panel-body">
            <Panel
              payload={payload}
              marks={marks}
              selection={selection}
              readonly={readonly}
              comments={draft.comments}
              reviewed={reviewed}
              images={images}
              select={select}
              coverage={covered}
              chosenFile={found}
              chooseFile={findFile}
              zoomTo={(node) => {
                select({ node });
                setTab(null);
                setZoom((z) => ({ node, n: (z?.n ?? 0) + 1 }));
              }}
              back={
                history.length
                  ? {
                      to: history[history.length - 1] ? titleOf(payload, history[history.length - 1]) : "Overview",
                      go: back,
                    }
                  : null
              }
              screen={file?.screen ?? null}
              current={file?.path ? { file: file.path, lines: file.change?.lines } : null}
              openFile={(path, change, at) => setFile({ path, change, at })}
              openScreen={(i, time) => {
                setFile({ path: null, screen: i });
                if (time !== undefined) setSeek((s) => ({ time, n: (s?.n ?? 0) + 1 }));
              }}
              openLink={(url) => plugin.open(url)}
              openDiagram={(id) => setTab(id)}
              addComment={addComment}
              removeComment={removeComment}
              editComment={editComment}
              toggleReviewed={toggleReviewed}
              composeAt={composeAt}
              footer={footer}
            />
          </div>
          <div className="cm-panel-foot" ref={setFooter} />
        </aside>
      </div>
    </div>
  );
}

import { Check, MessageSquare, Monitor, MousePointerClick, PanelRight, Smartphone, Tablet, Undo2 } from "lucide-react";
import { VIEWPORTS, type Decision, type Verdict, type Viewport } from "../types";

const ICONS = { desktop: Monitor, tablet: Tablet, phone: Smartphone } as const;

type Props = {
  title: string;
  selecting: boolean;
  onSelecting: () => void;
  viewport: Viewport;
  onViewport: (v: Viewport) => void;
  verdict: Verdict;
  verdictChosen: boolean;
  onVerdict: (v: Verdict) => void;
  count: number;
  panel: boolean;
  onPanel: () => void;
  readonly: boolean;
  decided: Decision | null;
};

export function Toolbar({
  title,
  selecting,
  onSelecting,
  viewport,
  onViewport,
  verdict,
  verdictChosen,
  onVerdict,
  count,
  panel,
  onPanel,
  readonly,
  decided,
}: Props) {
  return (
    <header className="toolbar">
      <span className="toolbar-title" title={title}>
        {title}
      </span>
      {readonly ? (
        <span className={`decided decided-${decided?.verdict ?? verdict}`}>
          {(decided?.verdict ?? verdict) === "approve" ? <Check size={13} /> : <Undo2 size={13} />}
          {(decided?.verdict ?? verdict) === "approve" ? "Approved" : "Changes requested"}
        </span>
      ) : (
        <button
          type="button"
          className={`btn select-btn ${selecting ? "is-on" : ""}`}
          onClick={onSelecting}
          data-select
          title="Pick an element to comment on (S)"
        >
          <MousePointerClick size={14} />
          {selecting ? "Selecting…" : "Select"}
          <kbd>S</kbd>
        </button>
      )}
      <span className="spacer" />
      <span className="segment" role="group" aria-label="Viewport">
        {VIEWPORTS.map((v) => {
          const Icon = ICONS[v.key];
          return (
            <button
              key={v.key}
              type="button"
              className={viewport === v.key ? "is-on" : ""}
              onClick={() => onViewport(v.key)}
              title={v.width ? `${v.label} · ${v.width}px` : v.label}
              aria-pressed={viewport === v.key}
            >
              <Icon size={14} />
            </button>
          );
        })}
      </span>
      {!readonly ? (
        <span className="segment verdict" role="group" aria-label="Verdict" data-auto={!verdictChosen}>
          <button
            type="button"
            className={verdict === "approve" ? "is-on" : ""}
            onClick={() => onVerdict("approve")}
            aria-pressed={verdict === "approve"}
            data-verdict="approve"
          >
            <Check size={13} /> Approve
          </button>
          <button
            type="button"
            className={verdict === "revise" ? "is-on" : ""}
            onClick={() => onVerdict("revise")}
            aria-pressed={verdict === "revise"}
            data-verdict="revise"
          >
            <Undo2 size={13} /> Request changes
          </button>
        </span>
      ) : null}
      <button
        type="button"
        className={`btn icon-btn ${panel ? "is-on" : ""}`}
        onClick={onPanel}
        title={panel ? "Hide comments" : "Show comments"}
        aria-pressed={panel}
        data-panel
      >
        {panel ? <PanelRight size={14} /> : <MessageSquare size={14} />}
        <span className="count">{count}</span>
      </button>
    </header>
  );
}

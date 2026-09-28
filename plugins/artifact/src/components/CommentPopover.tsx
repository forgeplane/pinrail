import { Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Box } from "../artifact";
import { KINDS, type Kind } from "../types";

type Props = {
  box: Box;
  selector: string;
  tag: string;
  text: string;
  kind: Kind;
  isNew: boolean;
  onSave: (text: string, kind: Kind) => void;
  onCancel: () => void;
  onRemove?: () => void;
};

const WIDTH = 360;
const GAP = 12;

const PLACEHOLDER: Record<Kind, string> = {
  change: "What should change here?",
  question: "What do you want to ask about it?",
  praise: "What should stay as it is?",
};

export function CommentPopover({ box, selector, tag, text, kind, isNew, onSave, onCancel, onRemove }: Props) {
  const [value, setValue] = useState(text);
  const [chosen, setChosen] = useState<Kind>(kind);
  const area = useRef<HTMLTextAreaElement>(null);
  const self = useRef<HTMLDivElement>(null);
  // where it sits, whether it opens above the element, and where along its
  // edge the arrow points at the element
  const [place, setPlace] = useState({ left: box.x, top: box.y + box.h + GAP, above: false, arrow: 24 });

  useEffect(() => {
    area.current?.focus();
  }, []);

  // below the element, inside the stage; above it when there is no room
  useEffect(() => {
    const el = self.current;
    const stage = el?.closest("[data-stage]") as HTMLElement | null;
    if (!el || !stage) return;
    const layer = el.parentElement!.getBoundingClientRect();
    const view = stage.getBoundingClientRect();
    const h = el.offsetHeight;
    const left = Math.max(8, Math.min(box.x, layer.width - WIDTH - 8));
    let top = box.y + box.h + GAP;
    let above = false;
    if (layer.top + top + h > view.bottom - 8 && box.y - h - GAP > view.top - layer.top) {
      top = box.y - h - GAP;
      above = true;
    }
    const arrow = Math.max(18, Math.min(WIDTH - 18, box.x + Math.min(box.w, 48) / 2 - left));
    setPlace({ left, top, above, arrow });
  }, [box]);

  const save = () => onSave(value, chosen);

  return (
    <div
      ref={self}
      className={`popover kind-${chosen} ${place.above ? "is-above" : ""}`}
      style={{ left: place.left, top: place.top, width: WIDTH, ["--arrow" as string]: `${place.arrow}px` }}
      data-popover
      role="dialog"
      aria-label={isNew ? "New comment" : "Edit comment"}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="popover-head">
        <span className="tag-chip">{`<${tag}>`}</span>
        <span className="popover-path mono" title={selector}>
          {selector}
        </span>
        <button type="button" className="icon-btn popover-close" onClick={onCancel} aria-label="Close">
          <X size={14} />
        </button>
      </div>
      <textarea
        ref={area}
        className="popover-text"
        rows={3}
        value={value}
        placeholder={PLACEHOLDER[chosen]}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
            e.preventDefault();
            e.stopPropagation();
            save();
          }
          if (e.key === "Escape") {
            e.stopPropagation();
            onCancel();
          }
        }}
        data-comment-text
      />
      <div className="kind-pills" role="group" aria-label="Kind">
        {KINDS.map((k) => (
          <button
            key={k.key}
            type="button"
            className={`kind-pill kind-${k.key} ${chosen === k.key ? "is-on" : ""}`}
            onClick={() => setChosen(k.key)}
            aria-pressed={chosen === k.key}
          >
            <span className="kind-dot" />
            {k.label}
          </button>
        ))}
      </div>
      <div className="popover-foot">
        <span className="popover-hint">
          <kbd>⌘</kbd>
          <kbd>↵</kbd> to {isNew ? "add" : "save"}
        </span>
        <span className="spacer" />
        {onRemove ? (
          <button
            type="button"
            className="btn ghost danger"
            onClick={onRemove}
            title="Remove comment"
            aria-label="Remove comment"
            data-remove
          >
            <Trash2 size={14} />
          </button>
        ) : null}
        <button type="button" className="btn ghost" onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="btn primary" onClick={save} disabled={!value.trim()} data-save>
          {isNew ? "Add comment" : "Save"}
        </button>
      </div>
    </div>
  );
}

import { Trash2 } from "lucide-react";
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

const WIDTH = 320;

export function CommentPopover({ box, selector, tag, text, kind, isNew, onSave, onCancel, onRemove }: Props) {
  const [value, setValue] = useState(text);
  const [chosen, setChosen] = useState<Kind>(kind);
  const area = useRef<HTMLTextAreaElement>(null);
  const self = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number }>({ left: box.x, top: box.y + box.h + 8 });

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
    let left = Math.min(box.x, layer.width - WIDTH - 8);
    left = Math.max(8, left);
    let top = box.y + box.h + 8;
    if (layer.top + top + h > view.bottom - 8 && box.y - h - 8 > view.top - layer.top) top = box.y - h - 8;
    setPlace({ left, top });
  }, [box]);

  const save = () => onSave(value, chosen);

  return (
    <div ref={self} className="popover" style={{ left: place.left, top: place.top, width: WIDTH }} data-popover onClick={(e) => e.stopPropagation()}>
      <div className="popover-target">
        <span className="mono">{selector}</span>
        <span className="faint">{tag}</span>
      </div>
      <textarea
        ref={area}
        className="ta"
        rows={3}
        value={value}
        placeholder={isNew ? "What should change here?" : ""}
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
      <div className="popover-bar">
        <span className="segment kinds" role="group" aria-label="Kind">
          {KINDS.map((k) => (
            <button key={k.key} type="button" className={`${chosen === k.key ? "is-on" : ""} kind-${k.key}`} onClick={() => setChosen(k.key)} aria-pressed={chosen === k.key}>
              {k.label}
            </button>
          ))}
        </span>
        <span className="spacer" />
        {onRemove ? (
          <button type="button" className="btn icon-btn danger" onClick={onRemove} title="Remove comment" data-remove>
            <Trash2 size={14} />
          </button>
        ) : null}
        <button type="button" className="btn" onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="btn primary" onClick={save} disabled={!value.trim()} data-save>
          {isNew ? "Add" : "Save"}
        </button>
      </div>
    </div>
  );
}

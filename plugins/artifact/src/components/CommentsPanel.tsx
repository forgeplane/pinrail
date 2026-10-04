import { Pencil, Trash2, Unlink } from "lucide-react";
import { useState } from "react";
import type { Box } from "../artifact";
import { KINDS, type Comment } from "../types";

type Props = {
  comments: Comment[];
  pins: Map<string, Box>;
  focused: string | null;
  readonly: boolean;
  previous: Comment[];
  onFocus: (id: string) => void;
  onEdit: (id: string) => void;
  onRemove: (id: string) => void;
};

const kindLabel = (k: Comment["kind"]) => KINDS.find((x) => x.key === k)?.label ?? k;

export function CommentsPanel({ comments, pins, focused, readonly, previous, onFocus, onEdit, onRemove }: Props) {
  // a settled round with nothing of its own opens on what came before
  const [showPrevious, setShowPrevious] = useState(readonly && comments.length === 0 && previous.length > 0);
  return (
    <aside className="panel" data-panel-list>
      <div className="panel-head">
        <span className="pinrail-eyebrow">Comments</span>
        <span className="pinrail-faint">{comments.length}</span>
      </div>
      {comments.length === 0 ? (
        <p className="panel-empty">
          {readonly ? "No comments were made." : "Turn on Select and click an element to comment on it."}
        </p>
      ) : (
        <ol className="comment-list">
          {comments.map((c, i) => {
            const detached = !pins.has(c.id);
            return (
              <li
                key={c.id}
                className={`comment ${focused === c.id ? "is-focused" : ""} kind-${c.kind}`}
                data-comment={c.id}
                onClick={() => onFocus(c.id)}
              >
                <span className="comment-n">{i + 1}</span>
                <div className="comment-body">
                  <div className="comment-target">
                    <span className="mono">{c.selector}</span>
                    <span className={`kind-tag kind-${c.kind}`}>{kindLabel(c.kind)}</span>
                    {detached ? (
                      <span className="pinrail-faint with-icon" title="The element is no longer in the artifact">
                        <Unlink size={11} /> detached
                      </span>
                    ) : null}
                  </div>
                  <div className="comment-text">{c.text}</div>
                  {c.snippet ? <div className="comment-snippet">“{c.snippet}”</div> : null}
                </div>
                {!readonly ? (
                  <span className="comment-actions">
                    <button
                      type="button"
                      className="icon-btn"
                      title="Edit"
                      onClick={(e) => (e.stopPropagation(), onEdit(c.id))}
                      disabled={detached}
                      data-edit
                    >
                      <Pencil size={13} />
                    </button>
                    <button
                      type="button"
                      className="icon-btn danger"
                      title="Remove"
                      onClick={(e) => (e.stopPropagation(), onRemove(c.id))}
                      data-remove
                    >
                      <Trash2 size={13} />
                    </button>
                  </span>
                ) : null}
              </li>
            );
          })}
        </ol>
      )}
      {previous.length > 0 ? (
        <div className="previous">
          <button
            type="button"
            className="previous-toggle"
            onClick={() => setShowPrevious((s) => !s)}
            aria-expanded={showPrevious}
          >
            <span className="pinrail-eyebrow">Previous round</span>
            <span className="pinrail-faint">{previous.length}</span>
          </button>
          {showPrevious ? (
            <ol className="comment-list is-previous">
              {previous.map((c, i) => (
                <li key={c.id} className={`comment kind-${c.kind}`}>
                  <span className="comment-n">{i + 1}</span>
                  <div className="comment-body">
                    <div className="comment-target">
                      <span className="mono">{c.selector}</span>
                      <span className={`kind-tag kind-${c.kind}`}>{kindLabel(c.kind)}</span>
                    </div>
                    <div className="comment-text">{c.text}</div>
                  </div>
                </li>
              ))}
            </ol>
          ) : null}
        </div>
      ) : null}
    </aside>
  );
}

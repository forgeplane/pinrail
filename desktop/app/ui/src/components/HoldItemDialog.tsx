// Putting a work item on hold while it, or one of its sub-items, is in
// progress: the agents working on them are told to stop, so the person is
// asked first. Esc leaves.

import { useEffect, useRef } from "react";
import { api } from "../api/client";
import type { WorkItem } from "../api/types";
import { useWindowKey } from "../lib/useWindowKey";

/** The items in progress that putting `item` on hold stops: the item itself
 *  and its sub-items at any depth. Asks the core only when there can be some. */
export async function stoppedByHold(item: WorkItem): Promise<WorkItem[]> {
  if (item.status !== "in_progress" && item.sub_items.total === item.sub_items.closed) return [];
  const { items } = await api.items({ status: "open,in_progress,on_hold" });
  const under = new Map<string, WorkItem[]>();
  for (const i of items) if (i.parent) under.set(i.parent, [...(under.get(i.parent) ?? []), i]);
  const stopped = item.status === "in_progress" ? [item] : [];
  const visit = (key: string) => {
    for (const sub of under.get(key) ?? []) {
      if (sub.status === "in_progress") stopped.push(sub);
      visit(sub.key);
    }
  };
  visit(item.key);
  return stopped;
}

export function HoldItemDialog({
  item,
  stopped,
  onClose,
  onConfirm,
}: {
  item: WorkItem;
  /** the items in progress the hold stops, from `stoppedByHold` */
  stopped: WorkItem[];
  onClose: () => void;
  onConfirm: () => void;
}) {
  const keep = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    keep.current?.focus();
  }, [onClose]);
  useWindowKey(
    (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    },
    { capture: true },
  );

  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <div
        className="app-dialog discard-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="hold-item-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="dialog-head">
          <h2 id="hold-item-title">Put {item.key} on hold?</h2>
        </div>
        <p className="dim discard-what" title={item.title}>
          {item.title}
        </p>
        <ul className="hold-item-work" data-testid="hold-item-work">
          {stopped.map((i) => (
            <li key={i.key}>
              <span className="mono">{i.key}</span> {i.title}
              <span className="dim">
                {" · "}
                {i.claim?.agent === "person"
                  ? "you are working on it"
                  : `${i.claim?.agent ?? "an agent"} is working on it`}
              </span>
            </li>
          ))}
        </ul>
        <p className="dim">
          {stopped.length === 1
            ? "It is released, and an agent working on it is told to stop the next time it reports its status."
            : "They are released, and each agent working on one is told to stop the next time it reports its status."}
        </p>
        <div className="dialog-actions">
          <button ref={keep} type="button" className="chrome-button" onClick={onClose}>
            Keep working
          </button>
          <button type="button" className="chrome-button button-danger" onClick={onConfirm}>
            Put on hold
          </button>
        </div>
      </div>
    </div>
  );
}

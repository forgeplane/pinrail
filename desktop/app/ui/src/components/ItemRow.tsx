// One row of the Work items page: its status, key, title, marks, who works
// on it and when it last changed. Memoized, with handlers that take the
// item, so that moving the pointer down the list draws again only the rows
// whose highlight changed.

import { ChevronDown, ChevronRight, Clock, ListOrdered, Lock, User } from "lucide-react";
import { memo } from "react";
import type { WorkItem } from "../api/types";
import { age, ordinal, stamp } from "../lib/format";
import { AgentIcon } from "./AgentIcon";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { Tooltip } from "./Tooltip";
import { ProgressRing } from "./ProgressRing";

/** What a row does, the same functions from one render to the next. */
export type ItemRowActions = {
  hover: (index: number) => void;
  open: (item: WorkItem) => void;
  toggleFold: (key: string) => void;
  dragStart: (event: React.DragEvent, item: WorkItem, group: string) => void;
  dragOver: (event: React.DragEvent, item: WorkItem) => void;
  dragLeave: (event: React.DragEvent) => void;
  drop: (event: React.DragEvent, item: WorkItem) => void;
  dragEnd: () => void;
};

type Props = {
  item: WorkItem;
  depth: number;
  folded: boolean;
  /** how many sub-items it shows under it */
  children: number;
  index: number;
  group: string;
  focused: boolean;
  open: boolean;
  dropTarget: boolean;
  /** its parent's key, when the parent is not shown above it */
  orphanOf: string | null;
  /** whether the row names its repository */
  showRepo: boolean;
  actions: ItemRowActions;
};

export const ItemRow = memo(function ItemRow({
  item,
  depth,
  folded,
  children,
  index,
  group,
  focused,
  open,
  dropTarget,
  orphanOf,
  showRepo,
  actions,
}: Props) {
  // on hold, it waits: dimmed as a closed item is
  const closed = item.status === "done" || item.status === "cancelled" || item.status === "on_hold";
  return (
    // the fold button sits over the row's first column, beside the row
    // rather than inside it: a button cannot hold another
    <div className="item-line">
      <div
        role="button"
        tabIndex={-1}
        data-row={index}
        data-testid="item-row"
        data-key={item.key}
        className={`item-row ${focused ? "is-focused" : ""} ${open ? "is-open" : ""} ${closed ? "is-closed" : ""} ${dropTarget ? "is-drop" : ""}`}
        // its arrow under the group's, its status under the group's icon,
        // and each level of sub-items one step in
        style={{ paddingLeft: 4 + depth * 18 }}
        onMouseEnter={() => actions.hover(index)}
        onClick={() => actions.open(item)}
        draggable
        onDragStart={(event) => actions.dragStart(event, item, group)}
        onDragOver={(event) => actions.dragOver(event, item)}
        onDragLeave={actions.dragLeave}
        onDrop={(event) => actions.drop(event, item)}
        onDragEnd={actions.dragEnd}
      >
        <span className="item-fold" />
        <ItemStatusIcon status={item.status} />
        <span className="item-key mono">{item.key}</span>
        <span className="item-title">
          <span className="item-title-text truncate">{item.title}</span>
          {orphanOf ? <span className="item-parent">in {orphanOf}</span> : null}
          {showRepo ? <span className="item-repo">{item.repo}</span> : null}
        </span>
        <span className="item-marks">
          {item.queue ? (
            <span className="item-mark is-queued" data-testid="item-queued" title={`Queued, ${ordinal(item.queue)}`}>
              <ListOrdered size={11} aria-hidden="true" />
              {item.queue}
            </span>
          ) : null}
          {item.waiting ? (
            <span className="item-mark is-waiting" data-testid="item-waiting">
              <Clock size={11} aria-hidden="true" /> waiting on you
            </span>
          ) : null}
          {item.blocked && !closed ? (
            <span className="item-mark is-blocked" data-testid="item-blocked">
              <Lock size={11} aria-hidden="true" /> blocked by {item.blocked_by.join(", ")}
            </span>
          ) : null}
          {item.sub_items.in_progress && !closed && item.status !== "in_progress" ? (
            <span className="item-mark is-active" data-testid="item-active">
              <ItemStatusIcon status="in_progress" size={11} decorative /> {item.sub_items.in_progress} in progress
            </span>
          ) : null}
          {item.sub_items.total ? (
            <span
              className="item-progress"
              data-testid="item-progress"
              title={`${item.sub_items.closed} of ${item.sub_items.total} sub-items closed`}
            >
              <ProgressRing done={item.sub_items.closed} total={item.sub_items.total} />
              {item.sub_items.closed}/{item.sub_items.total}
            </span>
          ) : null}
        </span>
        <span className="item-claim">
          {item.claim ? (
            item.claim.agent === "person" ? (
              <Tooltip label="You are working on it" side="top">
                <User size={13} aria-label="You" />
              </Tooltip>
            ) : (
              <Tooltip label={`${item.claim.agent} is working on it`} side="top">
                <AgentIcon requestedBy={item.claim.agent} size={14} />
              </Tooltip>
            )
          ) : null}
        </span>
        <time className="review-row-time" dateTime={item.updated_at} title={stamp(item.updated_at)}>
          {age(item.updated_at)}
        </time>
      </div>
      {children ? (
        <button
          type="button"
          className="item-fold-toggle"
          style={{ left: 4 + depth * 18 }}
          aria-label={folded ? `Show the sub-items of ${item.key}` : `Hide the sub-items of ${item.key}`}
          aria-expanded={!folded}
          onClick={() => actions.toggleFold(item.key)}
        >
          {folded ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
        </button>
      ) : null}
    </div>
  );
});

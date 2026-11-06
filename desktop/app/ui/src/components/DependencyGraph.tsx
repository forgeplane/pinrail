// An item's dependencies one step each way: what it depends on on the left,
// the item in the middle, what it blocks on the right, each with its status.
// A node opens its item; one the item depends on can be removed.

import { X } from "lucide-react";
import type { ItemStatus, RelatedItem } from "../api/types";
import { ItemStatusIcon } from "./ItemStatusIcon";

/** The height each node takes in its column. */
const ROW = 46;

/** Where the node `i` of `n` sits, as a share of the graph's height. */
const at = (i: number, n: number) => (i + 0.5) / n;

function Node({
  item,
  current,
  onOpen,
  onRemove,
}: {
  item: { key: string; title: string; status: ItemStatus };
  current?: boolean;
  onOpen?: (key: string) => void;
  onRemove?: (key: string) => void;
}) {
  return (
    <span className={`dep-node ${current ? "is-current" : ""} ${item.status === "cancelled" ? "is-cancelled" : ""}`}>
      <button
        type="button"
        className="dep-node-open"
        disabled={current}
        onClick={() => onOpen?.(item.key)}
        title={`${item.key} ${item.title}`}
      >
        <ItemStatusIcon status={item.status} />
        <span className="dep-node-text">
          <span className="dep-node-key mono">{item.key}</span>
          <span className="dep-node-title">{item.title}</span>
        </span>
      </button>
      {onRemove ? (
        <button
          type="button"
          className="dep-node-remove"
          aria-label={`No longer depend on ${item.key}`}
          onClick={() => onRemove(item.key)}
        >
          <X size={11} />
        </button>
      ) : null}
    </span>
  );
}

export function DependencyGraph({
  item,
  dependsOn,
  blocks,
  onOpen,
  onRemove,
}: {
  item: { key: string; title: string; status: ItemStatus };
  dependsOn: RelatedItem[];
  blocks: RelatedItem[];
  onOpen: (key: string) => void;
  onRemove: (key: string) => void;
}) {
  const rows = Math.max(dependsOn.length, blocks.length, 1);
  const height = rows * ROW;
  // the arrows, in a box 100 wide and as high as the graph: the columns
  // end at 31 and 69, and the item stands between 35 and 65
  const curve = (x1: number, y1: number, x2: number, y2: number) =>
    `M${x1} ${y1} C${(x1 + x2) / 2} ${y1} ${(x1 + x2) / 2} ${y2} ${x2} ${y2}`;
  return (
    <div className="dep-graph" style={{ height }} data-testid="dependency-graph">
      <svg className="dep-edges" viewBox={`0 0 100 ${height}`} preserveAspectRatio="none" aria-hidden="true">
        {dependsOn.map((d, i) => (
          <path
            key={d.key}
            d={curve(31, at(i, dependsOn.length) * height, 35, height / 2)}
            className={d.status === "cancelled" ? "is-dashed" : ""}
          />
        ))}
        {blocks.map((b, i) => (
          <path key={b.key} d={curve(65, height / 2, 69, at(i, blocks.length) * height)} />
        ))}
      </svg>
      <div className="dep-column is-left" aria-label="Depends on">
        {dependsOn.map((d, i) => (
          <div key={d.key} className="dep-slot" style={{ top: at(i, dependsOn.length) * height }}>
            <Node item={d} onOpen={onOpen} onRemove={onRemove} />
          </div>
        ))}
      </div>
      <div className="dep-column is-middle">
        <div className="dep-slot" style={{ top: height / 2 }}>
          <Node item={item} current />
        </div>
      </div>
      <div className="dep-column is-right" aria-label="Blocks">
        {blocks.map((b, i) => (
          <div key={b.key} className="dep-slot" style={{ top: at(i, blocks.length) * height }}>
            <Node item={b} onOpen={onOpen} />
          </div>
        ))}
      </div>
    </div>
  );
}

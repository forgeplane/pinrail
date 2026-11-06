// A work item named where something belongs to it, such as a review: its
// key and title, opening the item in the Work items page. Inside a row that
// is a link itself, it is no link of its own, which HTML does not allow,
// and opens the item on a click instead of the row.

import { ListChecks } from "lucide-react";
import { Link, useNavigate } from "react-router";
import type { ItemTag } from "../api/types";

export const itemPath = (key: string) => `/items?open=${encodeURIComponent(key)}`;

export function ItemChip({ item, inLink = false }: { item: Pick<ItemTag, "key" | "title">; inLink?: boolean }) {
  const navigate = useNavigate();
  const content = (
    <>
      <ListChecks size={12} aria-hidden="true" />
      <span className="mono">{item.key}</span>
      <span className="item-chip-title">{item.title}</span>
    </>
  );
  if (inLink)
    return (
      <span
        role="link"
        tabIndex={-1}
        className="item-chip"
        title={`${item.key} ${item.title}`}
        data-testid="item-chip"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          navigate(itemPath(item.key));
        }}
      >
        {content}
      </span>
    );
  return (
    <Link to={itemPath(item.key)} className="item-chip" title={`${item.key} ${item.title}`} data-testid="item-chip">
      {content}
    </Link>
  );
}

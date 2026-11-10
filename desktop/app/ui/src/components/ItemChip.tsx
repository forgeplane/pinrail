// A work item named where something belongs to it, such as a review: its
// key and title, opening the item in the Work items page. Inside a row that
// is a link itself, it is no link of its own, which HTML does not allow,
// and opens the item on a click instead of the row. Where an ItemOpener is
// provided, such as a session's page, a click opens the item there instead.

import { ListChecks } from "lucide-react";
import { createContext, useCallback, useContext, type MouseEvent } from "react";
import { Link, useNavigate } from "react-router";
import type { ItemTag } from "../api/types";

export const itemPath = (key: string) => `/items?open=${encodeURIComponent(key)}`;

/** Opens a work item where the page shows one, such as beside a session's thread. */
export const ItemOpener = createContext<((key: string) => void) | null>(null);

/** Opens a work item: where the page provides, or on the Work items page. */
export function useOpenItem() {
  const opener = useContext(ItemOpener);
  const navigate = useNavigate();
  return useCallback((key: string) => (opener ? opener(key) : navigate(itemPath(key))), [opener, navigate]);
}

/** A plain click on a link to an item, which an ItemOpener takes; other clicks follow the link. */
export function useItemLinkClick() {
  const opener = useContext(ItemOpener);
  return useCallback(
    (event: MouseEvent, key: string) => {
      if (!opener || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      opener(key);
    },
    [opener],
  );
}

export function ItemChip({
  item,
  inLink = false,
  keyOnly = false,
}: {
  item: Pick<ItemTag, "key" | "title">;
  inLink?: boolean;
  /** the key alone, the title on hover: for several chips in a row */
  keyOnly?: boolean;
}) {
  const openItem = useOpenItem();
  const onLinkClick = useItemLinkClick();
  const content = (
    <>
      <ListChecks size={12} aria-hidden="true" />
      <span className="mono">{item.key}</span>
      {keyOnly ? null : <span className="item-chip-title truncate">{item.title}</span>}
    </>
  );
  if (inLink)
    return (
      <span
        role="link"
        tabIndex={-1}
        className="item-chip pill pill-accent"
        title={`${item.key} ${item.title}`}
        data-testid="item-chip"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          openItem(item.key);
        }}
      >
        {content}
      </span>
    );
  return (
    <Link
      to={itemPath(item.key)}
      className="item-chip pill pill-accent"
      title={`${item.key} ${item.title}`}
      data-testid="item-chip"
      onClick={(e) => onLinkClick(e, item.key)}
    >
      {content}
    </Link>
  );
}

/** The chips of a review's work items inside a link, such as an inbox row
 *  or a thread's card: the first `max`, then "+N" with the rest in its
 *  tooltip. */
export function ItemChips({ items, max = 2 }: { items: ItemTag[]; max?: number }) {
  if (items.length === 0) return null;
  const shown = items.length > max ? items.slice(0, max) : items;
  const rest = items.slice(shown.length);
  return (
    <>
      {shown.map((item) => (
        <ItemChip key={item.id} item={item} inLink />
      ))}
      {rest.length ? (
        <span className="item-more" data-testid="item-more" title={rest.map((i) => `${i.key} ${i.title}`).join("\n")}>
          +{rest.length}
        </span>
      ) : null}
    </>
  );
}

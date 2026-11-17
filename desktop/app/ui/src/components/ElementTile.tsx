// A session's element as the periodic table draws it: its symbol in its
// category's colour and its atomic number in a corner. A rare element has a
// border in that colour, and a legendary one a tint as well. Hovering it
// shows the element's card.

import type { ReactNode } from "react";
import type { Element } from "../api/types";
import { categoryClass } from "../lib/elements";
import { Tooltip } from "./Tooltip";

const RARITY: Record<Element["rarity"], string> = {
  common: "Common",
  uncommon: "Uncommon",
  rare: "Rare",
  legendary: "Legendary",
};

/** The element's card: what it is, how rare a session's draw makes it, and
 *  one sentence about it. */
export function ElementCard({ element }: { element: Element }) {
  return (
    <span className={`element-card is-${categoryClass(element.category)}`}>
      <span className="element-card-head">
        <ElementTile element={element} size="xl" />
        <span className="element-card-title">
          <span className="element-card-name">{element.name}</span>
          <span className="element-card-facts">
            <span>{element.category.charAt(0).toUpperCase() + element.category.slice(1)}</span>
            <span className={`element-card-rarity is-${element.rarity}`}>{RARITY[element.rarity]}</span>
          </span>
        </span>
      </span>
      <span className="element-card-text">{element.description}</span>
    </span>
  );
}

export function ElementTile({
  element,
  size = "md",
  card = false,
  badge,
}: {
  element: Element;
  /** 22px beside text, 34px in an avatar's place, 48px on its card */
  size?: "md" | "lg" | "xl";
  /** shows the element's card on hover */
  card?: boolean;
  /** a small mark on its corner, such as the agent's icon */
  badge?: ReactNode;
}) {
  const tile = (
    <span
      className={`element-tile is-${size} is-${categoryClass(element.category)} is-${element.rarity}`}
      role="img"
      aria-label={`${element.name}, element ${element.number}`}
      data-testid="element-tile"
      tabIndex={card ? 0 : undefined}
    >
      {size !== "md" ? <span className="element-tile-number">{element.number}</span> : null}
      <span className="element-tile-symbol">{element.symbol}</span>
      {badge ? <span className="element-tile-badge">{badge}</span> : null}
    </span>
  );
  return card ? (
    <Tooltip label={<ElementCard element={element} />} side="bottom" card align="start">
      {tile}
    </Tooltip>
  ) : (
    tile
  );
}

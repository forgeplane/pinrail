// The periodic table of sessions: each element a session has been named
// after is discovered, drawn in its category's colour with how many
// sessions it named; the others are faint, a question mark in the place of
// their symbol. The space above the transition metals, where a printed
// table has its key, holds the card of the element pointed at: what it is,
// one sentence about it, and the sessions named after it.

import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "../api/client";
import type { Element, Session } from "../api/types";
import { categoryClass } from "../lib/elements";
import { sessionLabel } from "../lib/sessions";
import { useLoad } from "../lib/useLoad";
import { ElementTile } from "./ElementTile";
import { sessionPath } from "./Thread";

const RARITIES: Element["rarity"][] = ["common", "uncommon", "rare", "legendary"];
const RARITY_LABEL: Record<Element["rarity"], string> = {
  common: "Common",
  uncommon: "Uncommon",
  rare: "Rare",
  legendary: "Legendary",
};

/** The table, loaded again each time `changed` moves on; a click on a
 *  discovered element calls `onPick` with its atomic number. */
export function PeriodicTable({ changed, onPick }: { changed: number; onPick: (element: number) => void }) {
  const [elements, setElements] = useState<Element[] | null>(null);
  useEffect(() => {
    let current = true;
    api.elements().then(
      (d) => current && setElements(d.elements),
      () => {},
    );
    return () => {
      current = false;
    };
  }, [changed]);
  // the element pointed at or focused, whose card the table shows
  const [shown, setShown] = useState<number | null>(null);
  if (!elements) return null;

  const found = (e: Element) => (e.sessions ?? 0) > 0;
  const discovered = elements.filter(found);
  const sessions = discovered.reduce((n, e) => n + (e.sessions ?? 0), 0);
  const card = shown ? elements[shown - 1] : null;

  return (
    <section className="ptable" aria-label="Periodic table of sessions">
      <div className="ptable-summary">
        <span className="ptable-count">
          <strong>{discovered.length}</strong> of {elements.length} elements discovered · {sessions}{" "}
          {sessions === 1 ? "session" : "sessions"}
        </span>
        <span className="ptable-rarities">
          {RARITIES.map((r) => {
            const all = elements.filter((e) => e.rarity === r);
            return (
              <span key={r} className={`ptable-rarity is-${r}`}>
                {RARITY_LABEL[r]} {all.filter(found).length}/{all.length}
              </span>
            );
          })}
        </span>
      </div>
      <div className="ptable-grid" onMouseLeave={() => setShown(null)}>
        <div className="ptable-inset" aria-live="polite">
          {card ? (
            <ElementInset element={card} />
          ) : (
            <p className="ptable-hint">Point at an element to read about it.</p>
          )}
        </div>
        {elements.map((e) => (
          <button
            key={e.number}
            type="button"
            className={`ptable-cell is-${categoryClass(e.category)} is-${e.rarity} ${found(e) ? "is-found" : "is-unfound"} ${shown === e.number ? "is-shown" : ""}`}
            // the lanthanides and actinides below the table, a row apart
            style={{ gridRow: e.row > 7 ? e.row + 1 : e.row, gridColumn: e.column }}
            aria-label={
              found(e)
                ? `${e.name}, ${e.sessions} ${e.sessions === 1 ? "session" : "sessions"}`
                : `Element ${e.number}, not discovered`
            }
            data-testid="ptable-cell"
            onMouseEnter={() => setShown(e.number)}
            onFocus={() => setShown(e.number)}
            onClick={() => found(e) && onPick(e.number)}
          >
            <span className="ptable-number">{e.number}</span>
            {(e.sessions ?? 0) > 1 ? <span className="ptable-sessions">{e.sessions}</span> : null}
            <span className="ptable-symbol">{found(e) ? e.symbol : "?"}</span>
            {found(e) ? <span className="ptable-name">{e.name}</span> : null}
          </button>
        ))}
      </div>
    </section>
  );
}

/** The card of the element pointed at, in the table's empty space: for a
 *  discovered one, what it is, one sentence about it and its sessions; for
 *  another, only its number and how rare it is. */
function ElementInset({ element }: { element: Element }) {
  const found = (element.sessions ?? 0) > 0;
  const sessions =
    useLoad(
      () =>
        found
          ? api
              .sessions({ element: String(element.number), limit: "4", include_archived: "true" })
              .then((d) => d.sessions)
          : Promise.resolve<Session[]>([]),
      [element.number, found],
    ).data ?? [];
  if (!found)
    return (
      <div className={`ptable-card is-unfound is-${element.rarity}`}>
        <span className="ptable-card-mystery">?</span>
        <span className="ptable-card-body">
          <span className="ptable-card-name">Element {element.number}</span>
          <span className="ptable-card-facts">
            Not discovered yet ·{" "}
            <span className={`ptable-rarity is-${element.rarity}`}>{RARITY_LABEL[element.rarity]}</span>
          </span>
          <span className="ptable-card-text">A session named after it will reveal it.</span>
        </span>
      </div>
    );
  return (
    <div className={`ptable-card is-${categoryClass(element.category)}`}>
      <ElementTile element={element} size="xl" />
      <span className="ptable-card-body">
        <span className="ptable-card-name">{element.name}</span>
        <span className="ptable-card-facts">
          {element.category.charAt(0).toUpperCase() + element.category.slice(1)} ·{" "}
          <span className={`ptable-rarity is-${element.rarity}`}>{RARITY_LABEL[element.rarity]}</span>
        </span>
        <span className="ptable-card-text">{element.description}</span>
        <span className="ptable-card-sessions">
          {sessions.map((s) => (
            <Link key={`${s.agent}/${s.id}`} to={sessionPath(s)} className="ptable-card-session">
              {sessionLabel(s)}
            </Link>
          ))}
          {(element.sessions ?? 0) > sessions.length && sessions.length ? (
            <span className="faint">and {(element.sessions ?? 0) - sessions.length} more</span>
          ) : null}
        </span>
      </span>
    </div>
  );
}

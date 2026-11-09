// The What's new dialog shows the cards of one or more releases one at a
// time, newest release first. Each card has a picture in the current theme,
// a title, a short description, and an optional button that opens the
// feature. ← and → move between cards. Esc, the close button, or Done on
// the last card closes the dialog.

import { Sparkles, X } from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { pictureOf, type CardAction, type Release } from "../lib/whatsNew";
import { useTheme } from "../lib/theme";
import { Tooltip } from "./Tooltip";
import { Dialog } from "./Dialog";

export function WhatsNewDialog({
  releases,
  onAction,
  onClose,
}: {
  releases: Release[];
  /** runs a card's button action after the dialog has closed */
  onAction: (action: CardAction) => void;
  onClose: () => void;
}) {
  const theme = useTheme();
  const cards = useMemo(
    () => releases.flatMap((r) => r.cards.map((card) => ({ ...card, version: r.version }))),
    [releases],
  );
  const [at, setAt] = useState(0);
  const card = cards[Math.min(at, cards.length - 1)];
  const last = at >= cards.length - 1;
  const next = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    next.current?.focus();
  }, []);

  // added before the dialog is painted, so a key pressed as soon as it
  // shows is not lost
  useLayoutEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        setAt((n) => Math.min(n + 1, cards.length - 1));
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        setAt((n) => Math.max(n - 1, 0));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cards.length, onClose]);

  if (!card) return null;
  const picture = card.image ? pictureOf(card.version, card.image, theme) : null;

  return (
    // its keys are its own, added before it paints
    <Dialog
      labelledBy="whats-new-title"
      plain
      className="whats-new"
      backdropClass="whats-new-backdrop"
      data-testid="whats-new"
      initialFocus={next}
      onClose={onClose}
      closeOnEscape={false}
    >
      <header className="whats-new-head">
        <span className="whats-new-tag">New in {card.version}</span>
        <h2 id="whats-new-title">What's new in Pinrail</h2>
        <Tooltip label="Close" keys={["Esc"]}>
          <button type="button" className="bar-button" onClick={onClose} aria-label="Close">
            <X size={16} />
          </button>
        </Tooltip>
      </header>
      <div className="whats-new-picture">
        {picture ? (
          <img src={picture} alt={card.alt ?? ""} />
        ) : (
          <Sparkles size={40} strokeWidth={1.25} aria-hidden="true" />
        )}
      </div>
      <div className="whats-new-text" aria-live="polite">
        <h3 data-testid="whats-new-card">{card.title}</h3>
        <p>{card.text}</p>
      </div>
      <footer className="whats-new-foot">
        {cards.length > 1 ? (
          <>
            <div className="whats-new-dots" role="tablist" aria-label="Cards">
              {cards.map((c, i) => (
                <button
                  key={`${c.version}:${i}`}
                  type="button"
                  role="tab"
                  aria-selected={i === at}
                  aria-label={c.title}
                  className={i === at ? "is-on" : ""}
                  onClick={() => setAt(i)}
                />
              ))}
            </div>
            <span className="whats-new-count">
              {at + 1} of {cards.length}
            </span>
          </>
        ) : null}
        <span className="whats-new-spacer" />
        {card.action ? (
          <button
            type="button"
            className="chrome-button"
            onClick={() => {
              onClose();
              onAction(card.action!);
            }}
          >
            {card.action.label}
          </button>
        ) : null}
        {cards.length > 1 ? (
          <button type="button" className="chrome-button" disabled={at === 0} onClick={() => setAt(at - 1)}>
            Back
          </button>
        ) : null}
        <button
          ref={next}
          type="button"
          className="chrome-button button-primary"
          data-testid="whats-new-next"
          onClick={() => (last ? onClose() : setAt(at + 1))}
        >
          {last ? "Done" : "Next"}
        </button>
      </footer>
    </Dialog>
  );
}

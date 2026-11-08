// A find bar, as a browser's find in page has: on the review screen it
// searches the text of the plugin's view, which does the searching through
// the SDK, since its frame keeps its page to itself; in a session's thread
// it searches the thread. The bar sends the query and shows the count it
// gets back. Enter goes to the next match, Shift+Enter to
// the previous one, and Escape closes the bar.

import { ChevronDown, ChevronUp, X } from "lucide-react";
import { useEffect, useRef } from "react";
import type { Found } from "../lib/findText";
import { SHIFT } from "../lib/keys";
import { Tooltip } from "./Tooltip";

/** The most matches a view counts; more show as "1000+". */
const FIND_LIMIT = 1000;

type Props = {
  /** the search, which outlives the bar: it comes back when the bar opens again */
  query: string;
  onQuery: (query: string) => void;
  found: Found | null;
  /** searches for the query: the next match when it is the one before */
  onFind: (query: string, backwards?: boolean) => void;
  onClose: () => void;
  /** changes each time the bar is asked for again, which focuses it */
  focus: number;
  /** what the bar searches, as its field and its landmark name it */
  label?: string;
};

export function FindBar({ query, onQuery, found, onFind, onClose, focus, label = "Find in the view" }: Props) {
  const input = useRef<HTMLInputElement>(null);

  // the last search is shown again as the bar opens
  useEffect(() => {
    if (query) onFind(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, as the bar opens
  }, []);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focus]);

  const change = (value: string) => {
    onQuery(value);
    onFind(value);
  };
  const step = (backwards: boolean) => {
    if (query) onFind(query, backwards);
  };

  const count = found && query ? found.count : null;
  const status =
    count === null
      ? ""
      : count === 0
        ? "No matches"
        : `${found!.current} of ${count >= FIND_LIMIT ? `${FIND_LIMIT}+` : count}`;

  return (
    <div className="find-bar" role="search" aria-label={label}>
      <input
        ref={input}
        type="text"
        value={query}
        placeholder={label}
        aria-label={label}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        onChange={(e) => change(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            step(e.shiftKey);
          } else if (e.key === "Escape") {
            // the bar closes; the view stays maximized
            e.preventDefault();
            e.stopPropagation();
            onClose();
          }
        }}
      />
      <span className={`find-count ${count === 0 ? "is-none" : ""}`} role="status" aria-live="polite">
        {status}
      </span>
      <Tooltip label="Previous match" keys={[SHIFT, "Enter"]} side="bottom">
        <button
          type="button"
          className="bar-button"
          aria-label="Previous match"
          disabled={!count}
          onClick={() => step(true)}
        >
          <ChevronUp size={15} />
        </button>
      </Tooltip>
      <Tooltip label="Next match" keys={["Enter"]} side="bottom">
        <button
          type="button"
          className="bar-button"
          aria-label="Next match"
          disabled={!count}
          onClick={() => step(false)}
        >
          <ChevronDown size={15} />
        </button>
      </Tooltip>
      <Tooltip label="Close" keys={["Esc"]} side="bottom">
        <button type="button" className="bar-button" aria-label="Close the find bar" onClick={onClose}>
          <X size={15} />
        </button>
      </Tooltip>
    </div>
  );
}

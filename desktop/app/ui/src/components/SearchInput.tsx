// The input of a page's search field. React writes an input's text again
// each time it draws the input, so this one is drawn again only when its
// own text or labels change: text the browser already has, which React
// hears of at the end of the same event, is still there when the page
// draws again in between, as it does when an answer or a notice arrives.

import { useMemo, type Ref } from "react";
import { useLatest } from "../lib/useLatest";

export function SearchInput({
  ref,
  id,
  value,
  onChange,
  label,
  placeholder,
}: {
  ref?: Ref<HTMLInputElement>;
  id?: string;
  value: string;
  /** called with the field's text when the person changes it */
  onChange: (value: string) => void;
  /** the field's accessible name */
  label: string;
  placeholder: string;
}) {
  const changed = useLatest(onChange);
  return useMemo(
    () => (
      <input
        ref={ref}
        id={id}
        type="search"
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        placeholder={placeholder}
        aria-label={label}
        value={value}
        onChange={(event) => changed.current(event.target.value)}
      />
    ),
    [ref, id, placeholder, label, value, changed],
  );
}

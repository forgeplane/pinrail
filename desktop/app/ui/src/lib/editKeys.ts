// The keys of a field edited in place: Enter, or ⌘Enter in a field of
// several lines, saves; Esc cancels and goes no further, so the panel or
// page behind it stays open. A key that an input method is composing with
// is the field's.

import type { KeyboardEvent } from "react";
import { composing, hasMod } from "./keys";

export function editKeys({
  onSave,
  onCancel,
  saveOn = "enter",
}: {
  onSave: () => void;
  onCancel: () => void;
  /** "enter" for a line, "mod-enter" where Enter starts a new line */
  saveOn?: "enter" | "mod-enter";
}) {
  return (event: KeyboardEvent<HTMLElement>) => {
    if (composing(event)) return;
    if (event.key === "Enter" && (saveOn === "enter" ? !event.shiftKey : hasMod(event.nativeEvent))) {
      event.preventDefault();
      onSave();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    }
  };
}

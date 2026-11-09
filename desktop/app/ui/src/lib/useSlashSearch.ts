// The / key focuses a page's search field, unless the person is typing in
// another field. A page behind a modal dialog leaves the key to the dialog;
// a field inside the dialog passes `inDialog`.

import { useEffect, type RefObject } from "react";
import { modalOpen, isTyping } from "./keys";

export function useSlashSearch(field: RefObject<HTMLInputElement | null>, { inDialog = false } = {}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      if (!inDialog && modalOpen()) return;
      const el = event.target as HTMLElement | null;
      if (isTyping(el)) return;
      if (!field.current || field.current.disabled) return;
      event.preventDefault();
      field.current.focus();
      field.current.select();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [field, inDialog]);
}

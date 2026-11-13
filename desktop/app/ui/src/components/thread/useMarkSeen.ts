// Marks a finished session seen once the person has had it in view: shown
// for a second while the window is focused. After Mark as unread, the
// session stays unread until it is shown again.

import { useEffect, useState } from "react";
import { api } from "../../api/client";
import type { Session } from "../../api/types";

/** How long a finished session is in view before it counts as seen. */
const SEEN_AFTER_MS = 1_000;

/** Whether the window is focused and shown, kept up to date. */
function useWindowInView(): boolean {
  const inView = () => document.hasFocus() && document.visibilityState === "visible";
  const [shown, setShown] = useState(inView);
  useEffect(() => {
    const update = () => setShown(inView());
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    document.addEventListener("visibilitychange", update);
    return () => {
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
      document.removeEventListener("visibilitychange", update);
    };
  }, []);
  return shown;
}

/**
 * Marks `session` seen while it is finished and in view, and gives the
 * Mark as unread action, which holds it unread while it stays shown.
 */
export function useMarkSeen(session: Session | null): { markUnread: (() => void) | null } {
  const inView = useWindowInView();
  const key = session ? `${session.agent}/${session.id}` : null;
  // the session the person marked unread while it was shown
  const [unread, setUnread] = useState<string | null>(null);
  // showing another session ends the hold
  const [shownKey, setShownKey] = useState(key);
  if (key !== shownKey) {
    setShownKey(key);
    setUnread(null);
  }
  const finished = session?.attention === "finished";
  useEffect(() => {
    if (!session || !finished || !inView || unread === key) return;
    const timer = window.setTimeout(() => {
      void api.setSeen(session.agent, session.id, true).catch(() => {});
    }, SEEN_AFTER_MS);
    return () => window.clearTimeout(timer);
  }, [session, finished, inView, unread, key]);

  // a turn that ended was seen since: it can be marked unread
  const p = session?.presence;
  const seen = !!p?.finished_at && !!p.seen_at && p.seen_at >= p.finished_at;
  const markUnread =
    session && seen
      ? () => {
          setUnread(key);
          void api.setSeen(session.agent, session.id, false).catch(() => {});
        }
      : null;
  return { markUnread };
}

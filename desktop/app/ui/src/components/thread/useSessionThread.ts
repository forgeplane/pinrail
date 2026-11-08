// A session's thread as the page shows it: the session, what it sent and
// said, the person's messages, its agent's comments and changes to work
// items, and the items it holds or worked on, loaded again when a live
// notice is about any of them.

import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, EVERY_ITEM_STATUS } from "../../api/client";
import type {
  Review,
  Session,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
  WorkItem,
} from "../../api/types";
import { useLive } from "../../state/live";

export function useSessionThread(
  named: { agent: string; id: string } | null,
  focus: Review | undefined,
  onSession?: (session: Session) => void,
) {
  const live = useLive();
  // the work items the session holds or worked on, with what changes them
  const [sessionItems, setSessionItems] = useState<WorkItem[]>([]);
  useEffect(() => {
    if (!named) return;
    let cancelled = false;
    api
      .items({ agent: named.agent, session: named.id, status: EVERY_ITEM_STATUS })
      .then((d) => !cancelled && setSessionItems(d.items))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [named?.agent, named?.id, live.lastNotice]); // eslint-disable-line react-hooks/exhaustive-deps -- the session's names, not the object
  const [session, setSession] = useState<Session | null>(null);
  const [items, setItems] = useState<Review[]>(focus ? [focus] : []);
  const [statuses, setStatuses] = useState<SessionStatus[]>([]);
  const [messages, setMessages] = useState<SessionMessage[]>([]);
  const [comments, setComments] = useState<SessionComment[]>([]);
  const [itemEvents, setItemEvents] = useState<SessionItemEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      if (named) {
        const thread = await api.session(named.agent, named.id);
        setSession(thread.session);
        onSession?.(thread.session);
        setItems(thread.items);
        setStatuses(thread.statuses);
        setMessages(thread.messages ?? []);
        setComments(thread.comments ?? []);
        setItemEvents(thread.item_events ?? []);
      } else if (focus) {
        setItems([await api.getReview(focus.id)]);
      }
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "The server did not answer.");
    }
  }, [named?.agent, named?.id, focus?.id]); // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the objects
  useEffect(() => {
    load();
  }, [load]);

  // anything about one of its reviews, or a new one from the session
  const ids = useRef(new Set<string>());
  ids.current = new Set(items.map((r) => r.id));
  useEffect(() => {
    const notice = live.lastNotice;
    if (!notice) return;
    const ours =
      (notice.review_id && ids.current.has(notice.review_id)) ||
      (named && notice.review?.requested_by === named.agent && notice.review?.session === named.id) ||
      (named && notice.session?.agent === named.agent && notice.session.id === named.id) ||
      // a comment its agent wrote on a work item, or a change to its items
      (named && notice.kind === "item_changed");
    if (ours) load();
  }, [live.lastNotice]); // eslint-disable-line react-hooks/exhaustive-deps -- each notice once

  return { session, items, statuses, messages, comments, itemEvents, error, load, sessionItems };
}

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
  // how many times its items changed, which loads them again
  const [itemsChanged, setItemsChanged] = useState(0);
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
  }, [named?.agent, named?.id, itemsChanged]); // eslint-disable-line react-hooks/exhaustive-deps -- the session's names, not the object
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
  // the items it holds, worked on or commented on
  const keys = useRef(new Set<string>());
  keys.current = new Set([
    ...sessionItems.map((i) => i.key),
    ...comments.map((c) => c.item.key),
    ...itemEvents.map((e) => e.item.key),
  ]);
  useEffect(() => {
    const notice = live.lastNotice;
    if (!notice) return;
    // a change its agent made to an item, or any change to one of its items
    const itemOurs =
      !!named &&
      !!notice.item &&
      ((notice.item.agent === named.agent && notice.item.session === named.id) || keys.current.has(notice.item.key));
    const ours =
      itemOurs ||
      (notice.review_id && ids.current.has(notice.review_id)) ||
      (named && notice.review?.requested_by === named.agent && notice.review?.session === named.id) ||
      (named && notice.session?.agent === named.agent && notice.session.id === named.id);
    if (itemOurs) setItemsChanged((n) => n + 1);
    if (ours) load();
  }, [live.lastNotice]); // eslint-disable-line react-hooks/exhaustive-deps -- each notice once

  return { session, items, statuses, messages, comments, itemEvents, error, load, sessionItems };
}

// A session's thread as the page shows it: the session, what it sent and
// said, the person's messages, its agent's comments and changes to work
// items, and the items it holds or worked on, loaded again when a live
// notice is about any of them.

import { useCallback, useEffect, useRef, useState } from "react";
import { api, EVERY_ITEM_STATUS, errorMessage } from "../../api/client";
import type {
  Notice,
  Review,
  Session,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
  WorkItem,
} from "../../api/types";
import { noticeItem, useConnected, useNotice } from "../../state/live";
import { useLoad } from "../../lib/useLoad";
import { useLatest } from "../../lib/useLatest";

/** What a live notice changes in a session's thread: `thread` when the
 *  thread is to load again, `item` when one of its work items changed.
 *  `reviews` are the thread's reviews by id, and `items` the keys of the
 *  items it holds, worked on or commented on. */
export function aboutThread(
  notice: Notice,
  named: { agent: string; id: string } | null,
  reviews: Set<string>,
  items: Set<string>,
): { thread: boolean; item: boolean } {
  // a change its agent made to an item, any change to one of its items, or
  // a new prefix, which may rename them
  const changed = noticeItem(notice);
  const item =
    !!named &&
    (notice.kind === "prefix_changed" ||
      (!!changed && ((changed.agent === named.agent && changed.session === named.id) || items.has(changed.key))));
  const thread =
    item ||
    (!!notice.review_id && reviews.has(notice.review_id)) ||
    (!!named && notice.review?.requested_by === named.agent && notice.review?.session === named.id) ||
    (!!named && notice.session?.agent === named.agent && notice.session.id === named.id);
  return { thread, item };
}

/** No items, the same array each time, while none have loaded. */
const NO_ITEMS: WorkItem[] = [];

export function useSessionThread(
  named: { agent: string; id: string } | null,
  focus: Review | undefined,
  onSession?: (session: Session) => void,
) {
  // the work items the session holds or worked on, with what changes them
  // how many times its items changed, which loads them again
  const [itemsChanged, setItemsChanged] = useState(0);
  const sessionItems =
    useLoad(
      () =>
        named
          ? api.items({ agent: named.agent, session: named.id, status: EVERY_ITEM_STATUS }).then((d) => d.items)
          : Promise.resolve<WorkItem[]>([]),
      [named?.agent, named?.id, itemsChanged],
    ).data ?? NO_ITEMS;
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
      setError(errorMessage(e, "The server did not answer."));
    }
  }, [named?.agent, named?.id, focus?.id]); // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the objects
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- loads from the core
    load();
  }, [load]);
  // and again when the event stream opens: what changed before it opened,
  // such as while the thread loaded at startup, reaches no notice
  const connected = useConnected();
  const wasConnected = useRef(connected);
  useEffect(() => {
    if (connected && !wasConnected.current) load();
    wasConnected.current = connected;
  }, [connected, load]);

  // anything about one of its reviews, or a new one from the session
  const ids = useLatest(new Set(items.map((r) => r.id)));
  // the items it holds, worked on or commented on
  const keys = useLatest(
    new Set([
      ...sessionItems.map((i) => i.key),
      ...comments.map((c) => c.item.key),
      ...itemEvents.map((e) => e.item.key),
    ]),
  );
  useNotice((notice) => {
    const { thread, item } = aboutThread(notice, named, ids.current, keys.current);
    if (item) setItemsChanged((n) => n + 1);
    if (thread) load();
  });

  return { session, items, statuses, messages, comments, itemEvents, error, load, sessionItems };
}

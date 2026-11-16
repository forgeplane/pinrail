// A session's thread as the page shows it: the session, what it sent and
// said, the person's messages, its agent's comments and changes to work
// items, and the items it holds or worked on, loaded again when a live
// notice is about any of them.

import { useCallback, useEffect, useRef, useState } from "react";
import { api, EVERY_ITEM_STATUS, errorMessage } from "../../api/client";
import type {
  SessionTurn,
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
import { reuse } from "../../lib/reuse";

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

/** A session's thread as last loaded. */
type Loaded = {
  session: Session;
  items: Review[];
  statuses: SessionStatus[];
  messages: SessionMessage[];
  comments: SessionComment[];
  itemEvents: SessionItemEvent[];
  turns: SessionTurn[];
};

/** How many threads opened last are kept to show at once when opened again. */
const KEPT = 4;
/** The threads opened last, by `agent/id`, the newest last. Opening one
 *  shows it at once from here, and loads it again as usual. */
const kept = new Map<string, Loaded>();

function keep(key: string, loaded: Loaded) {
  kept.delete(key);
  kept.set(key, loaded);
  while (kept.size > KEPT) kept.delete(kept.keys().next().value!);
}

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
  // a thread opened lately shows as it was until it has loaded again
  const key = named ? `${named.agent}/${named.id}` : null;
  const [cached] = useState(() => (key ? kept.get(key) : undefined));
  const [session, setSession] = useState<Session | null>(cached?.session ?? null);
  const [items, setItems] = useState<Review[]>(cached?.items ?? (focus ? [focus] : []));
  const [statuses, setStatuses] = useState<SessionStatus[]>(cached?.statuses ?? []);
  const [messages, setMessages] = useState<SessionMessage[]>(cached?.messages ?? []);
  const [comments, setComments] = useState<SessionComment[]>(cached?.comments ?? []);
  const [itemEvents, setItemEvents] = useState<SessionItemEvent[]>(cached?.itemEvents ?? []);
  const [turns, setTurns] = useState<SessionTurn[]>(cached?.turns ?? []);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (cached) onSession?.(cached.session);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- once, for the thread it opened with
  // what it shows now, for the next time it is opened
  useEffect(() => {
    if (key && session) keep(key, { session, items, statuses, messages, comments, itemEvents, turns });
  }, [key, session, items, statuses, messages, comments, itemEvents, turns]);

  // one load at a time: notices that come while one runs are answered by
  // one more load after it, however many they are
  const loading = useRef<{ key: string; running: boolean; again: boolean } | null>(null);
  const load = useCallback(async (): Promise<void> => {
    const key = named ? `${named.agent}/${named.id}` : (focus?.id ?? "");
    const current = loading.current;
    if (current?.running && current.key === key) {
      current.again = true;
      return;
    }
    const mine = { key, running: true, again: false };
    loading.current = mine;
    try {
      if (named) {
        const thread = await api.session(named.agent, named.id);
        setSession(thread.session);
        onSession?.(thread.session);
        // what did not change stays the same objects, which the thread's
        // entries do not draw again
        setItems((prev) => reuse(prev, thread.items, (r) => r.id));
        setStatuses((prev) => reuse(prev, thread.statuses, (s) => s.id));
        setMessages((prev) => reuse(prev, thread.messages ?? [], (m) => m.id));
        setComments((prev) => reuse(prev, thread.comments ?? [], (c) => c.id));
        setItemEvents((prev) => reuse(prev, thread.item_events ?? [], (e) => e.id));
        setTurns((prev) => reuse(prev, thread.turns ?? [], (t) => t.turn));
      } else if (focus) {
        setItems([await api.getReview(focus.id)]);
      }
      setError(null);
    } catch (e) {
      setError(errorMessage(e, "The server did not answer."));
    } finally {
      mine.running = false;
      if (mine.again && loading.current === mine) await load();
    }
  }, [named?.agent, named?.id, focus?.id]); // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the objects
  useEffect(() => {
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

  return { session, items, statuses, messages, comments, itemEvents, turns, error, load, sessionItems };
}

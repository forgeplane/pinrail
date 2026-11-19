// The view a session shows, Summary or Terminal, and the last one used for
// each session, kept in the browser's storage for the last 500 sessions.

/** Summary, Terminal, or both, the summary above the terminal. */
export type SessionView = "summary" | "terminal" | "split";

const KEY = "pinrail:session-views";
/** How many sessions keep their last view. */
export const KEPT = 500;

type Store = Pick<Storage, "getItem" | "setItem">;

function read(store: Store): [string, SessionView][] {
  try {
    const saved = JSON.parse(store.getItem(KEY) ?? "[]") as unknown;
    if (!Array.isArray(saved)) return [];
    return saved.filter(
      (e): e is [string, SessionView] =>
        Array.isArray(e) && typeof e[0] === "string" && (e[1] === "summary" || e[1] === "terminal" || e[1] === "split"),
    );
  } catch {
    return [];
  }
}

/** The view last used for a session, if one was. */
export function lastView(session: string, store: Store = localStorage): SessionView | null {
  return read(store).find(([s]) => s === session)?.[1] ?? null;
}

/** Records the view a session shows; the oldest beyond 500 are dropped. */
export function rememberView(session: string, view: SessionView, store: Store = localStorage) {
  const kept = read(store).filter(([s]) => s !== session);
  kept.push([session, view]);
  try {
    store.setItem(KEY, JSON.stringify(kept.slice(-KEPT)));
  } catch {
    // no storage: the session opens in the default view next time
  }
}

/** The view a session opens in: the one the address names, else the one
 *  the setting gives, and the summary when the session has no terminal. */
export function openingView(
  asked: string | null,
  setting: "last" | SessionView,
  last: SessionView | null,
  hasTerminal: boolean,
): SessionView {
  if (!hasTerminal) return "summary";
  if (asked === "terminal" || asked === "summary" || asked === "split") return asked;
  if (setting === "last") return last ?? "summary";
  return setting;
}

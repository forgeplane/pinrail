// What a screen puts in the top bar: a breadcrumb in place of the page
// name. Cleared when the screen leaves.

import { createContext, useCallback, useContext, useEffect, useId, useMemo, useState, type ReactNode } from "react";
import type { PluginShortcut } from "../api/types";

export type TopBarContent = {
  /** the breadcrumb, shown instead of the page title */
  crumb?: ReactNode;
  /** the plugin whose view is open, for the keyboard-shortcuts dialog */
  plugin?: { name: string; title: string; icon: string | null; shortcuts: PluginShortcut[] };
};

type TopBar = { content: TopBarContent | null; set: (key: string, content: TopBarContent | null) => void };

const TopBarContext = createContext<TopBar | null>(null);

/** The screens' contents, merged in the order the screens mounted: a
 *  review in a side panel adds its plugin to the breadcrumb of the screen
 *  beside it. */
function merged(parts: Map<string, TopBarContent>): TopBarContent | null {
  if (parts.size === 0) return null;
  const out: TopBarContent = {};
  for (const part of parts.values()) {
    if (part.crumb !== undefined) out.crumb = part.crumb;
    if (part.plugin !== undefined) out.plugin = part.plugin;
  }
  return out;
}

export function TopBarProvider({ children }: { children: ReactNode }) {
  const [parts, setParts] = useState<Map<string, TopBarContent>>(() => new Map());
  const set = useCallback((key: string, content: TopBarContent | null) => {
    setParts((now) => {
      const next = new Map(now);
      if (content) next.set(key, content);
      else next.delete(key);
      return next;
    });
  }, []);
  const value = useMemo(() => ({ content: merged(parts), set }), [parts, set]);
  return <TopBarContext.Provider value={value}>{children}</TopBarContext.Provider>;
}

export function useTopBarContent(): TopBarContent | null {
  return useContext(TopBarContext)?.content ?? null;
}

/** Sets the top bar for as long as the calling screen is mounted. */
export function useTopBar(content: TopBarContent | null) {
  const bar = useContext(TopBarContext);
  const set = bar?.set;
  const key = useId();
  // the screen re-renders with fresh nodes; the bar follows them, and
  // keeps the screen in its place among the others
  useEffect(() => {
    set?.(key, content);
  }, [set, key, content]);
  useEffect(() => () => set?.(key, null), [set, key]);
}

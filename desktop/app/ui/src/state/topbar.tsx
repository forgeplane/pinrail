// What a screen puts in the top bar: a breadcrumb in place of the page
// name. Cleared when the screen leaves.

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import type { PluginShortcut } from "../api/types";

export type TopBarContent = {
  /** the breadcrumb, shown instead of the page title */
  crumb?: ReactNode;
  /** the plugin whose view is open, for the keyboard-shortcuts dialog */
  plugin?: { name: string; title: string; icon: string | null; shortcuts: PluginShortcut[] };
};

type TopBar = { content: TopBarContent | null; set: (content: TopBarContent | null) => void };

const TopBarContext = createContext<TopBar | null>(null);

export function TopBarProvider({ children }: { children: ReactNode }) {
  const [content, set] = useState<TopBarContent | null>(null);
  const value = useMemo(() => ({ content, set }), [content]);
  return <TopBarContext.Provider value={value}>{children}</TopBarContext.Provider>;
}

export function useTopBarContent(): TopBarContent | null {
  return useContext(TopBarContext)?.content ?? null;
}

/** Sets the top bar for as long as the calling screen is mounted. */
export function useTopBar(content: TopBarContent | null) {
  const bar = useContext(TopBarContext);
  const set = bar?.set;
  useEffect(() => {
    set?.(content);
    return () => set?.(null);
    // the screen re-renders with fresh nodes; the bar follows them
  }, [set, content]);
}

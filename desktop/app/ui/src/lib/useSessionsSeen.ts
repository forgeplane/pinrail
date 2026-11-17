// What the window does with each list of sessions it loads: it keeps them,
// so a session's page shows its header at once, and announces an element
// that no session had before when it is rare or legendary.

import { useCallback } from "react";
import type { Session } from "../api/types";
import { useToast } from "../state/toasts";
import { discover } from "./elements";
import { rememberSessions } from "./knownSessions";

export function useSessionsSeen(): (sessions: Session[]) => void {
  const toast = useToast();
  return useCallback(
    (sessions: Session[]) => {
      rememberSessions(sessions);
      for (const { element, count } of discover(sessions)) {
        if (element.rarity !== "rare" && element.rarity !== "legendary") continue;
        const by = sessions.find((s) => s.element === element.number);
        const name = by?.generated_name ?? by?.name;
        toast(
          `New ${element.rarity} element discovered: ${element.name}${name ? `, by ${name}` : ""}. Your sessions have found ${count} of 118 elements.`,
        );
      }
    },
    [toast],
  );
}

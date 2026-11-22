// The chip beside a session's title: what the session asks of the person,
// and what explains it, such as the tool waiting for permission or how long
// a turn has worked.

import { useEffect, useState } from "react";
import type { Session } from "../api/types";
import { chipOf } from "../lib/sidebarSessions";

/** How often a working turn's duration moves on. */
const TICK_MS = 15_000;

export function PresenceChip({ session }: { session: Session }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS);
    return () => window.clearInterval(timer);
  }, []);
  const chip = chipOf(session, now);
  // a screen reader hears when the session needs the person or finished,
  // not each update of a working turn
  const announced = chip && (chip.state === "needs_you" || chip.state === "finished") ? chip.text : "";
  return (
    <>
      {chip ? (
        <span className={`presence-chip is-${chip.state}`} data-testid="presence-chip" title={chip.text}>
          <span className={`state-mark is-${chip.state}`} aria-hidden />
          <span className="presence-chip-text">{chip.text}</span>
        </span>
      ) : null}
      <span className="presence-announce" aria-live="polite">
        {announced}
      </span>
    </>
  );
}

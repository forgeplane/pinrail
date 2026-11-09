// The time now, as of the last tick: ages shown in a list move on with it,
// and every render of one tick reads the same time.

import { useEffect, useState } from "react";

export function useNow(everyMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), everyMs);
    return () => window.clearInterval(timer);
  }, [everyMs]);
  return now;
}

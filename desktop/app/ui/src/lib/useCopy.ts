// Copying to the clipboard with a moment of feedback: "done" or "failed"
// for a while after a copy, then nothing again. A second copy starts the
// moment over, and the timer goes with the component.

import { useCallback, useEffect, useRef, useState } from "react";
import { copyText } from "./clipboard";

export type CopyState = "done" | "failed" | null;

/** Copies text, or what a function answers, and says how it went for `duration` ms. */
export function useCopy(duration = 3500) {
  const [state, setState] = useState<CopyState>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = useCallback(
    async (what: string | (() => Promise<string>)) => {
      window.clearTimeout(timer.current);
      try {
        await copyText(typeof what === "string" ? what : await what());
        setState("done");
      } catch {
        setState("failed");
      }
      timer.current = window.setTimeout(() => setState(null), duration);
    },
    [duration],
  );
  return { state, copy };
}

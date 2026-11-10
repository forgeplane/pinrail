// A key listener on the window that always calls the component's latest
// handler, registered once while it is enabled: no dependency list to keep
// up to date, and no refs copied during render to read current values. It
// is in place before the browser paints what it belongs to, so a key
// pressed the moment a panel shows is not lost.

import { useEffectEvent, useLayoutEffect } from "react";

export function useWindowKey(
  handler: (event: KeyboardEvent) => void,
  { capture = false, enabled = true }: { capture?: boolean; enabled?: boolean } = {},
) {
  const onKey = useEffectEvent(handler);
  useLayoutEffect(() => {
    if (!enabled) return;
    const listener = (event: KeyboardEvent) => onKey(event);
    window.addEventListener("keydown", listener, capture);
    return () => window.removeEventListener("keydown", listener, capture);
  }, [enabled, capture]);
}

// What the app says back: a short line that appears, stays long enough to
// read, and goes. For what a person just did, not for what needs deciding;
// anything that needs deciding is a review.

import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";

export type ToastTone = "ok" | "danger";

export type Toast = {
  id: number;
  text: string;
  tone: ToastTone;
};

/** How long a line stays. A failure stays longer: it is worth reading twice. */
const LIFETIME: Record<ToastTone, number> = { ok: 4000, danger: 7000 };

type Toasts = {
  toasts: Toast[];
  notify: (text: string, tone?: ToastTone) => void;
  dismiss: (id: number) => void;
};

const ToastContext = createContext<Toasts | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);
  const timers = useRef(new Map<number, number>());

  const dismiss = useCallback((id: number) => {
    setToasts((all) => all.filter((t) => t.id !== id));
    const timer = timers.current.get(id);
    if (timer) window.clearTimeout(timer);
    timers.current.delete(id);
  }, []);

  const notify = useCallback(
    (text: string, tone: ToastTone = "ok") => {
      const id = next.current++;
      // the newest at the end; a handful at most, so the oldest go
      setToasts((all) => [...all, { id, text, tone }].slice(-4));
      timers.current.set(
        id,
        window.setTimeout(() => dismiss(id), LIFETIME[tone]),
      );
    },
    [dismiss],
  );

  const value = useMemo(() => ({ toasts, notify, dismiss }), [toasts, notify, dismiss]);
  return <ToastContext.Provider value={value}>{children}</ToastContext.Provider>;
}

/** Says something to the person: `notify("Plugin removed")`. */
export function useToast(): (text: string, tone?: ToastTone) => void {
  const store = useContext(ToastContext);
  if (!store) throw new Error("useToast outside ToastProvider");
  return store.notify;
}

export function useToasts(): Toasts {
  const store = useContext(ToastContext);
  if (!store) throw new Error("useToasts outside ToastProvider");
  return store;
}

// The lines the app says back, stacked in the corner. Each one goes on its
// own; clicking dismisses it early. They are announcements, so a screen
// reader hears them without the focus moving.

import { CircleAlert, CircleCheck, X } from "lucide-react";
import { useToasts } from "../state/toasts";

export function Toasts() {
  const { toasts, dismiss } = useToasts();
  if (toasts.length === 0) return null;
  return (
    <div className="toasts" role="region" aria-label="Notifications" data-toasts>
      {toasts.map((toast) => (
        <output key={toast.id} className={`toast toast-${toast.tone}`} data-toast>
          {toast.tone === "danger" ? <CircleAlert size={15} /> : <CircleCheck size={15} />}
          <span className="toast-text">{toast.text}</span>
          <button type="button" className="toast-close" onClick={() => dismiss(toast.id)} aria-label="Dismiss">
            <X size={13} />
          </button>
        </output>
      ))}
    </div>
  );
}

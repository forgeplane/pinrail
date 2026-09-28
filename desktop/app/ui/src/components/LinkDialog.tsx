// A plugin's view asked the app to open a link. The app shows where it
// goes, outside the view so the view cannot draw or answer it, and opens it
// only when the person agrees. Esc, or a click outside, opens nothing.

import { useEffect, useRef } from "react";
import type { LinkRequest } from "../lib/links";

export type LinkChoice = "cancel" | "once" | "always";

export function LinkDialog({
  plugin,
  request,
  onClose,
}: {
  plugin: string;
  request: LinkRequest;
  onClose: (choice: LinkChoice) => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  const canAlways = request.kind === "web" && !request.long;

  useEffect(() => {
    cancel.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose("cancel");
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="app-dialog-backdrop" onMouseDown={() => onClose("cancel")}>
      <div
        className="app-dialog link-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="link-title"
        onMouseDown={(e) => e.stopPropagation()}
        data-link-dialog
      >
        <div className="dialog-head">
          <h2 id="link-title">{request.kind === "mail" ? "Write an email?" : "Open a link?"}</h2>
        </div>
        <p className="dim">
          <span className="text">{plugin}</span> wants to{" "}
          {request.kind === "mail" ? "start an email to" : "open a page on"}
        </p>
        <p className="link-target" data-link-target>
          {request.target}
        </p>
        <pre className="link-url mono" data-link-url>
          {request.url}
        </pre>
        {request.long ? (
          <p className="notice">
            This address is unusually long, so it can carry a lot of information to the site. Open it only if you
            expected it.
          </p>
        ) : null}
        <div className="dialog-actions">
          <button
            ref={cancel}
            type="button"
            className="chrome-button"
            onClick={() => onClose("cancel")}
            data-link-cancel
          >
            Don't open
          </button>
          <button type="button" className="chrome-button" onClick={() => onClose("once")} data-link-once>
            {request.kind === "mail" ? "Write it" : "Open once"}
          </button>
          {canAlways ? (
            <button type="button" className="chrome-button" onClick={() => onClose("always")} data-link-always>
              Always allow {request.target}
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}

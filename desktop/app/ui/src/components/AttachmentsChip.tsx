// The files a review carries, beside its plugin badge: how many and how big,
// and on a click each one by name, size, type and hash, to save. The same
// for every plugin, which cannot hide it: what came with a review is the
// person's to see before they decide on it.

import { Download, FileBox } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { inTauri, serverUrl } from "../api/client";
import type { Attachment } from "../api/types";
import { size } from "../lib/format";
import { useToasts } from "../state/toasts";

const GAP = 6;
const MARGIN = 8;

type Props = {
  reviewId: string;
  attachments: Attachment[];
};

export function AttachmentsChip({ reviewId, attachments }: Props) {
  const { notify } = useToasts();
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const panel = useRef<HTMLDivElement | null>(null);
  const total = attachments.reduce((sum, a) => sum + a.size, 0);

  useLayoutEffect(() => {
    if (!open || !trigger.current || !panel.current) return;
    const at = trigger.current.getBoundingClientRect();
    const width = panel.current.offsetWidth;
    const left = Math.min(Math.max(MARGIN, at.left), window.innerWidth - width - MARGIN);
    setPos({ top: at.bottom + GAP, left });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const close = (event: Event) => {
      const target = event.target as Node | null;
      if (target && (panel.current?.contains(target) || trigger.current?.contains(target))) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
    };
  }, [open]);

  const save = useCallback(
    async (name: string) => {
      if (inTauri()) {
        try {
          const { invoke } = await import("@tauri-apps/api/core");
          const saved = await invoke<string | null>("save_attachment", { review: reviewId, name });
          if (saved) notify(`Saved ${name} to ${saved}`);
        } catch (error) {
          notify(String(error), "danger");
        }
        return;
      }
      // outside the app: the core answers with an attachment, so this downloads
      const link = document.createElement("a");
      link.href = `${await serverUrl()}/api/v1/reviews/${reviewId}/attachments/${encodeURIComponent(name)}`;
      link.download = name;
      link.rel = "noreferrer";
      document.body.append(link);
      link.click();
      link.remove();
    },
    [reviewId, notify],
  );

  if (!attachments.length) return null;
  const count = `${attachments.length} file${attachments.length === 1 ? "" : "s"}`;
  return (
    <>
      <button
        ref={trigger}
        type="button"
        className={`attachments-chip ${open ? "is-open" : ""}`}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-haspopup="dialog"
        data-attachments-chip
      >
        <FileBox size={13} /> {count} · {size(total)}
      </button>
      {open
        ? createPortal(
            <div
              ref={panel}
              className="attachments-panel"
              role="dialog"
              aria-label={`Files this review carries: ${count}`}
              style={pos ? { top: pos.top, left: pos.left } : { visibility: "hidden" }}
              data-attachments-panel
            >
              <div className="attachments-head">
                <span>Sent with this review</span>
                <span className="faint">{size(total)}</span>
              </div>
              <ul className="attachments-list">
                {attachments.map((a) => (
                  <li key={a.name} data-attachment={a.name}>
                    <div className="attachment-text">
                      <span className="attachment-name">{a.name}</span>
                      <span className="attachment-meta">
                        {size(a.size)} · {a.media_type} · <span className="mono" title={a.sha256}>{a.sha256.slice(0, 12)}</span>
                      </span>
                    </div>
                    <button type="button" className="chrome-button attachment-save" onClick={() => void save(a.name)} data-attachment-save={a.name}>
                      <Download size={13} /> Save…
                    </button>
                  </li>
                ))}
              </ul>
            </div>,
            document.body,
          )
        : null}
    </>
  );
}

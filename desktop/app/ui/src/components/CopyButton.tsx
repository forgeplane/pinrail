// A button that copies text, with an icon and a tooltip that say how the
// copy went for a moment: the review's and the session's ids, and a
// review's decision as Markdown.

import { ClipboardCheck, ClipboardX, Copy } from "lucide-react";
import { useCopy } from "../lib/useCopy";
import { Tooltip } from "./Tooltip";

export function CopyButton({
  text,
  label,
  ariaLabel,
  className = "id-copy",
  size = 14,
}: {
  /** what to copy, or a function that answers it */
  text: string | (() => Promise<string>);
  /** the tooltip before a copy */
  label: string;
  ariaLabel: string;
  className?: string;
  size?: number;
}) {
  const { state, copy } = useCopy();
  return (
    <Tooltip label={state === "done" ? "Copied" : state === "failed" ? "Could not copy" : label} side="bottom">
      <button
        type="button"
        className={`${className} ${state === "done" ? "ok" : state === "failed" ? "danger" : ""}`}
        onClick={() => void copy(text)}
        aria-label={ariaLabel}
      >
        {state === "done" ? (
          <ClipboardCheck size={size} className="copy-done" />
        ) : state === "failed" ? (
          <ClipboardX size={size} />
        ) : (
          <Copy size={size} />
        )}
      </button>
    </Tooltip>
  );
}

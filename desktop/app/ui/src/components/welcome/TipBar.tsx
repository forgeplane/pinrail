// One short tip under a screen's content: where in the app to find
// something later.

import { Lightbulb } from "lucide-react";
import type { ReactNode } from "react";

export function TipBar({ children }: { children: ReactNode }) {
  return (
    <p className="tip-bar" role="note">
      <span className="tip-bar-icon">
        <Lightbulb size={12} strokeWidth={2.2} aria-hidden="true" />
      </span>
      <span>
        <b className="tip-bar-label">Tip</b>
        {children}
      </span>
    </p>
  );
}

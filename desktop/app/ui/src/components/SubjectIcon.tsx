// What an inbox row is about, as an icon, with what happened to it as a
// badge on the icon's top right: a work item that was added, a session
// whose turn failed, a review's new round.

import { Check, Clock, FileText, Lock, MessageSquare, Pencil, Play, Plus, RefreshCw, Square, X } from "lucide-react";
import type { ComponentType, ReactNode, SVGProps } from "react";
import type { Notification } from "../api/types";

/** A glyph that draws at a size and a stroke, as Lucide's icons do. */
type Glyph = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>;

/** A badge's glyph, its colour and what it says to a screen reader. */
export type Badge = { glyph: Glyph; tone: "new" | "ok" | "fail" | "wait" | "end" | "note"; label: string };

const BADGES = {
  added: { glyph: Plus, tone: "new", label: "Added" },
  commented: { glyph: MessageSquare, tone: "note", label: "Commented" },
  claimed: { glyph: Play, tone: "new", label: "Claimed" },
  done: { glyph: Check, tone: "ok", label: "Done" },
  cancelled: { glyph: X, tone: "end", label: "Cancelled" },
  changed: { glyph: Pencil, tone: "note", label: "Changed" },
  finished: { glyph: Check, tone: "ok", label: "Finished" },
  failed: { glyph: X, tone: "fail", label: "Failed" },
  stopped: { glyph: Square, tone: "end", label: "Stopped" },
  permission: { glyph: Lock, tone: "wait", label: "Waits for a permission" },
  input: { glyph: QuestionGlyph, tone: "wait", label: "Waits for an answer" },
  asks: { glyph: QuestionGlyph, tone: "new", label: "Asks you" },
  round: { glyph: RefreshCw, tone: "new", label: "New round" },
  note: { glyph: FileText, tone: "note", label: "Note" },
  withdrawn: { glyph: X, tone: "end", label: "Withdrawn" },
  expired: { glyph: Clock, tone: "end", label: "Expired" },
} satisfies Record<string, Badge>;

export const BADGE = BADGES as Record<keyof typeof BADGES, Badge>;

/** A question mark alone, which fits a badge better than a circled one. */
function QuestionGlyph(props: SVGProps<SVGSVGElement> & { size?: number | string }) {
  const { size = 24, ...rest } = props;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...rest}
    >
      <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
      <path d="M12 17h.01" />
    </svg>
  );
}

/** The badge of a notification, by why it was added: none for a new review,
 *  whose icon says it all. */
export function badgeOf(n: Notification): Badge | null {
  switch (n.reason) {
    case "item_added":
      return BADGE.added;
    case "item_commented":
      return BADGE.commented;
    case "item_claimed":
      return BADGE.claimed;
    case "item_closed":
      return n.subject.kind === "item" && n.subject.item.status === "cancelled" ? BADGE.cancelled : BADGE.done;
    case "item_changed":
      return BADGE.changed;
    case "agent_finished":
      return BADGE.finished;
    case "agent_failed":
      return BADGE.failed;
    case "agent_stopped":
      return BADGE.stopped;
    case "permission_requested":
      return BADGE.permission;
    case "input_requested":
      return BADGE.input;
    case "question_asked":
      return BADGE.asks;
    case "new_round":
      return BADGE.round;
    case "agent_note":
      return BADGE.note;
    case "review_withdrawn":
      return BADGE.withdrawn;
    case "review_expired":
      return BADGE.expired;
    default:
      return null;
  }
}

export function SubjectIcon({ children, badge }: { children: ReactNode; badge: Badge | null }) {
  return (
    <span className="subject-icon">
      {children}
      {badge ? (
        <span className={`subject-badge is-${badge.tone}`} role="img" aria-label={badge.label} title={badge.label}>
          <badge.glyph size={8} strokeWidth={3.2} aria-hidden="true" />
        </span>
      ) : null}
    </span>
  );
}

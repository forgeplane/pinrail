// How a decision, a discard or a withdrawal was made when it was not made in
// the app: a CLI or API badge, with the details in its tooltip. A decision or
// a discard made from the agent session that submitted the review gets a
// warning, since that is an agent answering its own request; withdrawing its
// own review is what an agent is expected to do. Nothing is shown for one
// made in the app, or recorded before the app kept where it came from.

import { TriangleAlert } from "lucide-react";
import type { Review, Via } from "../api/types";
import { Tooltip } from "./Tooltip";

type Made = { via?: Via | null; session?: string | null };
type Command = "decide" | "discard" | "withdraw";

const ACTIONS: Record<Command, string> = { decide: "Decided", discard: "Discarded", withdraw: "Withdrawn" };

/** The source of a decision, a discard or a withdrawal, and whether it came
 *  from the session that asked; null for one made in the app. */
export function madeWith(review: Review, made: Made, command: Command) {
  if (made.via !== "cli" && made.via !== "api") return null;
  const asker = !!made.session && made.session === review.session;
  const own = asker && command !== "withdraw";
  const action = ACTIONS[command];
  const how = made.via === "cli" ? `${action} with pinrail ${command}` : `${action} by a program through the API`;
  const from = asker
    ? ", from the agent session that submitted the review"
    : made.session
      ? ", from another agent session"
      : "";
  return { badge: made.via === "cli" ? "CLI" : "API", own, label: `${how}${from}` };
}

/** The source as a badge after "Decided by …", "… discarded" or "… withdrew". */
export function SourceBadge({ review, made, command }: { review: Review; made: Made; command: Command }) {
  const source = madeWith(review, made, command);
  if (!source) return null;
  return (
    <Tooltip label={source.label}>
      <span
        className={`source-badge ${source.own ? "is-own" : ""}`}
        data-testid="source-badge"
        tabIndex={0}
        aria-label={source.label}
      >
        {source.own ? <TriangleAlert size={10} aria-hidden="true" /> : null}
        {source.badge}
      </span>
    </Tooltip>
  );
}

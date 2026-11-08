// A work item's comments and activity as one timeline: runs of changes as
// short lines on a rail, each with an icon and who did it, a long run
// folded behind one line, and runs of comments as cards, the comment
// field at the end of the last one.

import {
  ChevronsUpDown,
  FileText,
  FolderGit2,
  GitPullRequestArrow,
  Link2,
  ListTree,
  Paperclip,
  Pencil,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import type { ItemComment, ItemEntry, ItemStatus } from "../api/types";
import { age, stamp } from "../lib/format";
import { AgentIcon } from "./AgentIcon";
import { ItemStatusIcon, STATUS_LABEL } from "./ItemStatusIcon";

type Change = Extract<ItemEntry, { type: "event" }>;
type Comment = ItemComment & { type: "comment" };

/** Who did something: you, or the agent by its name. */
export const who = (actor: string) => (actor === "person" ? "You" : actor);

/** A change on the timeline, in words. */
export function said(entry: Extract<ItemEntry, { type: "event" }>): string {
  const a = (entry.attrs ?? {}) as Record<string, unknown>;
  const list = (v: unknown) => (Array.isArray(v) ? (v as string[]) : []);
  switch (entry.kind) {
    case "created":
      return "added it";
    case "edited":
      return `changed its ${list(a.fields).join(", ")}`;
    case "files": {
      const parts = [];
      if (list(a.added).length) parts.push(`attached ${list(a.added).join(", ")}`);
      if (list(a.removed).length) parts.push(`removed ${list(a.removed).join(", ")}`);
      return parts.join(" and ") || "changed its files";
    }
    case "parent":
      return a.to ? `put it under ${a.to}` : "took it out from under its parent";
    case "depends": {
      const parts = [];
      if (list(a.added).length) parts.push(`made it depend on ${list(a.added).join(", ")}`);
      if (list(a.removed).length) parts.push(`made it no longer depend on ${list(a.removed).join(", ")}`);
      return parts.join(" and ");
    }
    case "moved":
      return `moved it from ${a.from} to ${a.to}`;
    case "claimed":
      return a.from ? "took it over" : "started working on it";
    case "released":
      return a.because === "archived" ? `released it when session ${a.session} was archived` : "stopped working on it";
    case "status": {
      const to = a.to as ItemStatus;
      if (to === "on_hold") return "put it on hold";
      if (a.from === "on_hold" && to === "open") return "took it off hold";
      if (to === "done") return "marked it done";
      if (to === "cancelled") return a.with ? `cancelled it with ${a.with}` : "cancelled it";
      if (to === "open") return "opened it again";
      return `set it ${STATUS_LABEL[to]?.toLowerCase() ?? to}`;
    }
    case "review_sent":
      return `sent the ${a.kind === "question" ? "question" : "review"} “${a.title}”`;
    case "review_ended":
      return `“${a.title}” ended: ${String(a.outcome)}`;
    case "queued":
      return "queued it";
    case "unqueued":
      return "took it out of the queue";
    case "review_added":
    case "review_linked":
      return `added the ${a.kind === "question" ? "question" : "review"} “${a.title}”`;
    case "review_removed":
      return `removed the ${a.kind === "question" ? "question" : "review"} “${a.title}”`;
    default:
      return entry.kind.replace(/_/g, " ");
  }
}

/** A run of changes longer than this folds all but its first and its last two. */
const FOLD_OVER = 4;

/** What a kind of change is, in a few words, for a folded run. */
const KIND_WORDS: Record<string, string> = {
  created: "added",
  edited: "edits",
  files: "files",
  parent: "parent",
  depends: "dependencies",
  moved: "repository",
  claimed: "claims",
  released: "claims",
  status: "status",
  review_sent: "reviews",
  review_ended: "reviews",
  review_linked: "reviews",
  review_added: "reviews",
  queued: "queue",
  unqueued: "queue",
  review_removed: "reviews",
};

/** The kinds of change in a run, as a list in words: "status, parent and claims". */
function summary(changes: Change[]): string {
  const words = [...new Set(changes.map((c) => KIND_WORDS[c.kind] ?? c.kind.replace(/_/g, " ")))];
  return words.length > 1 ? `${words.slice(0, -1).join(", ")} and ${words.at(-1)}` : (words[0] ?? "");
}

/** Who did something, as a small picture: you, or the agent. */
function Avatar({ actor }: { actor: string }) {
  return actor === "person" ? (
    <span className="item-me" aria-hidden="true">
      Y
    </span>
  ) : (
    <AgentIcon requestedBy={actor} size={13} />
  );
}

/** The icon of a change on the rail: the new status, who added the item, or
 *  what changed. */
function ChangeIcon({ change }: { change: Change }) {
  const a = (change.attrs ?? {}) as Record<string, unknown>;
  switch (change.kind) {
    case "status":
      return <ItemStatusIcon status={a.to as ItemStatus} size={13} decorative />;
    case "claimed":
      return <ItemStatusIcon status="in_progress" size={13} decorative />;
    case "released":
      return <ItemStatusIcon status="open" size={13} decorative />;
    case "created":
      return <Avatar actor={change.actor} />;
    case "edited":
      return <Pencil size={12} aria-hidden="true" />;
    case "files":
      return <Paperclip size={12} aria-hidden="true" />;
    case "parent":
      return <ListTree size={12} aria-hidden="true" />;
    case "depends":
      return <Link2 size={12} aria-hidden="true" />;
    case "moved":
      return <FolderGit2 size={12} aria-hidden="true" />;
    case "review_sent":
    case "review_ended":
    case "review_linked":
    case "review_added":
    case "review_removed":
      return <GitPullRequestArrow size={12} aria-hidden="true" />;
    default:
      return <FileText size={12} aria-hidden="true" />;
  }
}

function ChangeLine({ change }: { change: Change }) {
  return (
    <li className="activity-line" data-testid="item-event">
      <span className="activity-icon">
        <ChangeIcon change={change} />
      </span>
      <span className="activity-text">
        <b>{who(change.actor)}</b> {said(change)}
        <span className="activity-age">
          {" · "}
          <time dateTime={change.at} title={stamp(change.at)}>
            {age(change.at)} ago
          </time>
        </span>
      </span>
    </li>
  );
}

/** A run of changes on its rail, folded when long until it is opened. */
function Changes({ changes }: { changes: Change[] }) {
  const [open, setOpen] = useState(false);
  const fold = !open && changes.length > FOLD_OVER;
  const hidden = fold ? changes.slice(1, -2) : [];
  const shown = fold ? [changes[0], ...changes.slice(-2)] : changes;
  return (
    <ol className="activity-changes">
      {fold ? (
        <>
          <ChangeLine change={shown[0]} />
          <li className="activity-line">
            <span className="activity-icon">
              <ChevronsUpDown size={12} aria-hidden="true" />
            </span>
            <button type="button" className="activity-more" onClick={() => setOpen(true)}>
              Show {hidden.length} more: {summary(hidden)}
            </button>
          </li>
          {shown.slice(1).map((c) => (
            <ChangeLine key={c.id} change={c} />
          ))}
        </>
      ) : (
        changes.map((c) => <ChangeLine key={c.id} change={c} />)
      )}
    </ol>
  );
}

/** The timeline: changes on a rail and comments in cards, oldest first,
 *  with `field` at the end of the last card of comments, or in a card of
 *  its own after the last changes. */
export function Activity({
  entries,
  renderComment,
  field,
}: {
  entries: ItemEntry[];
  renderComment: (comment: Comment) => ReactNode;
  field: ReactNode;
}) {
  const runs: ({ kind: "changes"; items: Change[] } | { kind: "comments"; items: Comment[] })[] = [];
  for (const entry of entries) {
    const last = runs[runs.length - 1];
    if (entry.type === "comment") {
      if (last?.kind === "comments") last.items.push(entry);
      else runs.push({ kind: "comments", items: [entry] });
    } else if (entry.type === "event") {
      if (last?.kind === "changes") last.items.push(entry);
      else runs.push({ kind: "changes", items: [entry] });
    }
  }
  return (
    <div className="activity">
      {runs.map((run) =>
        run.kind === "changes" ? (
          <Changes key={`c${run.items[0].id}`} changes={run.items} />
        ) : (
          <div key={`m${run.items[0].id}`} className="activity-card">
            {run.items.map((c) => renderComment(c))}
          </div>
        ),
      )}
      {field}
    </div>
  );
}

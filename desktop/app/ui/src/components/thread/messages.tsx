// The lines and messages of a session's thread: its statuses and stages,
// what its agent did to work items, its comments and notes, the cards of
// the reviews it sent, and the person's messages.

import {
  Activity,
  ArrowUp,
  Check,
  CheckCircle2,
  Clock,
  GitCommitHorizontal,
  Paperclip,
  RefreshCw,
  Star,
  Trash2,
  Undo2,
} from "lucide-react";
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Link, type To } from "react-router";
import { ApiError, api } from "../../api/client";
import type {
  ItemStatus,
  Review,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
} from "../../api/types";
import { age, stamp } from "../../lib/format";
import { renderLinks, renderNote, useItemPrefixes } from "../../lib/markdown";
import { openAgentLink } from "../../lib/native";
import { type Entry, type FromSession, type Round } from "../../lib/sessions";
import { useToast } from "../../state/toasts";
import { ItemChip, ItemChips, useOpenItem } from "../ItemChip";
import { Markdown, useFileUrl } from "../ItemMarkdown";
import { ItemStatusIcon, STATUS_LABEL } from "../ItemStatusIcon";
import { AgentIcon } from "../AgentIcon";
import { OutcomeBadge, SummaryCounts } from "../Badges";
import { NoteFiles, filesOf, useNoteFileUrl, useSaveNoteFile } from "../NoteFiles";
import { ReviewIcon } from "../ReviewIcon";
import { Tooltip } from "../Tooltip";

/** How a status reads on its own: a step as it is, the start of a stage, which
 *  the title names, as Started, and the end as Finished with what it says. */
export function statusLine(status: SessionStatus): string {
  if (status.kind === "start") return "Started";
  if (status.kind === "done") return status.text ? `Finished: ${status.text}` : "Finished";
  return status.text;
}

/** Text the agent reported, with its links opened in the browser. */
/** Follows a link in what an agent wrote: a work item's key opens the item
 *  in the app, and any other link opens in the browser. */
export function useFollowLink() {
  const openItem = useOpenItem();
  return useCallback(
    (link: HTMLAnchorElement) => {
      if (link.dataset.item) openItem(link.dataset.item);
      else openAgentLink(link.href);
    },
    [openItem],
  );
}

export function Linked({ text }: { text: string }) {
  const followLink = useFollowLink();
  const html = useMemo(() => renderLinks(text), [text]);
  return (
    <span
      onClick={(e) => {
        const link = (e.target as HTMLElement).closest("a");
        if (!link) return;
        e.preventDefault();
        followLink(link);
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/** Statuses the agent reported in a row: the newest, and the earlier ones
 *  behind a button. */
export function StatusGroup({ statuses }: { statuses: SessionStatus[] }) {
  const [open, setOpen] = useState(false);
  const newest = statuses[statuses.length - 1];
  const earlier = statuses.length - 1;
  const shown = open ? statuses : [newest];
  return (
    <div className="thread-statuses" data-testid="status-group">
      {earlier > 0 ? (
        <button type="button" className="thread-status-more" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          {open ? "Show only the newest" : `${earlier} earlier`}
        </button>
      ) : null}
      {shown.map((status) => (
        <div key={status.id} className="thread-status-line" data-testid="status">
          <Activity size={13} className="thread-status-icon" aria-hidden />
          <span className="thread-status-text">
            <Linked text={status.text} />
          </span>
          <span className="faint" title={stamp(status.at)}>
            {age(status.at)} ago
          </span>
        </div>
      ))}
    </div>
  );
}

/** A starred review's mark at the end of its row in the details. */
export function StarMark({ review }: { review: Review }) {
  if (!review.starred) return null;
  return <Star size={12} className="thread-item-star" fill="currentColor" aria-label="Starred" role="img" />;
}

/** The agent saying it starts a stage, which it names. */
export function StartedLine({ status }: { status: SessionStatus }) {
  return (
    <div className="thread-divider thread-started" id={`status-${status.id}`} data-testid="started">
      <span>
        <span className="thread-line-label">
          <GitCommitHorizontal size={12} aria-hidden /> Started:
        </span>{" "}
        <span className="thread-line-text">
          <Linked text={status.text} />
        </span>{" "}
        ·{" "}
        <span className="thread-age" title={stamp(status.at)}>
          {age(status.at)} ago
        </span>
      </span>
    </div>
  );
}

/** How tall a note is before it folds, about 12 lines. */
const NOTE_FOLDED = 230;

/** What the session's agent did to work items, as a line among its
 *  statuses: the verb, and the chip of each item it did it to. */
export function ItemEventLine({ events }: { events: SessionItemEvent[] }) {
  const event = events[events.length - 1];
  const a = (event.attrs ?? {}) as Record<string, unknown>;
  const to = a.to as ItemStatus | undefined;
  const did =
    event.kind === "created"
      ? "Added"
      : event.kind === "claimed"
        ? a.from
          ? "Took over"
          : "Started working on"
        : event.kind === "released"
          ? "Stopped working on"
          : to === "done"
            ? "Marked done"
            : to === "cancelled"
              ? "Cancelled"
              : to === "open"
                ? "Opened again"
                : `Set ${STATUS_LABEL[to ?? "open"].toLowerCase()}`;
  const icon: ItemStatus = event.kind === "claimed" ? "in_progress" : event.kind === "status" && to ? to : "open";
  return (
    <div className="thread-statuses" data-testid="item-event">
      <div className="thread-status-line is-item-event">
        <ItemStatusIcon status={icon} size={13} decorative />
        <span className="thread-status-text thread-item-event">
          <span className="thread-item-event-did">{did}</span>
          {events.map((e) => (
            <ItemChip key={e.id} item={e.item} />
          ))}
        </span>
        <span className="faint thread-item-event-age" title={stamp(event.at)}>
          {age(event.at)} ago
        </span>
      </div>
    </div>
  );
}

/** A comment the session's agent wrote on a work item: the item's chip,
 *  and the comment's Markdown as the item's panel shows it. */
export function CommentMessage({ comment }: { comment: SessionComment }) {
  const followLink = useFollowLink();
  const fileUrl = useFileUrl(comment.item.key);
  return (
    <div className="thread-note thread-comment msg" data-testid="thread-comment">
      <span className="msg-avatar">
        <AgentIcon requestedBy={comment.author} size={14} />
      </span>
      <div className="msg-col">
        <div className="msg-meta">
          <span className="text">{comment.author}</span> commented on <ItemChip item={comment.item} /> ·{" "}
          <span className="thread-age" title={stamp(comment.created_at)}>
            {age(comment.created_at)} ago
          </span>
        </div>
        <div
          onClick={(e) => {
            const link = (e.target as HTMLElement).closest("a");
            if (!link || link.dataset.attachment !== undefined) return;
            e.preventDefault();
            followLink(link);
          }}
        >
          <Markdown text={comment.body} files={comment.files} fileUrl={fileUrl} />
        </div>
        {comment.files.length ? (
          <ul className="item-files">
            {comment.files.map((f) => (
              <li key={f.name}>
                <a href={fileUrl(f.name) ?? "#"} target="_blank" rel="noreferrer">
                  <Paperclip size={12} /> {f.name}
                </a>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </div>
  );
}

/** A note the agent sent: a message under the agent's icon, as a question
 *  is, with its Markdown folded when long and its files under it. Its links
 *  open in the browser, and a link to one of its files saves it. */
export function NoteMessage({
  status,
  agent,
  session,
}: {
  status: SessionStatus;
  agent: string | null;
  session: string | null;
}) {
  const followLink = useFollowLink();
  const files = filesOf(status);
  const note = useMemo(
    () => (agent && session ? { agent, session, status: status.id } : null),
    [agent, session, status.id],
  );
  const url = useNoteFileUrl(note);
  const save = useSaveNoteFile(note, url);
  // only an image the note carries shows in place
  const imageUrl = useCallback(
    (name: string) => (files.some((f) => f.name === name && f.media_type.startsWith("image/")) ? url(name) : null),
    [files, url],
  );
  const prefixes = useItemPrefixes();
  // eslint-disable-next-line react-hooks/exhaustive-deps -- rendered again when the key prefixes change
  const html = useMemo(() => renderNote(status.text, imageUrl), [status.text, imageUrl, prefixes]);
  const body = useRef<HTMLDivElement>(null);
  const [long, setLong] = useState(false);
  const [open, setOpen] = useState(false);
  // measured again as its images load
  const measure = useCallback(() => setLong((body.current?.scrollHeight ?? 0) > NOTE_FOLDED + 40), []);
  useLayoutEffect(measure, [html, measure]);
  return (
    <div className="thread-note msg" data-testid="note">
      <span className="msg-avatar">
        <AgentIcon requestedBy={agent} size={14} />
      </span>
      <div className="msg-col">
        <div className="msg-meta">
          <span className="text">{agent ?? "The agent"}</span> wrote ·{" "}
          <span className="thread-age" title={stamp(status.at)}>
            {age(status.at)} ago
          </span>
          {files.length ? (
            <span className="note-file-count">
              <Paperclip size={12} aria-hidden /> {files.length} file{files.length === 1 ? "" : "s"}
            </span>
          ) : null}
        </div>
        <div
          ref={body}
          className={`thread-note-body ${long && !open ? "is-folded" : ""}`}
          style={long && !open ? { maxHeight: NOTE_FOLDED } : undefined}
          onLoadCapture={measure}
          onClick={(e) => {
            const link = (e.target as HTMLElement).closest("a");
            if (!link) return;
            e.preventDefault();
            const file = link.dataset.attachment;
            if (file !== undefined) void save(file);
            else followLink(link);
          }}
          dangerouslySetInnerHTML={{ __html: html }}
        />
        {long ? (
          <button type="button" className="thread-note-more" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
            {open ? "Show less" : "Show more"}
          </button>
        ) : null}
        <NoteFiles files={files} url={url} onSave={(name) => void save(name)} />
      </div>
    </div>
  );
}

/** The agent saying it has finished, with what it finished with. */
export function FinishedLine({ status }: { status: SessionStatus }) {
  return (
    <div className="thread-divider thread-finished" data-testid="finished">
      <span>
        <span className="thread-line-label">
          <CheckCircle2 size={13} aria-hidden /> Finished{status.text ? ":" : ""}
        </span>{" "}
        {status.text ? (
          <span className="thread-line-text">
            <Linked text={status.text} />
          </span>
        ) : null}{" "}
        ·{" "}
        <span className="thread-age" title={stamp(status.at)}>
          {age(status.at)} ago
        </span>
      </span>
    </div>
  );
}

type CardProps = {
  review: Review;
  round?: Round;
  to: To;
  state?: FromSession;
  /** shown in the panel beside the thread */
  open: boolean;
  onClick: (event: React.MouseEvent) => void;
};

/** A review the session sent, by its newest round, which opens it. */
export function ReviewCard({ review, round, to, state, open, onClick }: CardProps) {
  return (
    <Link
      to={to}
      state={state}
      onClick={onClick}
      id={`review-${review.id}`}
      className={`thread-card ${open ? "is-open" : ""}`}
      aria-current={open ? "true" : undefined}
      data-testid="thread-review"
    >
      <ReviewIcon review={review} size={16} />
      <span className="thread-card-text">
        <span className="thread-card-title">{review.title}</span>
        <span className="thread-card-meta faint" title={stamp(review.created_at)}>
          Review · {review.plugin}
          {round ? ` · round ${round.n} of ${round.of}` : ""} · {age(review.created_at)} ago
        </span>
        {review.items?.length ? (
          <span className="thread-card-items">
            <ItemChips items={review.items} max={3} />
          </span>
        ) : null}
        {/* under the title, where many counts cannot squeeze it */}
        <SummaryCounts summary={review.status === "pending" ? review.summary : review.decision?.summary} />
      </span>
      <OutcomeBadge review={review} />
    </Link>
  );
}

/** How long a card stays highlighted after a line leads to it. */
const FLASH_MS = 1600;

/** Scrolls to a review's card and highlights it. */
function showCard(id: string, onOpen?: () => void) {
  const el = document.getElementById(`review-${id}`);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  // an attribute, because opening the review sets the card's class
  delete el.dataset.flashed;
  void el.offsetWidth; // restart the highlight when clicked again
  el.dataset.flashed = "true";
  window.setTimeout(() => delete el.dataset.flashed, FLASH_MS);
  onOpen?.();
}

/** Where the agent sent a later round of a review shown further up: a quiet
 *  line that scrolls to the review's card, and opens the review in the
 *  panel beside the thread when reviews open there. */
export function RevisedLine({ entry, onOpen }: { entry: Extract<Entry, { kind: "revised" }>; onOpen?: () => void }) {
  const { review, round, card } = entry;
  const waiting = card.status === "pending" && review === card;
  return (
    <button type="button" className="thread-revised" data-testid="revised" onClick={() => showCard(card.id, onOpen)}>
      <RefreshCw size={13} className="thread-revised-icon" aria-hidden />
      <span className="thread-revised-text">
        Revised <span className="thread-revised-title">{review.title}</span>
        {round ? ` · round ${round}` : ""} ·{" "}
        <span className="thread-age" title={stamp(review.created_at)}>
          {age(review.created_at)} ago
        </span>
      </span>
      {waiting ? <span className="thread-revised-waiting">Waiting for you</span> : null}
      <ArrowUp size={12} className="thread-revised-up" aria-hidden />
    </button>
  );
}

/** Where the agent withdrew a review shown further up, and why. */
export function WithdrawnLine({
  entry,
  onOpen,
}: {
  entry: Extract<Entry, { kind: "withdrawn" }>;
  onOpen?: () => void;
}) {
  const { review, card } = entry;
  const at = review.withdrawn_at!;
  const said = `${review.withdrawn_by ?? "The agent"} withdrew`;
  return (
    <button
      type="button"
      className="thread-revised thread-withdrawn"
      data-testid="withdrawn"
      onClick={() => showCard(card.id, onOpen)}
    >
      <Undo2 size={13} className="thread-revised-icon" aria-hidden />
      <span className="thread-revised-text">
        {said} <span className="thread-revised-title">{review.title}</span> ·{" "}
        <span className="thread-age" title={stamp(at)}>
          {age(at)} ago
        </span>
        {review.withdrawn_reason ? `: ${review.withdrawn_reason}` : ""}
      </span>
      <ArrowUp size={12} className="thread-revised-up" aria-hidden />
    </button>
  );
}

/** A message the person sent the agent, on the right, with whether the
 *  agent has received it; one still queued can be deleted. */
export function MessageBubble({
  message,
  session,
  onDeleted,
}: {
  message: SessionMessage;
  session: { agent: string; id: string };
  onDeleted: () => void;
}) {
  const toast = useToast();
  const queued = !message.delivered_at;
  const remove = async () => {
    try {
      await api.deleteMessage(session.agent, session.id, message.id);
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "The message was not deleted.");
    }
    onDeleted();
  };
  return (
    <div
      className={`thread-message ${queued ? "is-queued" : "is-delivered"}`}
      id={`message-${message.id}`}
      data-testid="message"
    >
      <div className="thread-message-row">
        {queued ? (
          <Tooltip label="Delete the message" side="top">
            <button type="button" className="thread-message-delete" aria-label="Delete the message" onClick={remove}>
              <Trash2 size={14} />
            </button>
          </Tooltip>
        ) : null}
        <div className="thread-message-bubble" title={`Sent ${stamp(message.sent_at)}`}>
          {message.text}
        </div>
      </div>
      {queued ? (
        <span className="thread-message-state" data-testid="message-state">
          <Clock size={12} aria-hidden /> Queued
        </span>
      ) : (
        <span
          className="thread-message-state"
          data-testid="message-state"
          title={`Delivered ${stamp(message.delivered_at)}`}
        >
          <Check size={12} aria-hidden /> Delivered {age(message.delivered_at)} ago
        </span>
      )}
    </div>
  );
}

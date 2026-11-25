// A session's turns and the runs Pinrail started for it, in its thread.

import {
  CircleCheck,
  CircleSlash,
  CircleX,
  KeyRound,
  MessageCircleQuestion,
  Play,
  Power,
  ShieldAlert,
  Square,
  TriangleAlert,
  Unplug,
  Zap,
  type LucideIcon,
} from "lucide-react";
import { memo, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Run } from "../../api/runtime";
import type { Session, SessionTurn } from "../../api/types";
import { stamp } from "../../lib/format";
import { AgentIcon, agentOf } from "../AgentIcon";
import { MarkdownHtml } from "../MarkdownHtml";
import { WaitMark } from "../WaitMark";
import { ThreadLine } from "./messages";
import { attachedParts } from "../../lib/attachedPaths";
import { AttachedBadges } from "./AttachedText";

/** How long something lasted, in the largest unit that fits. */
export function lasted(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
}

/** An excerpt, folded to four lines until it is opened. */
function Excerpt({ text: whole, cut }: { text: string; cut: boolean }) {
  const [open, setOpen] = useState(false);
  // the attached files and images show as badges under the text
  const { text, files, images } = attachedParts(whole);
  const long = cut || text.split("\n").length > 4 || text.length > 320;
  return (
    <div className={`thread-turn-text ${open ? "is-open" : ""}`}>
      <p>{cut && open ? `${text}…` : text}</p>
      <AttachedBadges files={files} images={images} />
      {long ? (
        <button type="button" className="thread-turn-more" onClick={() => setOpen((o) => !o)}>
          {open ? "Show less" : "Show more"}
        </button>
      ) : null}
    </div>
  );
}

/** A reply, folded to about four lines until it is opened. It is
 *  measured again as its width changes, so a reply that fits once the
 *  thread is wider shows whole. */
function Folded({ cut, children }: { cut: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = box.current;
    // open, it is not cut at all: keep what it measured folded
    if (!el || open) return;
    const measure = () => setOverflows(el.scrollHeight > el.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [open]);
  const long = cut || overflows;
  return (
    <>
      <div ref={box} className={`thread-turn-folded ${open ? "is-open" : long ? "is-folded" : ""}`}>
        {children}
      </div>
      {long ? (
        <button type="button" className="thread-turn-more" onClick={() => setOpen((o) => !o)}>
          {open ? "Show less" : cut ? "Show more (the reply was cut)" : "Show more"}
        </button>
      ) : null}
    </>
  );
}

/** What a working agent is doing, as a word that changes every few
 *  seconds. */
const DOING = [
  "Thinking",
  "Reading",
  "Tinkering",
  "Weighing it up",
  "Sketching",
  "Untangling",
  "Piecing it together",
  "Mulling",
  "Wiring it up",
  "Tracing",
  "Sifting",
  "Polishing",
];
const DOING_EVERY_MS = 3000;

/** The agent at work, at the end of the thread: Pinrail's mark and what it
 *  is doing. */
export function WorkingLine({ name }: { name: string }) {
  const [at, setAt] = useState(() => Math.floor(Math.random() * DOING.length));
  useEffect(() => {
    // the word stays for a person who asks for less motion
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const timer = window.setInterval(() => setAt((n) => (n + 1) % DOING.length), DOING_EVERY_MS);
    return () => window.clearInterval(timer);
  }, []);
  return (
    <div className="thread-turn-working" role="status" aria-label={`${name} is working`} data-testid="turn-working">
      <span className="thread-line-icon">
        <WaitMark size={18} />
      </span>
      <span key={at} className="thread-turn-doing">
        {DOING[at]}…
      </span>
    </div>
  );
}

type Wait = NonNullable<NonNullable<Session["presence"]>["wait"]>;

/** What the agent waits for in its terminal, above the message box. With
 *  `onOpen`, Pinrail runs the agent, and a click anywhere on the line opens
 *  its terminal; without, it runs in a terminal of its own, where the
 *  person answers. */
export function WaitingLine({ name, wait, onOpen }: { name: string; wait: Wait; onOpen?: () => void }) {
  const [Icon, label] =
    wait.kind === "permission"
      ? [ShieldAlert, `${name} needs permission`]
      : wait.kind === "auth"
        ? [KeyRound, `${name} needs you to sign in`]
        : [MessageCircleQuestion, `${name} asks`];
  const detail = wait.kind === "permission" ? [wait.tool, wait.summary].filter(Boolean).join(" · ") : wait.summary;
  return (
    // the button is the line's for the keyboard; its click reaches the line
    <div
      className={`thread-waiting is-${wait.kind} ${onOpen ? "is-openable" : ""}`}
      data-testid="waiting-line"
      onClick={onOpen}
    >
      <span className="thread-line-icon">
        <Icon size={15} aria-hidden />
      </span>
      <span className="thread-waiting-label">{label}</span>
      {detail ? (
        <span className="thread-waiting-detail truncate" title={detail}>
          {detail}
        </span>
      ) : null}
      {onOpen ? (
        <button type="button" className="chrome-button">
          Open the terminal
        </button>
      ) : (
        <span className="thread-waiting-where">Answer it in the terminal where {name} runs</span>
      )}
    </div>
  );
}

/** How a turn that ended without a reply ended, in a sentence, or null
 *  while it goes on. A failed turn's reply is its error, which the
 *  sentence ends with. */
export function turnEndText(turn: SessionTurn, name: string): string | null {
  if (!turn.ended_at) return null;
  const took = lasted(Date.parse(turn.ended_at) - Date.parse(turn.started_at));
  switch (turn.end_kind) {
    // the person pressed Esc, or declined a permission, which agents
    // report the same way
    case "interrupted":
      return `${name} stopped after ${took}`;
    case "failed":
      return turn.reply ? `${name}'s turn failed after ${took}: ${turn.reply}` : `${name}'s turn failed after ${took}`;
    case "ended":
      return `The session ended during ${name}'s turn, after ${took}`;
    default:
      return `${name} finished the turn in ${took}`;
  }
}

const TURN_END_ICONS = { interrupted: CircleSlash, failed: CircleX, ended: Power } as const;

/** A turn: what the person asked and what the agent replied. */
export const TurnEntry = memo(function TurnEntry({
  turn,
  part,
  agent,
}: {
  turn: SessionTurn;
  /** the prompt, where the turn started, or its reply or how it ended */
  part: "prompt" | "end";
  agent: string | null;
}) {
  const name = agentOf(agent)?.label ?? agent ?? "The agent";
  const took = turn.ended_at ? lasted(Date.parse(turn.ended_at) - Date.parse(turn.started_at)) : null;
  const interrupted = turn.end_kind === "interrupted";
  // a failed turn's reply is the agent's error, not a message of its own
  const failed = turn.end_kind === "failed";
  // how the turn ended, and how long it took
  const ends = [interrupted ? "Interrupted" : turn.ended_at ? null : "Working", took].filter(Boolean).join(" · ");
  return (
    <div className="thread-turn" data-testid="turn">
      {part === "prompt" && turn.prompt !== null ? (
        // what the person asked, on the right, as their messages are
        <div className="thread-turn-prompt" aria-label="You asked" title={stamp(turn.started_at)}>
          <div className="thread-message-row">
            <div className="thread-message-bubble">
              <Excerpt text={turn.prompt} cut={turn.prompt_cut} />
            </div>
          </div>
        </div>
      ) : null}
      {part === "end" && turn.reply !== null && !failed ? (
        // what the agent replied, under its icon, as its other messages are
        <div className="thread-note thread-turn-reply msg" data-testid="turn-reply">
          <span className="msg-avatar">
            <AgentIcon requestedBy={agent} size={14} />
          </span>
          <div className="msg-col">
            <div className="msg-meta">
              <span className="text">{name}</span> replied
              {ends ? ` · ${ends}` : null}
            </div>
            <Folded cut={turn.reply_cut}>
              <MarkdownHtml text={turn.reply} mode="note" className="item-md thread-note-body" />
            </Folded>
          </div>
        </div>
      ) : null}
      {part === "end" && (turn.reply === null || failed) && turn.ended_at ? (
        // a turn with no reply kept, or one that failed, says how it ended
        // where the reply would be, as a line across the thread, since the
        // agent wrote nothing
        <TurnEndLine turn={turn} name={name} />
      ) : null}
    </div>
  );
});

/** A reply the agent gave before its turn went on, as when Claude Code
 *  replies again once a background job ends: shown where it came, as its
 *  own message. */
export const TurnReplyEntry = memo(function TurnReplyEntry({
  turn,
  reply,
  agent,
}: {
  turn: SessionTurn;
  /** which of the turn's replies */
  reply: number;
  agent: string | null;
}) {
  const shown = turn.replies?.[reply];
  if (!shown) return null;
  const name = agentOf(agent)?.label ?? agent ?? "The agent";
  return (
    <div className="thread-turn" data-testid="turn">
      <div className="thread-note thread-turn-reply msg" data-testid="turn-reply" title={stamp(shown.at)}>
        <span className="msg-avatar">
          <AgentIcon requestedBy={agent} size={14} />
        </span>
        <div className="msg-col">
          <div className="msg-meta">
            <span className="text">{name}</span> replied
          </div>
          <Folded cut={shown.cut}>
            <MarkdownHtml text={shown.text} mode="note" className="item-md thread-note-body" />
          </Folded>
        </div>
      </div>
    </div>
  );
});

/** Where a run Pinrail started began or ended. */
export function runLineText(run: Run, ended: boolean): string {
  if (!ended) return run.kind === "resume" ? "Resumed in Pinrail" : "Started in Pinrail";
  switch (run.end_reason) {
    case "stopped":
      return "Stopped by you";
    case "host_lost":
      return "Stopped when Pinrail lost its terminal host";
    case "restart":
      return "Stopped when the computer restarted";
    case "start_failed":
      return "Could not be started";
    default:
      if (run.exit_code !== null) return `Exited with code ${run.exit_code}`;
      if (run.exit_signal !== null) return `Ended by signal ${run.exit_signal}`;
      return "Ended";
  }
}

/** The icon of a run's line, and its colour for a run that went wrong. */
export function runLineIcon(run: Run, ended: boolean): [LucideIcon, "warn" | "danger" | undefined] {
  if (!ended) return [Play, undefined];
  switch (run.end_reason) {
    case "stopped":
      return [Square, undefined];
    case "host_lost":
      return [Unplug, "warn"];
    case "restart":
      return [Power, undefined];
    case "start_failed":
      return [TriangleAlert, "danger"];
    default:
      if (run.exit_code !== null && run.exit_code !== 0) return [TriangleAlert, "warn"];
      if (run.exit_code === null && run.exit_signal !== null) return [Zap, "warn"];
      return [Square, undefined];
  }
}

/** How a turn that ended without a reply ended, as a line in the thread:
 *  an icon, a sentence, and when it ended. */
function TurnEndLine({ turn, name }: { turn: SessionTurn; name: string }) {
  const Icon = TURN_END_ICONS[turn.end_kind as keyof typeof TURN_END_ICONS] ?? CircleCheck;
  const tone = turn.end_kind === "interrupted" ? "warn" : turn.end_kind === "failed" ? "danger" : undefined;
  return (
    <ThreadLine
      icon={<Icon size={13} aria-hidden />}
      tone={tone}
      at={turn.ended_at}
      className={`thread-turn-end is-${turn.end_kind ?? "done"}`}
      data-testid="turn-state"
    >
      <span title={`Turn ${turn.turn}, from ${stamp(turn.started_at)} to ${stamp(turn.ended_at)}`}>
        {turnEndText(turn, name)}
      </span>
    </ThreadLine>
  );
}

/** Where a run Pinrail started began or ended. With `session`, the line
 *  also says when the session started, which it began. */
export const RunLine = memo(function RunLine({
  run,
  ended,
  session,
}: {
  run: Run;
  ended: boolean;
  /** when the session started, for its first run */
  session?: string;
}) {
  const at = ended ? run.ended_at! : run.started_at;
  const [Icon, tone] = runLineIcon(run, ended);
  return (
    <ThreadLine
      icon={<Icon size={13} aria-hidden />}
      tone={tone}
      start={!ended}
      at={session ?? at}
      when={session ? stamp(session) : undefined}
      data-testid="run-line"
    >
      {session ? "Session started in Pinrail" : runLineText(run, ended)}
    </ThreadLine>
  );
});

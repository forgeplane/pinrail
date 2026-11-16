// A session's turns and the runs Pinrail started for it, in its thread.

import { memo, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Run } from "../../api/runtime";
import type { SessionTurn } from "../../api/types";
import { stamp } from "../../lib/format";
import { AgentIcon, agentOf } from "../AgentIcon";
import { MarkdownHtml } from "../MarkdownHtml";
import { WaitMark } from "../WaitMark";

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
function Excerpt({ text, cut }: { text: string; cut: boolean }) {
  const [open, setOpen] = useState(false);
  const long = cut || text.split("\n").length > 4 || text.length > 320;
  return (
    <div className={`thread-turn-text ${open ? "is-open" : ""}`}>
      <p>{cut && open ? `${text}…` : text}</p>
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

/** A turn the agent works on: Pinrail's mark and what it is doing. */
function WorkingLine({ name }: { name: string }) {
  const [at, setAt] = useState(() => Math.floor(Math.random() * DOING.length));
  useEffect(() => {
    // the word stays for a person who asks for less motion
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const timer = window.setInterval(() => setAt((n) => (n + 1) % DOING.length), DOING_EVERY_MS);
    return () => window.clearInterval(timer);
  }, []);
  return (
    <div className="thread-turn-working" role="status" aria-label={`${name} is working`} data-testid="turn-working">
      <WaitMark size={18} />
      <span key={at} className="thread-turn-doing">
        {DOING[at]}…
      </span>
    </div>
  );
}

/** A turn: what the person asked and what the agent replied. */
export const TurnEntry = memo(function TurnEntry({
  turn,
  agent,
  waiting = false,
}: {
  turn: SessionTurn;
  agent: string | null;
  /** the agent waits for the person, which the thread says itself */
  waiting?: boolean;
}) {
  const name = agentOf(agent)?.label ?? agent ?? "The agent";
  const took = turn.ended_at ? lasted(Date.parse(turn.ended_at) - Date.parse(turn.started_at)) : null;
  const interrupted = turn.end_kind === "interrupted";
  // how the turn ended, and how long it took
  const ends = [
    turn.prompt === null && turn.reply === null ? `Turn ${turn.turn}` : null,
    interrupted ? "Interrupted" : turn.ended_at ? null : "Working",
    took,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <div className="thread-turn" data-testid="turn">
      {turn.prompt !== null ? (
        // what the person asked, on the right, as their messages are
        <div className="thread-turn-prompt" aria-label="You asked" title={stamp(turn.started_at)}>
          <div className="thread-message-row">
            <div className="thread-message-bubble">
              <Excerpt text={turn.prompt} cut={turn.prompt_cut} />
            </div>
          </div>
        </div>
      ) : null}
      {turn.reply !== null ? (
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
      {turn.reply === null && !turn.ended_at ? (
        waiting ? null : (
          <WorkingLine name={name} />
        )
      ) : turn.reply === null && ends ? (
        // a turn with no reply kept says how it ended where the reply would be
        <div className="thread-note thread-turn-reply msg" data-testid="turn-state">
          <span className="msg-avatar">
            <AgentIcon requestedBy={agent} size={14} />
          </span>
          <div className="msg-col">
            <div className="msg-meta">
              <span className="text">{name}</span> · {ends}
            </div>
          </div>
        </div>
      ) : null}
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

export const RunLine = memo(function RunLine({ run, ended }: { run: Run; ended: boolean }) {
  const at = ended ? run.ended_at! : run.started_at;
  return (
    <div className="thread-divider thread-run" data-testid="run-line">
      <span title={stamp(at)}>{runLineText(run, ended)}</span>
    </div>
  );
});

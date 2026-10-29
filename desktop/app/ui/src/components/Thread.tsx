// An agent session's thread: the questions it asked, as the agent's
// messages, and the reviews it sent, oldest first. Questions are answered
// here, each with its chips or a reply box, and sent as a set.

import { Ban, Send, Star, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { ApiError, api } from "../api/client";
import type { Answer, Question, Review, Session } from "../api/types";
import { age, stamp } from "../lib/format";
import { MOD, hasMod, modalOpen } from "../lib/keys";
import { renderMarkdown } from "../lib/markdown";
import { openExternal } from "../lib/native";
import { answersOf, decisionOf, pick, unanswered, type Draft } from "../lib/questions";
import { useLive } from "../state/live";
import { useToast } from "../state/toasts";
import { AgentIcon } from "./AgentIcon";
import { OutcomeBadge, SummaryCounts } from "./Badges";
import { DiscardDialog } from "./DiscardDialog";
import { ReviewIcon } from "./ReviewIcon";
import { Tooltip } from "./Tooltip";

const DRAFTS = "pinrail:answers:";

type ThreadProps = {
  /** the questions opened, which the thread shows within their session */
  focus?: Review;
  /** the session to show, when it is not the focus's */
  session?: { agent: string; id: string };
  inPanel?: boolean;
  /** called when the focus's questions are answered or declined here */
  onEnded?: () => void;
  onClose?: () => void;
};

/** The session of a review, when the agent that sent it said which. */
export function sessionOf(review: Review): { agent: string; id: string } | null {
  return review.requested_by && review.session ? { agent: review.requested_by, id: review.session } : null;
}

export function Thread({ focus, session: given, inPanel = false, onEnded, onClose }: ThreadProps) {
  const live = useLive();
  const named = given ?? (focus ? sessionOf(focus) : null);
  const [session, setSession] = useState<Session | null>(null);
  const [items, setItems] = useState<Review[]>(focus ? [focus] : []);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      if (named) {
        const thread = await api.session(named.agent, named.id);
        setSession(thread.session);
        setItems(thread.items);
      } else if (focus) {
        setItems([await api.getReview(focus.id)]);
      }
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "The server did not answer.");
    }
  }, [named?.agent, named?.id, focus?.id]); // eslint-disable-line react-hooks/exhaustive-deps -- the names, not the objects
  useEffect(() => {
    load();
  }, [load]);

  // anything about one of its reviews, or a new one from the session
  const ids = useRef(new Set<string>());
  ids.current = new Set(items.map((r) => r.id));
  useEffect(() => {
    const notice = live.lastNotice;
    if (!notice) return;
    const ours =
      (notice.review_id && ids.current.has(notice.review_id)) ||
      (named && notice.review?.requested_by === named.agent && notice.review?.session === named.id);
    if (ours) load();
  }, [live.lastNotice]); // eslint-disable-line react-hooks/exhaustive-deps -- each notice once

  // the questions the keys answer: the ones opened, else the first waiting
  const active = useMemo(() => {
    if (focus && items.find((r) => r.id === focus.id)?.status === "pending") return focus.id;
    return items.find((r) => r.kind === "question" && r.status === "pending")?.id ?? null;
  }, [items, focus]);

  // the questions opened are scrolled to once the thread is there
  const scrolled = useRef(false);
  useEffect(() => {
    if (scrolled.current || !focus || items.length < 2) return;
    scrolled.current = true;
    document.getElementById(`set-${focus.id}`)?.scrollIntoView({ block: "start" });
  }, [items, focus]);

  const agent = named?.agent ?? focus?.requested_by ?? null;
  const waiting = items.filter((r) => r.status === "pending");

  return (
    <div className={`thread ${inPanel ? "in-panel" : ""}`} data-testid="thread">
      <div className="thread-head">
        <span className="thread-who">
          <AgentIcon requestedBy={agent} size={15} />
          <span className="text">{agent ?? "Questions"}</span>
          {session?.repo ? (
            <span className="dim">
              {session.repo}
              {session.ref ? <span className="mono faint"> #{session.ref}</span> : null}
            </span>
          ) : null}
        </span>
        {named ? (
          <span className="mono faint thread-session" title={named.id}>
            session {named.id.length > 12 ? `${named.id.slice(0, 8)}…` : named.id}
          </span>
        ) : null}
        {session ? (
          <span className="dim" title={stamp(session.last_at)}>
            active {age(session.last_at)} ago
          </span>
        ) : null}
        <span className="strip-spacer" />
        {waiting.length > 0 ? <span className="thread-waiting">{waiting.length} waiting for you</span> : null}
        {inPanel && onClose ? (
          <Tooltip label="Close" keys={["Esc"]} side="bottom">
            <button type="button" className="bar-button" onClick={onClose} aria-label="Close the panel">
              <X size={15} />
            </button>
          </Tooltip>
        ) : null}
      </div>
      {error ? <p className="notice notice-danger">{error}</p> : null}
      <div className="thread-items">
        {items.map((item) =>
          item.kind === "question" ? (
            <QuestionSet
              key={item.id}
              review={item}
              active={item.id === active}
              onDone={() => {
                if (item.id === focus?.id && onEnded) onEnded();
                else load();
              }}
            />
          ) : (
            <ReviewCard key={item.id} review={item} />
          ),
        )}
      </div>
    </div>
  );
}

/** A review the session sent, which opens the review. */
function ReviewCard({ review }: { review: Review }) {
  return (
    <Link to={`/reviews/${review.id}`} className="thread-card" data-testid="thread-review">
      <ReviewIcon review={review} size={14} />
      <span className="thread-card-title">{review.title}</span>
      <SummaryCounts summary={review.status === "pending" ? review.summary : review.decision?.summary} />
      <OutcomeBadge review={review} />
      <span className="faint" title={stamp(review.created_at)}>
        {age(review.created_at)} ago
      </span>
    </Link>
  );
}

function loadDrafts(id: string): Record<string, Draft> {
  try {
    return JSON.parse(sessionStorage.getItem(DRAFTS + id) ?? "{}");
  } catch {
    return {};
  }
}

/** The questions of one `pinrail ask`: answered together while they wait,
 *  and shown with their answers once they are sent. */
function QuestionSet({ review, active, onDone }: { review: Review; active: boolean; onDone: () => void }) {
  const live = useLive();
  const toast = useToast();
  const questions = useMemo(
    () => (review.payload as { questions?: Question[] } | undefined)?.questions ?? [],
    [review.payload],
  );
  const pending = review.status === "pending";
  const [drafts, setDrafts] = useState<Record<string, Draft>>(() => loadDrafts(review.id));
  const [current, setCurrent] = useState(0);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [declining, setDeclining] = useState(false);
  const answers = useMemo(() => answersOf(review.decision?.data), [review.decision]);

  const change = useCallback(
    (id: string, next: Draft) => {
      setDrafts((all) => {
        const drafted = { ...all, [id]: next };
        try {
          sessionStorage.setItem(DRAFTS + review.id, JSON.stringify(drafted));
        } catch {
          // no storage
        }
        return drafted;
      });
    },
    [review.id],
  );
  const left = unanswered(questions, drafts);

  const send = useCallback(async () => {
    if (busy || !pending) return;
    setBusy(true);
    setProblem(null);
    try {
      await api.decide(review.id, decisionOf(questions, drafts), "");
      try {
        sessionStorage.removeItem(DRAFTS + review.id);
      } catch {
        // ignore
      }
      toast(questions.length === 1 ? "Answer sent" : "Answers sent");
      onDone();
    } catch (e) {
      setProblem(e instanceof Error ? e.message : "The answers were not sent.");
    } finally {
      setBusy(false);
    }
  }, [busy, pending, review.id, questions, drafts, toast, onDone]);

  // a pick moves on to the next question still unanswered
  const picked = useCallback(
    (index: number, value: string) => {
      const question = questions[index];
      const next = pick(question, drafts[question.id], value);
      change(question.id, next);
      if (question.answer === "choices") return;
      const after = questions.findIndex((q, i) => i > index && unanswered([q], { [q.id]: drafts[q.id] ?? {} }) === 1);
      if (after >= 0) setCurrent(after);
    },
    [questions, drafts, change],
  );

  useEffect(() => {
    if (!active || !pending) return;
    const onKey = (event: KeyboardEvent) => {
      if (modalOpen()) return;
      if (hasMod(event) && !event.shiftKey && !event.altKey && event.key === "Enter") {
        event.preventDefault();
        send();
        return;
      }
      if (hasMod(event) && !event.shiftKey && !event.altKey && (event.key === "d" || event.key === "D")) {
        event.preventDefault();
        setDeclining(true);
        return;
      }
      const el = event.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const question = questions[current];
      if (!question) return;
      if (question.answer === "yes-no" && (event.key === "y" || event.key === "n")) {
        event.preventDefault();
        picked(current, event.key === "y" ? "yes" : "no");
        return;
      }
      const n = Number(event.key);
      const choice = question.choices?.[n - 1];
      if (Number.isInteger(n) && n >= 1 && choice !== undefined) {
        event.preventDefault();
        picked(current, choice);
      }
    };
    // the palette's "Discard this review"
    const onDiscard = () => setDeclining(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:discard", onDiscard);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:discard", onDiscard);
    };
  }, [active, pending, current, questions, picked, send]);

  return (
    <section
      id={`set-${review.id}`}
      className={`question-set ${pending ? "is-pending" : ""} ${active ? "is-active" : ""}`}
      aria-label={review.title}
      data-testid="question-set"
    >
      {questions.map((question, index) => (
        <QuestionMessage
          key={question.id}
          review={review}
          question={question}
          draft={drafts[question.id]}
          answer={answers.get(question.id)}
          current={pending && index === current}
          onFocus={() => setCurrent(index)}
          onPick={(value) => picked(index, value)}
          onChange={(next) => change(question.id, next)}
        />
      ))}
      <SetFooter review={review} />
      {problem ? <p className="notice notice-danger">{problem}</p> : null}
      {pending ? (
        <div className="set-bar">
          <span className="faint set-age" title={stamp(review.created_at)}>
            asked {age(review.created_at)} ago
          </span>
          <Tooltip label="Decline: the agent is told to stop" keys={[MOD, "D"]} side="top">
            <button type="button" className="chrome-button" onClick={() => setDeclining(true)}>
              <Ban size={13} /> Decline
            </button>
          </Tooltip>
          <Tooltip
            label={live.connected ? "Send the answers to the agent" : "Reconnect to send"}
            keys={[MOD, "Enter"]}
            side="top"
          >
            <button
              type="button"
              className="handover-button with-icon"
              data-testid="send-answers"
              disabled={busy || !live.connected}
              onClick={send}
            >
              {left > 0 && questions.length > 1 ? `Send, ${left} unanswered` : "Send"}
              <Send size={13} />
            </button>
          </Tooltip>
        </div>
      ) : null}
      {declining && pending ? (
        <DiscardDialog
          review={review}
          onClose={() => setDeclining(false)}
          onDone={() => {
            setDeclining(false);
            toast("Declined. The agent was told to stop.");
            onDone();
          }}
        />
      ) : null}
    </section>
  );
}

/** How questions that were not answered ended. */
function SetFooter({ review }: { review: Review }) {
  switch (review.status) {
    case "discarded":
      return (
        <p className="set-ended dim">
          {review.discarded_by ?? "The person"} declined to answer {age(review.discarded_at)} ago
          {review.discarded_reason ? `: ${review.discarded_reason}` : "."}
        </p>
      );
    case "withdrawn":
      return (
        <p className="set-ended dim">
          {review.withdrawn_by ?? "The agent"} withdrew the questions {age(review.withdrawn_at)} ago
          {review.withdrawn_reason ? `: ${review.withdrawn_reason}` : "."}
        </p>
      );
    case "expired":
      return <p className="set-ended dim">The questions expired at {stamp(review.expires_at)} without an answer.</p>;
    default:
      return null;
  }
}

type MessageProps = {
  review: Review;
  question: Question;
  draft: Draft | undefined;
  answer: Answer | undefined;
  /** the question the keys answer */
  current: boolean;
  onFocus: () => void;
  onPick: (value: string) => void;
  onChange: (next: Draft) => void;
};

/** One question as the agent's message, and below it, the answer: being
 *  picked or written while it waits, the person's own once sent. */
function QuestionMessage({ review, question, draft, answer, current, onFocus, onPick, onChange }: MessageProps) {
  const pending = review.status === "pending";
  const [noting, setNoting] = useState(!!draft?.note);
  const options =
    question.answer === "yes-no" ? ["yes", "no"] : question.answer === "text" ? [] : (question.choices ?? []);
  const isPicked = (value: string) =>
    !draft?.own &&
    (question.answer === "yes-no"
      ? draft?.yes === (value === "yes")
      : question.answer === "choices"
        ? !!draft?.choices?.includes(value)
        : draft?.choice === value);
  const writing = question.answer === "text" || !!draft?.own;

  return (
    <div className={`msg-pair ${current ? "is-current" : ""}`} onFocusCapture={onFocus} onMouseDown={onFocus}>
      <div className="msg msg-agent" data-testid="question">
        <span className="msg-avatar">
          <AgentIcon requestedBy={review.requested_by} size={14} />
        </span>
        <div className="msg-body">
          <p className="msg-question">{question.question}</p>
          {question.context ? <Context text={question.context} /> : null}
          {pending && options.length > 0 ? (
            <div
              className="chips"
              role={question.answer === "choices" ? "group" : "radiogroup"}
              aria-label={question.question}
            >
              {options.map((value, i) => (
                <button
                  key={value}
                  type="button"
                  className={`chip ${isPicked(value) ? "is-picked" : ""}`}
                  role={question.answer === "choices" ? "checkbox" : "radio"}
                  aria-checked={isPicked(value)}
                  onClick={() => onPick(value)}
                >
                  {current && question.answer !== "yes-no" && i < 9 ? <kbd className="chip-key">{i + 1}</kbd> : null}
                  {question.answer === "yes-no" ? (value === "yes" ? "Yes" : "No") : value}
                  {question.recommended === value ? (
                    <Star size={11} className="chip-star" aria-label="recommended" />
                  ) : null}
                </button>
              ))}
            </div>
          ) : null}
          {pending && writing ? (
            <textarea
              className="reply"
              rows={2}
              value={draft?.text ?? ""}
              placeholder={question.answer === "text" ? "Type your answer…" : "Answer in your own words…"}
              aria-label={`Answer to ${question.question}`}
              onChange={(e) => onChange({ ...draft, text: e.target.value })}
            />
          ) : null}
          {pending && noting ? (
            <textarea
              className="reply reply-note"
              rows={1}
              value={draft?.note ?? ""}
              placeholder="A note for the agent…"
              aria-label={`Note on ${question.question}`}
              autoFocus={!draft?.note}
              onChange={(e) => onChange({ ...draft, note: e.target.value })}
            />
          ) : null}
          {pending ? (
            <div className="msg-actions">
              {question.answer !== "text" ? (
                <button type="button" className="link-button" onClick={() => onChange({ ...draft, own: !draft?.own })}>
                  {draft?.own ? "Pick a choice instead" : "Answer in your own words"}
                </button>
              ) : null}
              {!noting ? (
                <button type="button" className="link-button" onClick={() => setNoting(true)}>
                  Add a note
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
      {review.status === "decided" ? <AnswerMessage question={question} answer={answer} review={review} /> : null}
    </div>
  );
}

/** The person's answer, as their own message. */
function AnswerMessage({ question, answer, review }: { question: Question; answer?: Answer; review: Review }) {
  const text =
    !answer || answer.unanswered
      ? null
      : answer.choice !== undefined
        ? answer.choice
        : answer.choices
          ? answer.choices.join(", ")
          : answer.yes !== undefined
            ? answer.yes
              ? "Yes"
              : "No"
            : (answer.text ?? null);
  const own = !!answer?.text && question.answer !== "text";
  return (
    <div className="msg msg-person" data-testid="answer">
      <div className="msg-body">
        {text === null ? <p className="faint">Not answered</p> : <p className={own ? "msg-own" : ""}>{text}</p>}
        {answer?.note ? <p className="msg-note">{answer.note}</p> : null}
        <span className="faint msg-when" title={stamp(review.decision?.decided_at)}>
          {review.decision?.decided_by ?? "You"} · {age(review.decision?.decided_at)} ago
        </span>
      </div>
    </div>
  );
}

/** A question's context, as Markdown, with its links opened in the browser. */
function Context({ text }: { text: string }) {
  const html = useMemo(() => renderMarkdown(text), [text]);
  return (
    <div
      className="msg-context"
      onClick={(e) => {
        const link = (e.target as HTMLElement).closest("a");
        if (!link) return;
        e.preventDefault();
        openExternal(link.href);
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

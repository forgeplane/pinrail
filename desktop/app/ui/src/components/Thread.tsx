// An agent session's thread: the questions it asked, as the agent's
// messages, and the reviews it sent, oldest first. Questions are answered
// here, each with its chips or a reply box, and sent as a set.

import { Ban, Check, GitBranch, Mic, Send, Star, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router";
import { ApiError, api } from "../api/client";
import type { Answer, Question, Review, Session } from "../api/types";
import { age, stamp } from "../lib/format";
import { MOD, hasMod, modalOpen } from "../lib/keys";
import { canDictate, useDictation } from "../lib/dictation";
import { renderMarkdown } from "../lib/markdown";
import { openExternal } from "../lib/native";
import { answersOf, decisionOf, pick, unanswered, type Draft } from "../lib/questions";
import { useLive } from "../state/live";
import { useToast } from "../state/toasts";
import { AgentIcon } from "./AgentIcon";
import { OutcomeBadge, SummaryCounts } from "./Badges";
import { DiscardDialog } from "./DiscardDialog";
import { ReviewIcon, reviewLabel } from "./ReviewIcon";
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

/** A session's page. */
export const sessionPath = (s: { agent: string; id: string }) =>
  `/sessions/${encodeURIComponent(s.agent)}/${encodeURIComponent(s.id)}`;

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

  // the questions the keys and the chat box answer: those picked, else the
  // ones opened, else the first waiting
  const [picked, setPicked] = useState<string | null>(null);
  const active = useMemo(() => {
    const waitingQuestions = (id: string | null | undefined) =>
      !!id && items.some((r) => r.id === id && r.kind === "question" && r.status === "pending");
    if (waitingQuestions(picked)) return picked;
    if (waitingQuestions(focus?.id)) return focus!.id;
    return items.find((r) => r.kind === "question" && r.status === "pending")?.id ?? null;
  }, [items, focus, picked]);
  // where the active questions put their chat box, under the timeline
  const [footer, setFooter] = useState<HTMLDivElement | null>(null);

  // once the thread is there, the questions opened are scrolled to, or,
  // for the session alone, its newest message
  const timeline = useRef<HTMLDivElement>(null);
  const scrolled = useRef(false);
  useEffect(() => {
    if (scrolled.current) return;
    if (focus) {
      if (items.length < 2) return;
      scrolled.current = true;
      document.getElementById(`set-${focus.id}`)?.scrollIntoView({ block: "start" });
    } else if (session && timeline.current) {
      scrolled.current = true;
      timeline.current.scrollTop = timeline.current.scrollHeight;
    }
  }, [items, focus, session]);

  const agent = named?.agent ?? focus?.requested_by ?? null;
  const waiting = items.filter((r) => r.status === "pending");
  const questions = items.filter((r) => r.kind === "question").length;
  const reviews = items.length - questions;
  const repo = session?.repo ?? focus?.origin.repo ?? null;
  const ref = session?.ref ?? focus?.origin.ref ?? null;

  return (
    <div className={`thread ${inPanel ? "in-panel" : ""}`} data-testid="thread">
      <header className="thread-head">
        <span className="thread-avatar">
          <AgentIcon requestedBy={agent} size={18} />
        </span>
        <div className="thread-title">
          <h1>
            {agent ?? "Questions"}
            {repo ? (
              <>
                {" "}
                <span className="faint thread-in">in</span> {repo}
                {ref ? (
                  <span className="thread-ref" title={`Branch ${ref}`}>
                    <GitBranch size={13} aria-label="Branch" />
                    {ref}
                  </span>
                ) : null}
              </>
            ) : null}
          </h1>
          <div className="thread-sub">
            {waiting.length > 0 ? (
              <span className="thread-waiting">
                <span className="thread-waiting-dot" aria-hidden />
                Waiting for you · {waiting.length}
              </span>
            ) : null}
            {named ? (
              // from a question, the session's own page; on it, the id alone
              focus ? (
                <Link to={sessionPath(named)} className="faint thread-session" title={`Open session ${named.id}`}>
                  session {named.id}
                </Link>
              ) : (
                <span className="faint thread-session" title={named.id}>
                  session {named.id}
                </span>
              )
            ) : null}
            {session ? (
              <span className="faint" title={stamp(session.first_at)}>
                · started {age(session.first_at)} ago
              </span>
            ) : null}
          </div>
        </div>
        <span className="thread-head-end">
          {items.length > 0 ? (
            <span className="thread-counts">
              {[
                reviews ? `${reviews} ${reviews === 1 ? "review" : "reviews"}` : null,
                questions ? `${questions} ${questions === 1 ? "question" : "questions"}` : null,
              ]
                .filter(Boolean)
                .join(" · ")}
            </span>
          ) : null}
          {inPanel && onClose ? (
            <Tooltip label="Close" keys={["Esc"]} side="bottom">
              <button type="button" className="bar-button" onClick={onClose} aria-label="Close the panel">
                <X size={15} />
              </button>
            </Tooltip>
          ) : null}
        </span>
      </header>
      {error ? <p className="notice notice-danger">{error}</p> : null}
      <div className="thread-body">
        <div className="thread-main">
          <div className="thread-items" ref={timeline}>
            {session ? (
              <div className="thread-divider">
                <span>Session started {stamp(session.first_at)}</span>
              </div>
            ) : null}
            {items.map((item) =>
              item.kind === "question" ? (
                <QuestionSet
                  key={item.id}
                  review={item}
                  active={item.id === active}
                  footer={item.id === active ? footer : null}
                  onActivate={() => setPicked(item.id)}
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
          <div className="thread-footer" ref={setFooter}>
            {active ? null : <IdleComposer />}
          </div>
        </div>
        {inPanel ? null : (
          <aside className="thread-aside" aria-label="Session details">
            <section>
              <h2>Waiting on you</h2>
              {waiting.length === 0 ? <p className="faint">Nothing is waiting.</p> : null}
              {waiting.map((r) => (
                <a
                  key={r.id}
                  href={r.kind === "question" ? `#set-${r.id}` : `#/reviews/${r.id}`}
                  className="thread-aside-card"
                  onClick={(e) => {
                    if (r.kind !== "question") return;
                    e.preventDefault();
                    document.getElementById(`set-${r.id}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
                  }}
                >
                  <span className="text">{r.title}</span>
                  <span className="faint">
                    {r.kind === "question" ? "Questions" : reviewLabel(r)} · asked {age(r.created_at)} ago
                  </span>
                </a>
              ))}
            </section>
            {session ? (
              <section>
                <h2>Session</h2>
                <dl className="thread-facts">
                  <dt>Agent</dt>
                  <dd>{session.agent}</dd>
                  <dt>Id</dt>
                  <dd className="mono">{session.id}</dd>
                  <dt>Last activity</dt>
                  <dd title={stamp(session.last_at)}>{age(session.last_at)} ago</dd>
                </dl>
              </section>
            ) : null}
          </aside>
        )}
      </div>
    </div>
  );
}

/** A review the session sent, which opens the review. */
function ReviewCard({ review }: { review: Review }) {
  return (
    <Link to={`/reviews/${review.id}`} className="thread-card" data-testid="thread-review">
      <ReviewIcon review={review} size={16} />
      <span className="thread-card-text">
        <span className="thread-card-title">{review.title}</span>
        <span className="faint" title={stamp(review.created_at)}>
          Review · {review.plugin} · {age(review.created_at)} ago
        </span>
      </span>
      <SummaryCounts summary={review.status === "pending" ? review.summary : review.decision?.summary} />
      <OutcomeBadge review={review} />
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

/** Whether a draft has a choice picked, so what is written is a note on it. */
const hasPick = (question: Question, draft: Draft | undefined) =>
  !draft?.own &&
  (question.answer === "choice"
    ? draft?.choice !== undefined
    : question.answer === "choices"
      ? !!draft?.choices?.length
      : question.answer === "yes-no"
        ? draft?.yes !== undefined
        : false);

type SetProps = {
  review: Review;
  /** the questions the keys and the chat box answer */
  active: boolean;
  /** where the chat box goes while they are active */
  footer: HTMLElement | null;
  onActivate: () => void;
  onDone: () => void;
};

/** The questions of one `pinrail ask`: the agent's message, with each
 *  question below it, answered together while they wait. */
function QuestionSet({ review, active, footer, onActivate, onDone }: SetProps) {
  const live = useLive();
  const toast = useToast();
  const questions = useMemo(
    () => (review.payload as { questions?: Question[] } | undefined)?.questions ?? [],
    [review.payload],
  );
  const pending = review.status === "pending";
  const [drafts, setDrafts] = useState<Record<string, Draft>>(() => loadDrafts(review.id));
  const [comment, setComment] = useState(() => {
    try {
      return sessionStorage.getItem(DRAFTS + review.id + ":comment") ?? "";
    } catch {
      return "";
    }
  });
  const onComment = useCallback(
    (text: string) => {
      setComment(text);
      try {
        sessionStorage.setItem(DRAFTS + review.id + ":comment", text);
      } catch {
        // no storage
      }
    },
    [review.id],
  );
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
      await api.decide(review.id, decisionOf(questions, drafts), comment.trim());
      try {
        sessionStorage.removeItem(DRAFTS + review.id);
        sessionStorage.removeItem(DRAFTS + review.id + ":comment");
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
  }, [busy, pending, review.id, questions, drafts, comment, toast, onDone]);

  // a pick from the keyboard moves on to the next question still
  // unanswered; one with the pointer keeps the question selected
  const picked = useCallback(
    (index: number, value: string, advance: boolean) => {
      const question = questions[index];
      const next = pick(question, drafts[question.id], value);
      change(question.id, next);
      // an unpick stays on the question
      const answered = unanswered([question], { [question.id]: next }) === 0;
      if (!advance || !answered || question.answer === "choices") return;
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
        picked(current, event.key === "y" ? "yes" : "no", true);
        return;
      }
      const n = Number(event.key);
      const choice = question.choices?.[n - 1];
      if (Number.isInteger(n) && n >= 1 && choice !== undefined) {
        event.preventDefault();
        picked(current, choice, true);
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

  const several = questions.length > 1;
  return (
    <section
      id={`set-${review.id}`}
      className={`question-set msg ${pending ? "is-pending" : ""} ${active ? "is-active" : ""}`}
      aria-label={review.title}
      data-testid="question-set"
      onMouseDown={pending ? onActivate : undefined}
    >
      <span className="msg-avatar">
        <AgentIcon requestedBy={review.requested_by} size={14} />
      </span>
      <div className="msg-col">
        <div className="msg-meta">
          <span className="text">{review.requested_by ?? "The agent"}</span> {pending ? "asks" : "asked"} ·{" "}
          <span title={stamp(review.created_at)}>{age(review.created_at)} ago</span>
          {pending ? <span className="msg-waiting">waiting for your answer</span> : null}
        </div>
        {questions.map((question, index) => (
          <QuestionMessage
            key={question.id}
            review={review}
            number={several ? `${index + 1}/${questions.length}` : null}
            question={question}
            draft={drafts[question.id]}
            answer={answers.get(question.id)}
            current={active && pending && index === current}
            onFocus={() => {
              onActivate();
              setCurrent(index);
            }}
            onPick={(value) => picked(index, value, false)}
            onChange={(next) => change(question.id, next)}
            onNext={() => {
              // Enter in a question's box goes to the next one's, and from
              // the last to the comment on them all
              const set = document.getElementById(`set-${review.id}`);
              if (index + 1 < questions.length) {
                setCurrent(index + 1);
                set?.querySelector<HTMLTextAreaElement>(`[data-reply="${index + 1}"]`)?.focus();
              } else {
                document.querySelector<HTMLTextAreaElement>(".thread-composer textarea")?.focus();
              }
            }}
            index={index}
          />
        ))}
        {review.status === "decided" && review.agent_note ? (
          <div className="msg-answer set-comment" data-testid="set-comment">
            <span>{review.decision?.decided_by ?? "You"} commented</span>
            <span className="msg-answer-text">{review.agent_note}</span>
          </div>
        ) : null}
        <SetFooter review={review} />
        {problem ? <p className="notice notice-danger">{problem}</p> : null}
      </div>
      {active && pending && footer
        ? createPortal(
            <Composer
              comment={comment}
              onComment={onComment}
              left={several ? left : 0}
              busy={busy}
              connected={live.connected}
              onSend={send}
              onDecline={() => setDeclining(true)}
            />,
            footer,
          )
        : null}
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
  /** "2/3" among several questions */
  number: string | null;
  question: Question;
  draft: Draft | undefined;
  answer: Answer | undefined;
  /** the question the keys and the chat box answer */
  current: boolean;
  onFocus: () => void;
  onPick: (value: string) => void;
  onChange: (next: Draft) => void;
  /** Enter in its box: on to the next question */
  onNext: () => void;
  index: number;
};

/** One question in its bubble, with its choices while it waits, what the
 *  person has written for it so far, and once sent, the answer. */
function QuestionMessage({
  review,
  number,
  question,
  draft,
  answer,
  current,
  onFocus,
  onPick,
  onChange,
  onNext,
  index,
}: MessageProps) {
  const pending = review.status === "pending";
  const options =
    question.answer === "yes-no" ? ["yes", "no"] : question.answer === "text" ? [] : (question.choices ?? []);
  const isPicked = (value: string) =>
    !draft?.own &&
    (question.answer === "yes-no"
      ? draft?.yes === (value === "yes")
      : question.answer === "choices"
        ? !!draft?.choices?.includes(value)
        : draft?.choice === value);
  return (
    <div
      className={`msg-question-block ${pending ? "is-pending" : ""} ${current ? "is-current" : ""}`}
      onMouseDown={pending ? onFocus : undefined}
      data-testid="question"
    >
      <div className="msg-body">
        <p className="msg-question">
          {number ? <span className="msg-number">{number}</span> : null}
          {question.question}
        </p>
        {question.context ? <Context text={question.context} /> : null}
        {pending && question.answer === "choices" ? <span className="msg-hint">Select all that apply</span> : null}
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
                {current && i < 9 ? (
                  <kbd className="chip-key">{question.answer === "yes-no" ? (value === "yes" ? "Y" : "N") : i + 1}</kbd>
                ) : null}
                {question.answer === "yes-no" ? (value === "yes" ? "Yes" : "No") : value}
                {question.recommended === value ? (
                  <Star size={11} className="chip-star" aria-label="recommended" />
                ) : null}
              </button>
            ))}
            {draft?.own ? (
              <button type="button" className="chip-clear" onClick={() => onChange({ ...draft, own: false })}>
                Pick a choice instead
              </button>
            ) : null}
          </div>
        ) : null}
        {pending ? (
          <QuestionReply question={question} draft={draft} onChange={onChange} onNext={onNext} index={index} />
        ) : null}
      </div>
      {review.status === "decided" ? <AnswerLine question={question} answer={answer} review={review} /> : null}
    </div>
  );
}

/** A text box that grows with its text and takes dictation: Esc stops
 *  the dictation first, then leaves the box, so the keys pick choices again. */
function useWritingBox(value: string, write: (text: string) => void) {
  const toast = useToast();
  const box = useRef<HTMLTextAreaElement>(null);
  const dictation = useDictation(write, (message) => toast(message, "danger"));
  const listening = dictation.listening;
  const stopDictation = dictation.stop;
  useEffect(() => {
    if (!listening) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing || modalOpen()) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      stopDictation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening, stopDictation]);
  // it grows with the text, and scrolls only past its largest height
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
    el.style.overflowY = el.scrollHeight > 160 ? "auto" : "hidden";
  }, [value]);
  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Escape" && !listening) {
      e.preventDefault();
      e.stopPropagation();
      e.currentTarget.blur();
    }
  };
  const toggle = () => {
    if (listening) dictation.stop();
    else dictation.start(value);
    box.current?.focus();
  };
  const mic = canDictate ? (
    <Tooltip label={listening ? "Stop dictating" : "Dictate"} side="top">
      <button
        type="button"
        className={`bar-button composer-dictate ${listening ? "is-listening" : ""}`}
        aria-label={listening ? "Stop dictating" : "Dictate"}
        aria-pressed={listening}
        onClick={toggle}
      >
        <Mic size={14} />
      </button>
    </Tooltip>
  ) : null;
  return { box, onKeyDown, mic };
}

/** The box in a question's bubble for what the person writes for it: the
 *  answer to a text question; for one with choices, an answer in their own
 *  words while nothing is picked, and a note on the pick once one is. */
function QuestionReply({
  question,
  draft,
  onChange,
  onNext,
  index,
}: {
  question: Question;
  draft: Draft | undefined;
  onChange: (next: Draft) => void;
  onNext: () => void;
  index: number;
}) {
  const asNote = hasPick(question, draft);
  const value = (asNote ? draft?.note : draft?.text) ?? "";
  const write = (text: string) => {
    if (asNote) onChange({ ...draft, note: text });
    else onChange({ ...draft, text, own: question.answer !== "text" ? text.trim() !== "" : undefined });
  };
  const { box, onKeyDown, mic } = useWritingBox(value, write);
  const label = asNote
    ? `Note on ${question.question}`
    : question.answer === "text"
      ? `Answer to ${question.question}`
      : `Answer to ${question.question} in your own words`;
  return (
    <div className={`msg-reply ${value ? "has-text" : ""}`}>
      <textarea
        ref={box}
        rows={1}
        value={value}
        aria-label={label}
        placeholder={
          asNote ? "Add a note…" : question.answer === "text" ? "Type your answer…" : "Or answer in your own words…"
        }
        onChange={(e) => write(e.target.value)}
        onKeyDown={(e) => {
          // Enter goes on to the next question, Shift+Enter starts a new
          // line, and ⌘↵ sends, which the set hears
          if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            onNext();
            return;
          }
          onKeyDown(e);
        }}
        data-reply={index}
      />
      {mic}
    </div>
  );
}

type ComposerProps = {
  /** the person's comment to the agent on the whole set */
  comment: string;
  onComment: (text: string) => void;
  /** how many questions would go back unanswered, among several */
  left: number;
  busy: boolean;
  connected: boolean;
  onSend: () => void;
  onDecline: () => void;
};

/** The box under the thread: a comment to the agent on the questions as a
 *  whole, sent with the answers, and the buttons that send or decline them. */
function Composer({ comment, onComment, left, busy, connected, onSend, onDecline }: ComposerProps) {
  const { box, onKeyDown, mic } = useWritingBox(comment, onComment);
  return (
    <div className="thread-composer">
      <div className={`composer ${busy ? "is-busy" : ""}`}>
        <textarea
          ref={box}
          rows={1}
          value={comment}
          aria-label="Comment to the agent"
          placeholder="Add a comment to the agent on these questions…"
          onChange={(e) => onComment(e.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="composer-bar">
          <span className="composer-hint">{connected ? "The comment travels with your answers" : "Reconnecting…"}</span>
          {mic}
          <Tooltip label="Decline: the agent is told to stop" keys={[MOD, "D"]} side="top">
            <button type="button" className="chrome-button" onClick={onDecline}>
              <Ban size={13} /> Decline
            </button>
          </Tooltip>
          <Tooltip
            label={connected ? "Send the answers to the agent" : "Reconnect to send"}
            keys={[MOD, "Enter"]}
            side="top"
          >
            <button
              type="button"
              className="handover-button with-icon"
              data-testid="send-answers"
              disabled={busy || !connected}
              onClick={onSend}
            >
              {left > 0 ? `Send, ${left} unanswered` : "Send"}
              <Send size={13} />
              <span className="handover-keys" aria-hidden="true">
                <kbd>{MOD}</kbd>
                <kbd>↵</kbd>
              </span>
            </button>
          </Tooltip>
        </div>
      </div>
    </div>
  );
}

/** The chat box with nothing to answer. */
function IdleComposer() {
  return (
    <div className="thread-composer is-idle">
      <div className="composer">
        <textarea
          rows={1}
          disabled
          aria-label="Comment to the agent"
          placeholder="No question is waiting for an answer"
        />
      </div>
    </div>
  );
}

/** The person's answer, in a line under the question. */
function AnswerLine({ question, answer, review }: { question: Question; answer?: Answer; review: Review }) {
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
  const by = review.decision?.decided_by ?? "You";
  return (
    <div className="msg-answer" data-testid="answer">
      {text === null ? (
        <span className="faint">Not answered</span>
      ) : (
        <>
          <Check size={13} className="msg-answer-check" aria-hidden />
          <span>
            {by} answered{own ? " in their own words" : ""}
          </span>
          <span className={`msg-answer-text ${own ? "msg-own" : ""}`}>{text}</span>
        </>
      )}
      <span className="faint" title={stamp(review.decision?.decided_at)}>
        · {age(review.decision?.decided_at)} ago
      </span>
      {answer?.note ? <p className="msg-note">{answer.note}</p> : null}
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

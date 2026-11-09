// A session's details beside its thread: what waits on the person and on
// the agent, the work items it worked on, the starred reviews, its stages,
// the reviews that ended, and the session's facts.

import { Check, ChevronRight, Clock } from "lucide-react";
import { useMemo, useState, type MouseEvent } from "react";
import { Link, type To } from "react-router";
import type { Review, Session, SessionMessage, WorkItem } from "../../api/types";
import { age, stamp } from "../../lib/format";
import { stageReviews, type Entry, type FromSession, type Stage } from "../../lib/sessions";
import { outcomeOf } from "../Badges";
import { itemPath } from "../ItemChip";
import { ItemStatusIcon, STATUS_LABEL } from "../ItemStatusIcon";
import { ReviewIcon, reviewLabel } from "../ReviewIcon";
import { Tooltip } from "../Tooltip";
import { StarMark } from "./messages";
import { store, stored } from "./storage";

/** Whether the session's details list the reviews that ended, stored on
 *  this machine; they are folded away until opened. */
const HANDLED_SHOWN = "pinrail.thread.handled";

/** At most this many of a stage's reviews in its tooltip. */
const STAGE_REVIEWS = 5;
/** How many of the latest stages the details show before the rest fold. */
const STAGES_SHOWN = 8;

type Props = {
  session: Session | null;
  /** its reviews and questions that wait on the person, by newest round */
  waiting: Review[];
  /** each of its reviews once, by its newest round */
  newest: Review[];
  messages: SessionMessage[];
  /** the work items it holds or worked on */
  sessionItems: WorkItem[];
  started: Stage[];
  /** the stage the thread was opened at, which never folds away */
  stage?: number;
  entries: Entry[];
  /** a waiting review or question was clicked */
  onWaiting: (review: Review) => void;
  /** where a review's link goes, and what a click on it does */
  reviewLink: (review: Review) => { to: To; state?: FromSession; onClick: (event: MouseEvent) => void };
  onItemLink: (event: MouseEvent, key: string) => void;
};

export function ThreadDetails({
  session,
  waiting,
  newest,
  messages,
  sessionItems,
  started,
  stage,
  entries,
  onWaiting,
  reviewLink,
  onItemLink,
}: Props) {
  // every item it worked on, those in progress first and the closed ones last
  const STANDING = { in_progress: 0, open: 1, on_hold: 2, done: 3, cancelled: 4 } as const;
  const workedOn = [...sessionItems].sort(
    (a, b) => STANDING[a.status] - STANDING[b.status] || a.repo.localeCompare(b.repo) || a.number - b.number,
  );

  // the latest stages, and the earlier ones once unfolded; a stage the
  // thread was opened at is never folded away
  const [allStages, setAllStages] = useState(false);
  const earlier = Math.max(0, started.length - STAGES_SHOWN);
  const atEarlier = !!stage && started.slice(0, earlier).some((s) => s.start.id === stage);
  const stagesShown = allStages || atEarlier ? started : started.slice(earlier);

  // the reviews each stage sent, for its tooltip
  const sentBy = useMemo(() => stageReviews(entries), [entries]);

  // the person's messages the agent has not received yet
  const queued = messages.filter((m) => !m.delivered_at);
  // the reviews the session sent that have ended, newest first
  const handled = newest.filter((r) => r.kind === "review" && r.status !== "pending").reverse();
  // the starred reviews, newest first
  const starred = newest.filter((r) => r.kind === "review" && r.starred).reverse();
  const [showHandled, setShowHandled] = useState(() => stored(HANDLED_SHOWN) === "shown");
  const toggleHandled = () =>
    setShowHandled((shown) => {
      store(HANDLED_SHOWN, shown ? "hidden" : "shown");
      return !shown;
    });

  return (
    <aside className="thread-aside" aria-label="Session details">
      <section>
        <h2>Waiting on you{waiting.length > 0 ? ` · ${waiting.length}` : ""}</h2>
        {waiting.length === 0 ? <p className="faint">Nothing is waiting.</p> : null}
        {waiting.map((r) => (
          <a
            key={r.id}
            href={r.kind === "question" ? `#set-${r.id}` : `#/reviews/${r.id}`}
            className="thread-waiting-item"
            data-testid="waiting-item"
            onClick={(e) => {
              e.preventDefault();
              onWaiting(r);
            }}
          >
            <ReviewIcon review={r} size={14} />
            <span className="thread-waiting-item-text">
              <span className="text">{r.title}</span>
              <span className="faint" title={stamp(r.created_at)}>
                {r.kind === "question" ? "questions" : reviewLabel(r)} · {age(r.created_at)} ago
              </span>
            </span>
            <StarMark review={r} />
          </a>
        ))}
      </section>
      {queued.length > 0 ? (
        <section aria-label="Waiting on the agent">
          <h2>Waiting on the agent · {queued.length}</h2>
          {queued.map((m) => (
            <a
              key={m.id}
              href={`#message-${m.id}`}
              className="thread-waiting-item"
              data-testid="queued-message"
              onClick={(e) => {
                e.preventDefault();
                document.getElementById(`message-${m.id}`)?.scrollIntoView({ block: "center", behavior: "smooth" });
              }}
            >
              <Clock size={14} className="thread-queued-icon" aria-hidden />
              <span className="thread-waiting-item-text">
                <span className="text">{m.text}</span>
                <span className="faint" title={stamp(m.sent_at)}>
                  queued · {age(m.sent_at)} ago
                </span>
              </span>
            </a>
          ))}
        </section>
      ) : null}
      {workedOn.length > 0 ? (
        <section aria-label="Work items" data-testid="thread-work-items">
          <h2>Work items · {workedOn.length}</h2>
          {workedOn.map((item) => (
            <Link
              key={item.key}
              to={itemPath(item.key)}
              onClick={(e) => onItemLink(e, item.key)}
              className={`thread-waiting-item ${item.status === "done" || item.status === "cancelled" ? "is-ended" : ""}`}
              data-testid="thread-work-item"
            >
              <ItemStatusIcon status={item.status} />
              <span className="thread-waiting-item-text">
                <span className="text">{item.title}</span>
                <span className="faint">
                  <span className="mono">{item.key}</span> · {STATUS_LABEL[item.status].toLowerCase()}
                </span>
              </span>
            </Link>
          ))}
        </section>
      ) : null}
      {starred.length > 0 ? (
        <section aria-label="Starred">
          <h2>Starred · {starred.length}</h2>
          {starred.map((r) => {
            const outcome = outcomeOf(r);
            return (
              <Link
                key={r.id}
                {...reviewLink(r)}
                className={`thread-waiting-item ${r.status === "pending" ? "" : `is-ended tone-${outcome.tone}`}`}
                data-testid="starred-review"
              >
                <ReviewIcon review={r} size={14} />
                <span className="thread-waiting-item-text">
                  <span className="text">{r.title}</span>
                  <span className="faint" title={stamp(r.created_at)}>
                    {r.status === "pending" ? "waiting" : <span className="thread-outcome">{outcome.label}</span>} ·{" "}
                    {r.plugin} · {age(r.created_at)} ago
                  </span>
                </span>
                <StarMark review={r} />
              </Link>
            );
          })}
        </section>
      ) : null}
      {started.length > 0 ? (
        <section aria-label="Stages">
          <h2>Stages</h2>
          {earlier > 0 && !atEarlier ? (
            <button
              type="button"
              className="thread-status-more thread-rail-more"
              aria-expanded={allStages}
              onClick={() => setAllStages((all) => !all)}
            >
              {allStages ? "Show only the latest" : `${earlier} earlier ${earlier === 1 ? "stage" : "stages"}`}
            </button>
          ) : null}
          <ol className="thread-rail">
            {stagesShown.map((stage) => {
              const i = started.indexOf(stage);
              const last = i === started.length - 1;
              const state = stage.done ? "is-done" : last ? "is-last" : "is-left";
              // until it finished, until the next stage began for one left
              // open, or until now for the one in progress
              const end = stage.done?.at ?? (last ? null : started[i + 1].start.at);
              const took = age(stage.start.at, end ? Date.parse(end) : Date.now());
              const length = stage.current
                ? `In progress for ${took}`
                : stage.done
                  ? `Finished in ${took}`
                  : `Took ${took}`;
              const about = [stage.start.text, length, `Started ${stamp(stage.start.at)}`].join(" · ");
              return (
                <li key={stage.start.id} className={`thread-rail-stop ${state}`}>
                  <Tooltip
                    side="top"
                    label={
                      <span className="stage-tip">
                        <strong>{stage.start.text}</strong>
                        <span>{length}</span>
                        <span className="faint">Started {stamp(stage.start.at)}</span>
                        {(sentBy.get(stage.start.id) ?? []).length > 0 ? (
                          <span className="stage-tip-reviews">
                            {sentBy
                              .get(stage.start.id)!
                              .slice(0, STAGE_REVIEWS)
                              .map((r) => (
                                <span key={r.id} className="stage-tip-review">
                                  <ReviewIcon review={r} size={12} />
                                  <span className="stage-tip-title">{r.title}</span>
                                  <span className="faint">
                                    {r.status === "pending" ? "waiting" : outcomeOf(r).label}
                                  </span>
                                </span>
                              ))}
                            {sentBy.get(stage.start.id)!.length > STAGE_REVIEWS ? (
                              <span className="faint">{sentBy.get(stage.start.id)!.length - STAGE_REVIEWS} more</span>
                            ) : null}
                          </span>
                        ) : null}
                      </span>
                    }
                  >
                    <a
                      href={`#status-${stage.start.id}`}
                      className="thread-rail-link"
                      data-testid="stage"
                      aria-label={about}
                      onClick={(e) => {
                        e.preventDefault();
                        document
                          .getElementById(`status-${stage.start.id}`)
                          ?.scrollIntoView({ block: "center", behavior: "smooth" });
                      }}
                    >
                      <span className="thread-rail-dot" aria-hidden>
                        {stage.done ? <Check size={9} strokeWidth={3} /> : null}
                      </span>
                      <span className="thread-rail-text">{stage.start.text}</span>
                      <span className="faint thread-rail-when">{took}</span>
                    </a>
                  </Tooltip>
                </li>
              );
            })}
          </ol>
        </section>
      ) : null}
      {handled.length > 0 ? (
        <section aria-label="Reviews">
          <h2>
            <button type="button" className="thread-aside-fold" aria-expanded={showHandled} onClick={toggleHandled}>
              <ChevronRight size={12} className="thread-aside-fold-icon" aria-hidden />
              Reviews · {handled.length}
            </button>
          </h2>
          {showHandled
            ? handled.map((r) => {
                // the icon and the outcome in the colour of the outcome's badge
                const outcome = outcomeOf(r);
                return (
                  <Link
                    key={r.id}
                    {...reviewLink(r)}
                    className={`thread-waiting-item is-ended tone-${outcome.tone}`}
                    data-testid="handled-review"
                  >
                    <ReviewIcon review={r} size={14} />
                    <span className="thread-waiting-item-text">
                      <span className="text">{r.title}</span>
                      <span className="faint" title={stamp(r.created_at)}>
                        <span className="thread-outcome">{outcome.label}</span> · {r.plugin} · {age(r.created_at)} ago
                      </span>
                    </span>
                    <StarMark review={r} />
                  </Link>
                );
              })
            : null}
        </section>
      ) : null}
      {session ? (
        <section>
          <h2>Session</h2>
          <dl className="thread-facts">
            <dt>Agent</dt>
            <dd>{session.agent}</dd>
            <dt>Id</dt>
            <dd className="mono">{session.id}</dd>
            {session.repo ? (
              <>
                <dt>Project</dt>
                <dd>{session.repo}</dd>
              </>
            ) : null}
            {session.ref ? (
              <>
                <dt>Branch</dt>
                <dd className="mono">{session.ref}</dd>
              </>
            ) : null}
            {session.folder ? (
              <>
                <dt>Folder</dt>
                <dd className="mono" title={session.folder}>
                  {session.folder}
                </dd>
              </>
            ) : null}
            <dt>Last activity</dt>
            <dd title={stamp(session.last_at)}>{age(session.last_at)} ago</dd>
          </dl>
        </section>
      ) : null}
    </aside>
  );
}

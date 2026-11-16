// The entries of a session's thread, from its start divider down: its
// questions, reviews, statuses, notes and the lines between them. Memoized,
// so that typing in the find bar, dragging the details edge or a notice for
// another session does not draw every entry again.

import { memo } from "react";
import type { To } from "react-router";
import { stamp } from "../../lib/format";
import { entryKey, type Entry, type FromSession } from "../../lib/sessions";
import {
  CommentMessage,
  FinishedLine,
  ItemEventLine,
  MessageBubble,
  NoteMessage,
  ReviewCard,
  RevisedLine,
  StartedLine,
  StatusGroup,
  WithdrawnLine,
} from "./messages";
import { QuestionSet } from "./QuestionSet";
import { RunLine, TurnEntry } from "./runLines";

type Props = {
  entries: Entry[];
  /** when the session started, for the divider at the top: only once every
   *  entry is drawn */
  startedAt: string | null;
  named: { agent: string; id: string } | null;
  agent: string | null;
  /** the questions the keys and the chat box answer */
  active: string | null;
  /** where the active questions put their chat box */
  footer: HTMLDivElement | null;
  /** the review shown in the side panel */
  openReview: string | null;
  panelMode: boolean;
  reviewTo: (id: string) => To;
  from: FromSession | undefined;
  load: () => void;
  onPick: (id: string) => void;
  showReview: (id: string) => void;
  openFromLink: (event: React.MouseEvent, id: string) => void;
};

export const ThreadEntries = memo(function ThreadEntries({
  entries,
  startedAt,
  named,
  agent,
  active,
  footer,
  openReview,
  panelMode,
  reviewTo,
  from,
  load,
  onPick,
  showReview,
  openFromLink,
}: Props) {
  return (
    <>
      {startedAt ? (
        <div className="thread-divider">
          <span>Session started {stamp(startedAt)}</span>
        </div>
      ) : null}
      {entries.map((entry) => {
        if (entry.kind === "message")
          return <MessageBubble key={entryKey(entry)} message={entry.message} session={named!} onDeleted={load} />;
        if (entry.kind === "statuses") return <StatusGroup key={entryKey(entry)} statuses={entry.statuses} />;
        if (entry.kind === "started") return <StartedLine key={entryKey(entry)} status={entry.status} />;
        if (entry.kind === "finished") return <FinishedLine key={entryKey(entry)} status={entry.status} />;
        if (entry.kind === "itemEvents") return <ItemEventLine key={entryKey(entry)} events={entry.events} />;
        if (entry.kind === "comment") return <CommentMessage key={entryKey(entry)} comment={entry.comment} />;
        if (entry.kind === "turn") return <TurnEntry key={entryKey(entry)} turn={entry.turn} agent={agent} />;
        if (entry.kind === "run") return <RunLine key={entryKey(entry)} run={entry.run} ended={entry.ended} />;
        if (entry.kind === "note")
          return <NoteMessage key={entryKey(entry)} status={entry.status} agent={agent} session={named?.id ?? null} />;
        if (entry.kind === "revised")
          return (
            <RevisedLine
              key={entryKey(entry)}
              entry={entry}
              onOpen={panelMode ? () => showReview(entry.card.id) : undefined}
            />
          );
        if (entry.kind === "withdrawn")
          return (
            <WithdrawnLine
              key={entryKey(entry)}
              entry={entry}
              onOpen={panelMode ? () => showReview(entry.card.id) : undefined}
            />
          );
        const item = entry.review;
        return item.kind === "question" ? (
          <QuestionSet
            key={entryKey(entry)}
            review={item}
            active={item.id === active}
            footer={item.id === active ? footer : null}
            onActivate={onPick}
            onDone={load}
          />
        ) : (
          <ReviewCard
            key={entryKey(entry)}
            review={item}
            round={entry.round}
            to={reviewTo(item.id)}
            state={panelMode ? undefined : from}
            open={item.id === openReview}
            onClick={openFromLink}
          />
        );
      })}
    </>
  );
});

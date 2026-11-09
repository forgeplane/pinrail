// A review and its rounds, loaded for the id the route names and again on
// every event about them. The last review stays on screen until the next
// one has loaded, and an answer for a review clicked past is dropped.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../../api/client";
import type { Review } from "../../api/types";
import { useNotice } from "../../state/live";

export function useReviewRounds(id: string) {
  const [review, setReview] = useState<Review | null>(null);
  const [rounds, setRounds] = useState<Review[]>([]);
  // the whole review this round revises, with its payload, for the view: the
  // rounds list holds summaries only
  const [revised, setRevised] = useState<Review | null>(null);
  // the other rounds still waiting that have never been opened: what is new
  const [unopened, setUnopened] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  // the review the route names now: a fetch for one clicked past lands late
  // and is dropped, so the screen never settles on a review nobody picked
  const wanted = useRef(id);
  useEffect(() => {
    wanted.current = id;
  }, [id]);
  /** Loads the review and its rounds; true when the review exists. */
  const load = useCallback(async (): Promise<boolean> => {
    try {
      const [r, rs] = await Promise.all([api.getReview(id), api.rounds(id).catch(() => [] as Review[])]);
      const before = r.revises ? await api.getReview(r.revises).catch(() => null) : null;
      const waiting = rs.filter((round) => round.id !== id && round.status === "pending");
      const opened = await Promise.all(
        waiting.map((round) =>
          api
            .events(round.id)
            .then((events) => events.some((e) => e.kind === "viewed"))
            .catch(() => true),
        ),
      );
      if (wanted.current !== id) return false;
      setReview(r);
      setRounds(rs);
      setRevised(before);
      setUnopened(new Set(waiting.filter((_, i) => !opened[i]).map((round) => round.id)));
      setError(null);
      return true;
    } catch (e) {
      if (wanted.current !== id) return false;
      setError(errorMessage(e, "The server did not answer."));
      return false;
    }
  }, [id]);

  useEffect(() => {
    // marked viewed only once the server knows the review
    // eslint-disable-next-line react-hooks/set-state-in-effect -- loads from the core
    load().then((found) => {
      if (found) api.markViewed(id).catch(() => {});
    });
  }, [id, load]);

  // Every event about this review refreshes it, and so does a new round of
  // it: that event names the new review, revising this one or a later round.
  const roundIds = useMemo(() => new Set([id, ...rounds.map((r) => r.id)]), [id, rounds]);
  useNotice((notice) => {
    const revises = notice.review?.revises;
    // a star on another round marks this one too
    const round = !!notice.review_id && roundIds.has(notice.review_id);
    if (notice.review_id === id || (revises && roundIds.has(revises)) || round) load();
  });

  const previous = useMemo(() => {
    if (!review?.revises) return null;
    if (revised?.id === review.revises) return revised;
    return rounds.find((r) => r.id === review.revises) ?? null;
  }, [review, rounds, revised]);
  const revisedBy = useMemo(() => rounds.find((r) => r.revises === review?.id) ?? null, [rounds, review]);
  // the newest round, when it is waiting and has never been opened
  const waiting = useMemo(() => {
    const last = rounds[rounds.length - 1];
    return last && unopened.has(last.id) ? { round: last, number: rounds.length } : null;
  }, [rounds, unopened]);

  return { review, setReview, rounds, unopened, previous, revisedBy, waiting, error, load, wanted };
}

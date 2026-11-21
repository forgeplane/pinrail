-- Each reply of an agent's turn, in the order it came. A turn has more
-- than one when it goes on after it finished, as when Claude Code replies
-- again in the same turn once a background job ends: each is a message of
-- its own, with its own time.
CREATE TABLE session_turn_replies (
  id        INTEGER PRIMARY KEY,
  turn_id   INTEGER NOT NULL REFERENCES session_turns (id) ON DELETE CASCADE,
  at        TEXT NOT NULL,
  -- at most 32,000 characters, and whether the reply was longer
  reply     TEXT NOT NULL,
  reply_cut INTEGER NOT NULL DEFAULT 0 CHECK (reply_cut IN (0, 1))
);
CREATE INDEX session_turn_replies_turn ON session_turn_replies (turn_id, id);

-- the reply each turn kept so far, at the time its turn ended
INSERT INTO session_turn_replies (turn_id, at, reply, reply_cut)
  SELECT x.turn_id, COALESCE(t.ended_at, t.started_at), x.reply, x.reply_cut
    FROM session_turn_texts x JOIN session_turns t ON t.id = x.turn_id
   WHERE x.reply IS NOT NULL;

ALTER TABLE session_turn_texts DROP COLUMN reply;
ALTER TABLE session_turn_texts DROP COLUMN reply_cut;

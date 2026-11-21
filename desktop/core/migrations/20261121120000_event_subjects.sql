-- What an event is about and who caused it. An event names the review, the
-- work item or the session it is about, and is deleted with it. The actor's
-- kind is person, agent or system, which the core checks: `actor` keeps the
-- name, and an agent's session is in actor_session. Older rows keep NULL in
-- each.
--
-- The table is made again with AUTOINCREMENT: once events go with their
-- subjects, the newest ones can be deleted, and a plain rowid would give
-- their ids again to the events after them, which a client that saw the
-- deleted ones would never receive.
CREATE TABLE events_subjects (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  kind          TEXT NOT NULL,
  at            TEXT NOT NULL,
  review_id     TEXT REFERENCES reviews(id) ON DELETE CASCADE,
  item_id       TEXT REFERENCES work_items(id) ON DELETE CASCADE,
  session_id    TEXT REFERENCES sessions(id) ON DELETE CASCADE,
  actor_kind    TEXT,
  actor         TEXT,
  actor_session TEXT,
  attrs         TEXT
);
INSERT INTO events_subjects (id, review_id, kind, actor, at, attrs)
  SELECT id, review_id, kind, actor, at, attrs FROM events;
DROP TABLE events;
ALTER TABLE events_subjects RENAME TO events;

CREATE INDEX events_review ON events(review_id, id);
CREATE INDEX events_item ON events(item_id, id) WHERE item_id IS NOT NULL;
CREATE INDEX events_session ON events(session_id, id) WHERE session_id IS NOT NULL;
-- for the sweep of old events
CREATE INDEX events_kind_at ON events(kind, at);

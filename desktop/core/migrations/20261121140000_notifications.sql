-- The person's notifications: one row for each review, work item or
-- session that has something to report. The title, the key and the status
-- are read from the subject, and the row is deleted with it.
CREATE TABLE notifications (
  id          INTEGER PRIMARY KEY,
  -- the subject: exactly one of the three
  review_id   TEXT REFERENCES reviews(id) ON DELETE CASCADE,
  item_id     TEXT REFERENCES work_items(id) ON DELETE CASCADE,
  session_id  TEXT REFERENCES sessions(id) ON DELETE CASCADE,
  -- the subject's repository, for the filter and the settings
  repo_id     INTEGER REFERENCES repos(id) ON DELETE CASCADE,
  -- the reason of the latest event, and whether the person must act
  reason      TEXT NOT NULL,
  waiting     INTEGER NOT NULL DEFAULT 0 CHECK (waiting IN (0, 1)),
  -- the latest event, which orders the list, and who caused it
  last_event  INTEGER NOT NULL,
  last_actor  TEXT,
  -- a line from the latest event, at most 200 characters
  summary     TEXT,
  -- the last event the person has seen, and how many came after it
  read_event  INTEGER NOT NULL DEFAULT 0,
  unread      INTEGER NOT NULL DEFAULT 0,
  updated_at  TEXT NOT NULL,
  -- set when the person marks it done, cleared by the next event
  done_at     TEXT,
  -- no event updates a muted notification
  muted       INTEGER NOT NULL DEFAULT 0 CHECK (muted IN (0, 1)),
  CHECK ((review_id IS NOT NULL) + (item_id IS NOT NULL) + (session_id IS NOT NULL) = 1)
);
CREATE UNIQUE INDEX notifications_review ON notifications(review_id) WHERE review_id IS NOT NULL;
CREATE UNIQUE INDEX notifications_item ON notifications(item_id) WHERE item_id IS NOT NULL;
CREATE UNIQUE INDEX notifications_session ON notifications(session_id) WHERE session_id IS NOT NULL;
-- the three lists: what waits, what is unread, and everything not done
CREATE INDEX notifications_waiting ON notifications(last_event DESC) WHERE waiting = 1;
CREATE INDEX notifications_unread ON notifications(last_event DESC)
  WHERE unread > 0 AND done_at IS NULL;
CREATE INDEX notifications_open ON notifications(last_event DESC) WHERE done_at IS NULL;
CREATE INDEX notifications_repo ON notifications(repo_id);

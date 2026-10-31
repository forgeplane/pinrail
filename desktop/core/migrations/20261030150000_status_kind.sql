-- A status is a step, the start of a task, which names the task and so
-- the session, or the end of the agent's work: `kind` takes the place of
-- the `done` flag. Nothing refers to the table, so it is rebuilt with the
-- same ids, which the events that announced the statuses name.
CREATE TABLE session_statuses_kind (
  id         INTEGER PRIMARY KEY,
  agent      TEXT NOT NULL,
  session    TEXT NOT NULL,
  text       TEXT NOT NULL,
  -- what the status says, such as `step`: the core checks the kinds, so
  -- a new one needs no migration
  kind       TEXT NOT NULL DEFAULT 'step',
  created_at TEXT NOT NULL,
  FOREIGN KEY (agent, session) REFERENCES sessions(agent, id) ON DELETE CASCADE
);
INSERT INTO session_statuses_kind (id, agent, session, text, kind, created_at)
  SELECT id, agent, session, text, CASE done WHEN 1 THEN 'done' ELSE 'step' END, created_at
  FROM session_statuses;
DROP TABLE session_statuses;
ALTER TABLE session_statuses_kind RENAME TO session_statuses;
CREATE INDEX session_statuses_session ON session_statuses(agent, session, id);
